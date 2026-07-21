import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import type { PreviewResult, TaskItem } from './types';

const mocks = vi.hoisted(() => ({
  cancelPreview: vi.fn(async () => true),
  createPreview: vi.fn()
}));

vi.mock('./tauri', () => ({
  cancelPreview: mocks.cancelPreview,
  createPreview: mocks.createPreview,
  inTauri: true,
  normalizeAppError: (error: unknown) => error
}));

import { PREVIEW_IDLE_DELAY_MS, PreviewController } from './preview-controller';

function item(id: string): TaskItem {
  return {
    id,
    source_path: `C:\\images\\${id}.png`,
    input_root: 'C:\\images',
    relative_path: `${id}.png`,
    name: `${id}.png`,
    format: 'png',
    width: 2,
    height: 2,
    original_size: 100,
    modified_ms: 1,
    status: 'ready',
    saved_bytes: 0
  };
}

function result(key: string): PreviewResult {
  return {
    source_preview_path: `C:\\cache\\${key}-source.png`,
    candidate_preview_path: `C:\\cache\\${key}-candidate.png`,
    source_size: 100,
    candidate_size: 80,
    would_replace: true,
    cache_key: key,
    width: 2,
    height: 2
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal('window', globalThis);
  mocks.cancelPreview.mockClear();
  mocks.createPreview.mockReset();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('PreviewController', () => {
  it('ignores a non-immediate stale encoder result after a rapid switch', async () => {
    const first = deferred<PreviewResult>();
    const second = deferred<PreviewResult>();
    mocks.createPreview
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);
    const controller = new PreviewController();

    controller.schedule(item('first'), 'balanced', 'essential');
    await vi.advanceTimersByTimeAsync(PREVIEW_IDLE_DELAY_MS);
    controller.schedule(item('second'), 'balanced', 'essential');
    await vi.advanceTimersByTimeAsync(PREVIEW_IDLE_DELAY_MS);
    second.resolve(result('second'));
    await Promise.resolve();
    first.resolve(result('first'));
    await Promise.resolve();

    expect(mocks.cancelPreview).toHaveBeenCalledTimes(2);
    expect(mocks.createPreview).toHaveBeenCalledTimes(2);
    expect(get(controller).result?.cache_key).toBe('second');
  });

  it('cancels a scheduled preview before it reaches IPC', async () => {
    const controller = new PreviewController();
    controller.schedule(item('pending'), 'balanced', 'essential');
    controller.cancel();
    await vi.advanceTimersByTimeAsync(500);

    expect(mocks.createPreview).not.toHaveBeenCalled();
    expect(mocks.cancelPreview).toHaveBeenCalledTimes(2);
    expect(get(controller)).toEqual({ loading: false });
  });
});
