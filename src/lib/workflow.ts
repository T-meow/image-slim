import { get } from 'svelte/store';
import { validSubfolderName } from './format';
import { errorText, interpolate, type Messages } from './i18n';
import type { PreviewController } from './preview-controller';
import type { QueueController } from './queue';
import { SessionController, sessionBusy } from './session';
import { cancelBatch, cancelScan, normalizeAppError, startBatch, startScan } from './tauri';
import { toInputItem, type AppCapabilities, type BatchRequest, type BatchSettings, type BatchSummary, type InputItem, type ItemProgress, type ScanEvent, type TaskItem } from './types';

interface WorkflowOptions {
  settings: () => BatchSettings;
  messages: () => Messages;
  capabilities: () => AppCapabilities;
  confirm: (request: BatchRequest, count: number) => Promise<boolean>;
}

interface Rescan {
  originals: Map<string, TaskItem>;
  refreshed: Map<string, InputItem>;
  pending: InputItem[];
}

export class WorkflowController {
  private rescan?: Rescan;
  private pendingProgress: ItemProgress[] = [];
  private pendingSummaries: BatchSummary[] = [];

  constructor(
    private readonly queue: QueueController,
    private readonly session: SessionController,
    private readonly preview: Pick<PreviewController, 'cancel'>,
    private readonly options: WorkflowOptions
  ) {}

  async addPaths(paths: string[]): Promise<void> {
    if (!paths.length || !this.canStart()) return;
    await this.scan(paths);
  }

  async runReady(): Promise<void> {
    if (!this.canStart()) return;
    const items = this.queue.readyItems();
    const retries = items.filter((item) => item.status !== 'ready');
    if (retries.length) {
      await this.rescanAndRun(retries, items.filter((item) => item.status === 'ready').map(toInputItem));
    } else {
      await this.runBatch(items.map(toInputItem));
    }
  }

  async retryItems(ids: readonly string[]): Promise<void> {
    if (!this.canStart()) return;
    const items = ids.flatMap((id) => {
      const item = this.queue.get(id);
      return item ? [item] : [];
    });
    if (items.length) await this.rescanAndRun(items);
  }

  handleScan(event: ScanEvent): void {
    if (event.scan_id !== get(this.session).activeScanId) return;
    if (event.type === 'items') {
      if (this.rescan) {
        for (const input of event.items) {
          const previous = this.rescan.originals.get(pathKey(input.source_path));
          if (previous) this.rescan.refreshed.set(previous.id, {
            ...input, input_root: previous.input_root, relative_path: previous.relative_path
          });
        }
      } else {
        this.queue.merge(event.items);
      }
      return;
    }
    if (event.type === 'issues') {
      this.session.addIssues(event.issues);
      return;
    }
    if (event.type === 'progress') {
      this.session.updateScan(event.visited, event.accepted, event.current_path);
      return;
    }

    const rescan = this.rescan;
    this.rescan = undefined;
    this.session.finishScan(event.scan_id);
    const t = this.options.messages();
    let notice = event.cancelled ? t.scanCancelled : interpolate(t.summaryQueued, { count: event.accepted });
    if (!event.cancelled && event.issue_count) notice += interpolate(t.summaryWarnings, { count: event.issue_count });
    if (rescan && !event.cancelled) notice = interpolate(t.rescanFinished, { count: rescan.refreshed.size, failed: rescan.originals.size - rescan.refreshed.size });
    this.session.setNotice(notice);
    if (rescan && !event.cancelled) {
      const inputs = [...rescan.pending, ...rescan.refreshed.values()];
      if (inputs.length) void this.runBatch(inputs);
    }
  }

  handleProgress(progress: ItemProgress): void {
    const state = get(this.session);
    if (!state.activeBatchId) {
      if (state.starting) this.pendingProgress.push(progress);
      return;
    }
    if (progress.batch_id !== state.activeBatchId) return;
    this.queue.update(progress);
    if (progress.error) this.session.addIssues([progress.error]);
    if (['completed', 'unchanged', 'failed', 'cancelled'].includes(progress.status)) {
      this.session.itemFinished(progress.item_id);
    }
  }

