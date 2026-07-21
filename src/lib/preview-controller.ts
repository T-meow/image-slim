import { writable } from 'svelte/store';
import { cancelPreview, createPreview, inTauri, normalizeAppError } from './tauri';
import type { AppError, MetadataPolicy, CompressionPreset, PreviewResult, TaskItem } from './types';

export interface PreviewState {
  result?: PreviewResult;
  loading: boolean;
  error?: AppError;
}

export const PREVIEW_IDLE_DELAY_MS = 400;

export class PreviewController {
  private readonly store = writable<PreviewState>({ loading: false });
  private timer: number | undefined;
  private token = 0;
  readonly subscribe = this.store.subscribe;

  constructor(private readonly onError?: (error: AppError) => void) {}

  schedule(item: TaskItem | undefined, preset: CompressionPreset, metadataPolicy: MetadataPolicy): void {
    window.clearTimeout(this.timer);
    const token = ++this.token;
    void cancelPreview();
    this.store.set({ loading: false });
    if (!item || !inTauri) return;
    this.timer = window.setTimeout(
      () => void this.load(item, preset, metadataPolicy, token),
      PREVIEW_IDLE_DELAY_MS
    );
  }

  cancel(): void {
    window.clearTimeout(this.timer);
    this.token += 1;
    void cancelPreview();
    this.store.set({ loading: false });
  }

  dispose(): void {
    this.cancel();
  }

  private async load(
    item: TaskItem,
    preset: CompressionPreset,
    metadataPolicy: MetadataPolicy,
    token: number
  ): Promise<void> {
    this.store.set({ loading: true });
    try {
      const result = await createPreview({
        request_id: String(token),
        item: stripTaskState(item),
        preset,
        metadata_policy: metadataPolicy
      });
      if (token === this.token) this.store.set({ loading: false, result });
    } catch (error) {
      if (token === this.token) {
        const appError = normalizeAppError(error);
        this.store.set({ loading: false, error: appError });
        this.onError?.(appError);
      }
    }
  }
}

function stripTaskState(item: TaskItem) {
  const { status: _status, output_path: _outputPath, output_size: _outputSize, saved_bytes: _savedBytes, error: _error, ...input } = item;
  return input;
}
