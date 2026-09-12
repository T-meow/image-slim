import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { copy } from './i18n';
import { QueueController } from './queue';
import { SessionController } from './session';
import { WorkflowController } from './workflow';
import type { AppError, BatchSettings, BatchStartResult, InputItem, ItemProgress, ScanRequest } from './types';

const ipc = vi.hoisted(() => ({ startScan: vi.fn(), startBatch: vi.fn(), cancelScan: vi.fn(), cancelBatch: vi.fn() }));
vi.mock('./tauri', () => ({ ...ipc, normalizeAppError: (error: AppError) => error }));

const input: InputItem = {
  id: 'a', source_path: 'C:\\images\\nested\\a.png', input_root: 'C:\\images', relative_path: 'nested\\a.png',
  name: 'a.png', format: 'png', width: 100, height: 100, original_size: 1000, modified_ms: 1
};
const error: AppError = { code: 'not_found', path: input.source_path, detail: null, params: {}, retryable: true };
const completed: ItemProgress = { batch_id: 'batch', item_id: 'a', status: 'completed', output_path: 'C:\\images\\compressed\\nested\\a.png', output_size: 600, saved_bytes: 400, error: null };
const summary = { batch_id: 'batch', completed: 1, unchanged: 0, failed: 0, cancelled: 0, original_bytes: 1000, output_bytes: 600 };

function fixture() {
  const queue = new QueueController();
  queue.merge([input]);
  const session = new SessionController();
  const settings: BatchSettings = { preset: 'balanced', output_mode: 'subfolder', output_subfolder: 'compressed', metadata_policy: 'essential' };
  const confirm = vi.fn(async () => false);
  const workflow = new WorkflowController(queue, session, { cancel: vi.fn() }, {
    settings: () => settings, messages: () => copy.en, confirm,
    capabilities: () => ({ formats: [], presets: [], limits: { max_file_bytes: 1e9, max_pixels: 1e8, max_dimension: 65535, max_queue_items: 1 } })
  });
  return { queue, session, settings, confirm, workflow };
}

function scanRequest(): ScanRequest {
  return ipc.startScan.mock.calls[ipc.startScan.mock.calls.length - 1][0];
}

function finishScan(workflow: WorkflowController, options: { cancelled?: boolean; accepted?: number; issueCount?: number } = {}) {
  workflow.handleScan({ type: 'finished', scan_id: scanRequest().scan_id, accepted: options.accepted ?? 1, issue_count: options.issueCount ?? 0, cancelled: options.cancelled ?? false, limit_reached: false });
}

function scanned(workflow: WorkflowController) {
  workflow.handleScan({ type: 'items', scan_id: scanRequest().scan_id, items: [{ ...input, input_root: 'C:\\images\\nested', relative_path: 'a.png', original_size: 1100, modified_ms: 2 }] });
}

beforeEach(() => {
  vi.resetAllMocks();
  ipc.startScan.mockResolvedValue(undefined);
  ipc.startBatch.mockResolvedValue({ status: 'started', batch_id: 'batch', conflict_count: 0 });
});