  handleSummary(summary: BatchSummary): void {
    const state = get(this.session);
    if (!state.activeBatchId) {
      if (state.starting) this.pendingSummaries.push(summary);
      return;
    }
    if (summary.batch_id === state.activeBatchId) this.session.finishBatch(summary);
  }

  async cancel(): Promise<void> {
    const state = get(this.session);
    if (state.stopping || (!state.activeScanId && !state.activeBatchId)) return;
    this.session.requestStop();
    try {
      if (state.activeScanId) await cancelScan(state.activeScanId);
      if (state.activeBatchId) await cancelBatch(state.activeBatchId);
    } catch (error) {
      this.session.clearStopping();
      this.report(error, 'cancel');
    }
  }

  private canStart(): boolean {
    const settings = this.options.settings();
    return !sessionBusy(get(this.session))
      && ((settings.output_mode === 'overwrite' && !this.queue.hasAudio) || validSubfolderName(settings.output_subfolder));
  }

  private async rescanAndRun(items: TaskItem[], pending: InputItem[] = []): Promise<void> {
    this.rescan = {
      originals: new Map(items.map((item) => [pathKey(item.source_path), { ...item }])),
      refreshed: new Map(),
      pending
    };
    await this.scan(items.map((item) => item.source_path));
  }

  private async scan(paths: string[]): Promise<void> {
    const scanId = crypto.randomUUID();
    const replacements = new Set([...(this.rescan?.originals.values() ?? [])].map((item) => item.id));
    const settings = this.options.settings();
    this.preview.cancel();
    this.session.beginScan(scanId);
    try {
      await startScan({
        scan_id: scanId,
        paths,
        output_subfolder: validSubfolderName(settings.output_subfolder) ? settings.output_subfolder.trim() : 'compressed',
        existing_ids: this.queue.existingIds().filter((id) => !replacements.has(id)),
        remaining_capacity: Math.max(0, this.options.capabilities().limits.max_queue_items - this.queue.count + replacements.size)
      });
    } catch (error) {
      if (get(this.session).activeScanId !== scanId) return;
      this.rescan = undefined;
      this.session.finishScan(scanId);
      this.report(error, 'scan');
    }
  }

  private async runBatch(inputs: InputItem[]): Promise<void> {
    if (!inputs.length || !this.canStart()) return;
    const settings = { ...this.options.settings() };
    settings.output_subfolder = settings.output_subfolder.trim();
    const request: BatchRequest = { ...settings, items: inputs.map(toInputItem), allow_conflicts: false };
    this.session.beginStarting();
    this.preview.cancel();
    this.pendingProgress = [];
    this.pendingSummaries = [];
    try {
      let result = await startBatch(request);
      if (result.status === 'conflicts') {
        if (!await this.options.confirm(request, result.conflict_count)) return;
        result = await startBatch({ ...request, allow_conflicts: true });
      }
      if (result.status !== 'started' || !result.batch_id) throw new Error('Batch did not return a valid start result');
      this.queue.beginAttempt(inputs, settings);
      this.session.beginBatch(result.batch_id, inputs.length);
      this.pendingProgress.forEach((event) => this.handleProgress(event));
      this.pendingSummaries.forEach((event) => this.handleSummary(event));
    } catch (error) {
      this.report(error, 'start');
    } finally {
      this.session.finishStarting();
      this.pendingProgress = [];
      this.pendingSummaries = [];
    }
  }

  private report(error: unknown, operation: 'scan' | 'start' | 'cancel'): void {
    const appError = normalizeAppError(error);
    const t = this.options.messages();
    this.session.addIssues([appError]);
    this.session.setNotice(`${t.operationErrors[operation]}: ${errorText(appError, t)}`, true);
  }
}

function pathKey(path: string): string {
  return path.replace(/\//g, '\\').toLowerCase();
}