describe('WorkflowController', () => {
  it('keeps failed rows when a rescan cannot read the file, including at queue capacity', async () => {
    const { workflow, queue, session } = fixture();
    queue.update({ ...completed, status: 'failed', error, output_path: null, output_size: null, saved_bytes: 0 });
    const previous = { ...queue.get('a')! };
    await workflow.retryItems(['a']);
    expect(queue.get('a')).toEqual(previous);
    expect(scanRequest()).toMatchObject({ existing_ids: [], remaining_capacity: 1 });
    workflow.handleScan({ type: 'issues', scan_id: scanRequest().scan_id, issues: [error] });
    finishScan(workflow, { accepted: 0, issueCount: 1 });
    expect(queue.get('a')).toEqual(previous);
    expect(queue.count).toBe(1);
    expect(get(session).scanning).toBe(false);
    expect(get(session).issues).toContain(error);
    expect(ipc.startBatch).not.toHaveBeenCalled();
  });

  it('keeps saved output and stops the retry when scanning is cancelled after finding the file', async () => {
    const { workflow, queue } = fixture();
    queue.update(completed);
    const previous = { ...queue.get('a')! };
    await workflow.retryItems(['a']);
    scanned(workflow);
    finishScan(workflow, { cancelled: true });
    expect(queue.get('a')).toEqual(previous);
    expect(ipc.startBatch).not.toHaveBeenCalled();
  });

  it('keeps prior results when the user declines replacing an existing output', async () => {
    const { workflow, queue, session, confirm } = fixture();
    queue.update(completed);
    const previous = { ...queue.get('a')! };
    ipc.startBatch.mockResolvedValue({ status: 'conflicts', batch_id: null, conflict_count: 1 });
    await workflow.retryItems(['a']);
    scanned(workflow);
    finishScan(workflow);
    await vi.waitFor(() => expect(confirm).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(get(session).starting).toBe(false));
    expect(queue.get('a')).toEqual(previous);
    expect(ipc.startBatch).toHaveBeenCalledOnce();
  });

  it('refreshes only the requested rows after a successful start and retains directory mappings', async () => {
    const { workflow, queue } = fixture();
    queue.merge([{ ...input, id: 'b', source_path: 'C:\\images\\b.png', relative_path: 'b.png', name: 'b.png' }]);
    queue.update({ ...completed, status: 'failed', output_path: null, output_size: null, saved_bytes: 0, error });
    await workflow.retryItems(['a']);
    scanned(workflow);
    finishScan(workflow);
    await vi.waitFor(() => expect(queue.get('a')?.modified_ms).toBe(2));
    expect(ipc.startBatch.mock.calls[0][0].items).toEqual([{ ...input, original_size: 1100, modified_ms: 2 }]);
    expect(queue.get('b')?.status).toBe('ready');
    expect(queue.get('a')).toMatchObject({ input_root: 'C:\\images', relative_path: 'nested\\a.png', status: 'ready', saved_bytes: 0 });
    expect(queue.get('a')?.error).toBeUndefined();
  });

  it('locks repeated starts and imports, buffers early events, and records a settings snapshot', async () => {
    const { workflow, queue, session, settings } = fixture();
    let resolveStart!: (result: BatchStartResult) => void;
    ipc.startBatch.mockImplementation(() => new Promise<BatchStartResult>((resolve) => resolveStart = resolve));
    const start = workflow.runReady();
    expect(get(session).starting).toBe(true);
    await workflow.runReady();
    await workflow.addPaths(['C:\\more']);
    expect(ipc.startBatch).toHaveBeenCalledOnce();
    expect(ipc.startScan).not.toHaveBeenCalled();
    workflow.handleProgress(completed);
    workflow.handleSummary(summary);
    settings.preset = 'strong';
    settings.output_subfolder = 'exports';
    resolveStart({ status: 'started', batch_id: 'batch', conflict_count: 0 });
    await start;
    expect(queue.get('a')).toMatchObject({ status: 'completed', output_size: 600, attempt: { preset: 'balanced', output_subfolder: 'compressed' } });
    expect(get(session)).toMatchObject({ starting: false, running: false, lastSummary: summary });
    expect(ipc.startBatch.mock.calls[0][0].items[0]).not.toHaveProperty('attempt');
  });

  it('uses the same settings after confirming conflicts and drops events received while idle', async () => {
    const { workflow, queue, settings, confirm } = fixture();
    workflow.handleProgress(completed);
    workflow.handleSummary(summary);
    ipc.startBatch.mockResolvedValueOnce({ status: 'conflicts', batch_id: null, conflict_count: 1 });
    confirm.mockImplementation(async () => { settings.preset = 'strong'; return true; });
    await workflow.runReady();
    expect(ipc.startBatch).toHaveBeenCalledTimes(2);
    expect(ipc.startBatch.mock.calls[1][0]).toMatchObject({ preset: 'balanced', allow_conflicts: true });
    expect(queue.get('a')?.status).toBe('ready');
  });

  it('keeps the task when the scan command fails and allows another retry', async () => {
    const { workflow, queue, session } = fixture();
    queue.update(completed);
    ipc.startScan.mockRejectedValueOnce(error);
    await workflow.retryItems(['a']);
    expect(queue.get('a')?.output_size).toBe(600);
    expect(get(session)).toMatchObject({ scanning: false, noticeIsError: true });
    await workflow.retryItems(['a']);
    expect(ipc.startScan).toHaveBeenCalledTimes(2);
  });
});
