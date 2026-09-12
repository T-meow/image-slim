/** @vitest-environment jsdom */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, tick, unmount } from 'svelte';
import type { Component } from 'svelte';
import App from './App.svelte';
import { PREVIEW_IDLE_DELAY_MS } from './lib/preview-controller';
import type { AppCapabilities, InputItem, ScanEvent, ScanRequest } from './lib/types';

type EventHandler = (event: { payload: unknown }) => void;

const mocks = vi.hoisted(() => ({
  eventHandlers: new Map<string, EventHandler>(),
  invoke: vi.fn(),
  open: vi.fn()
}));

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path: string) => `asset://${encodeURIComponent(path)}`,
  invoke: mocks.invoke,
  isTauri: () => true
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (eventName: string, handler: EventHandler) => {
    mocks.eventHandlers.set(eventName, handler);
    return () => mocks.eventHandlers.delete(eventName);
  })
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    onDragDropEvent: vi.fn(async () => () => undefined)
  })
}));

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn(async () => '0.2.0')
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  confirm: vi.fn(async () => false),
  message: vi.fn(async () => undefined),
  open: mocks.open
}));

const capabilities: AppCapabilities = {
  formats: [
    { format: 'png', extensions: ['png'] },
    { format: 'jpeg', extensions: ['jpg', 'jpeg'] },
    { format: 'webp', extensions: ['webp'] }
  ],
  presets: ['lossless', 'balanced', 'strong'],
  limits: {
    max_file_bytes: 512 * 1024 * 1024,
    max_pixels: 100_000_000,
    max_dimension: 65_535,
    max_queue_items: 10_000
  }
};

const input: InputItem = {
  id: 'sample-id',
  source_path: 'C:\\images\\sample.png',
  input_root: 'C:\\images',
  relative_path: 'sample.png',
  name: 'sample.png',
  format: 'png',
  width: 800,
  height: 600,
  original_size: 1024,
  modified_ms: 1
};

beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  localStorage.clear();
  localStorage.setItem('image-slim-language', 'en');
  mocks.eventHandlers.clear();
  mocks.open.mockReset().mockResolvedValue(input.source_path);
  mocks.invoke.mockReset().mockImplementation(async (command: string) => {
    if (command === 'get_capabilities') return capabilities;
    if (command === 'cancel_preview') return false;
    if (command === 'start_batch') return { status: 'started', batch_id: 'batch', conflict_count: 0 };
    if (command === 'create_preview') {
      return {
        source_preview_path: 'C:\\cache\\source.png',
        candidate_preview_path: 'C:\\cache\\candidate.png',
        source_size: 1024,
        candidate_size: 800,
        would_replace: true,
        cache_key: 'preview',
        width: 800,
        height: 600
      };
    }
    return undefined;
  });
  vi.stubGlobal('matchMedia', () => ({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn()
  }));
  vi.stubGlobal('ResizeObserver', class {
    observe() {}
    disconnect() {}
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = '';
});

describe('App queue workflow', () => {
  it('renders streamed scan items, enables processing, and clears the queue', async () => {
    const app = mount(App as Component, { target: document.getElementById('app')! });
    try {
      const addButton = buttonByText('Add files');
      expect(addButton.disabled).toBe(true);

      await tick();
      await vi.waitFor(() => expect(mocks.eventHandlers.size).toBe(3));
      await vi.waitFor(() => expect(addButton.disabled).toBe(false));

      addButton.click();
      await vi.waitFor(() => {
        expect(mocks.invoke).toHaveBeenCalledWith('start_scan', expect.any(Object));
      });
      const scanCall = mocks.invoke.mock.calls.find(([command]) => command === 'start_scan');
      const request = (scanCall?.[1] as { request: ScanRequest }).request;

      emitScan({ type: 'items', scan_id: request.scan_id, items: [input] });
      await tick();
      expect(document.body.textContent).toContain('sample.png');
      await new Promise((resolve) => setTimeout(resolve, PREVIEW_IDLE_DELAY_MS + 25));
      expect(mocks.invoke).not.toHaveBeenCalledWith('create_preview', expect.anything());

      emitScan({
        type: 'finished',
        scan_id: request.scan_id,
        accepted: 1,
        issue_count: 0,
        cancelled: false,
        limit_reached: false
      });
      await tick();

      const startButton = buttonByText('Compress');
      expect(startButton.disabled).toBe(false);
      expect(document.querySelector('.task-row')).not.toBeNull();
      await vi.waitFor(() => {
        expect(mocks.invoke).toHaveBeenCalledWith('create_preview', expect.any(Object));
      }, { timeout: PREVIEW_IDLE_DELAY_MS + 500 });

      const clearButton = document.querySelector<HTMLButtonElement>('button[aria-label="Clear"]');
      expect(clearButton).not.toBeNull();
      clearButton!.click();
      await tick();

      expect(document.querySelector('.task-row')).toBeNull();
      expect(document.body.textContent).toContain('Drop images, audio, or folders');
    } finally {
      unmount(app);
    }
  });

  it('retains completed output and totals when changing the next-run settings', async () => {
    const app = mount(App as Component, { target: document.getElementById('app')! });
    try {
      await addImage();
      buttonByText('Compress').click();
      await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('start_batch', expect.any(Object)));
      await tick();
      finishBatch();
      await tick();
      const before = document.querySelector('.task-size')!.textContent;
      expect(document.querySelector('.task-state')?.textContent).toContain('Completed');
      expect(document.querySelector<HTMLButtonElement>('.start-button')?.disabled).toBe(true);

      const folder = document.querySelector<HTMLInputElement>('.folder-input input')!;
      folder.value = 'exports';
      folder.dispatchEvent(new Event('input', { bubbles: true }));
      buttonByText('Strong').click();
      await tick();
      expect(document.querySelector('.task-size')!.textContent).toBe(before);
      expect(document.querySelector('.task-state')?.textContent).toContain('Completed');
      expect(document.querySelector('button[aria-label="Show in folder"]')).not.toBeNull();
      expect(document.querySelector('.result-summary')?.textContent).toContain('Saved 224');
      expect(document.querySelector<HTMLButtonElement>('.start-button')?.disabled).toBe(true);
      expect(mocks.invoke.mock.calls.filter(([command]) => command === 'start_batch')).toHaveLength(1);

      buttonByText('View results').click();
      await tick();
      expect(document.querySelector('.queue-filters button.active')?.textContent).toContain('Completed');
      buttonByText('Clear completed').click();
      await tick();
      expect(document.querySelector('.task-row')).toBeNull();
      expect(document.querySelector('.result-summary')).toBeNull();
    } finally { await unmount(app); }
  });

  it('does not generate a new comparison against the stale source after overwriting it', async () => {
    localStorage.setItem('image-slim-output-mode', 'overwrite');
    const app = mount(App as Component, { target: document.getElementById('app')! });
    try {
      await addImage();
      buttonByText('Compress').click();
      await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('start_batch', expect.any(Object)));
      await tick();
      finishBatch(input.source_path);
      await tick();
      const previews = mocks.invoke.mock.calls.filter(([command]) => command === 'create_preview').length;
      await new Promise((resolve) => setTimeout(resolve, PREVIEW_IDLE_DELAY_MS + 25));
      expect(mocks.invoke.mock.calls.filter(([command]) => command === 'create_preview')).toHaveLength(previews);
      expect(document.body.textContent).toContain('The original was replaced by this result');
      expect(document.querySelector('.compare-handle')).toBeNull();
    } finally { await unmount(app); }
  });

  it('uses audio settings and players while preserving the saved attempt and image overwrite behavior', async () => {
    localStorage.setItem('image-slim-output-mode', 'overwrite');
    localStorage.setItem('image-slim-audio-bitrate', '64');
    const audio: InputItem = { ...input, id: 'audio', name: 'speech.wav', format: 'wav',
      source_path: 'C:\\audio\\speech.wav', input_root: 'C:\\audio', relative_path: 'speech.wav',
      width: 0, height: 0, audio: { sample_rate: 44100, channels: 2, duration_ms: 2000 } };
    const app = mount(App as Component, { target: document.getElementById('app')! });
    const pause = vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
    try {
      await addImage(audio);
      expect(document.querySelector('.thumbnail img')).toBeNull();
      expect(document.querySelector('.audio-preview')).not.toBeNull();
      const bitrate = document.querySelector<HTMLSelectElement>('select[aria-label="MP3 bitrate"]')!;
      expect(bitrate.value).toBe('64');
      bitrate.value = '192';
      bitrate.dispatchEvent(new Event('change', { bubbles: true }));
      await tick();
      const folder = document.querySelector<HTMLInputElement>('.folder-input input')!;
      folder.value = '..';
      folder.dispatchEvent(new Event('input', { bubbles: true }));
      await tick();
      expect(document.querySelector<HTMLButtonElement>('.start-button')?.disabled).toBe(true);
      folder.value = 'compressed';
      folder.dispatchEvent(new Event('input', { bubbles: true }));
      await tick();
      const originalPlayer = document.querySelector('audio');
      buttonByText('Compress').click();
      await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('start_batch', expect.objectContaining({
        request: expect.objectContaining({ audio_bitrate_kbps: 192, output_mode: 'overwrite', items: [audio] })
      })));
      await tick();
      expect(pause.mock.instances).toContain(originalPlayer);
      mocks.eventHandlers.get('batch-item')!({ payload: { batch_id: 'batch', item_id: audio.id, status: 'completed', output_path: 'C:\\audio\\compressed\\speech.wav.mp3', output_size: 800, saved_bytes: 224, error: null } });
      mocks.eventHandlers.get('batch-summary')!({ payload: { batch_id: 'batch', completed: 1, unchanged: 0, failed: 0, cancelled: 0, original_bytes: 1024, output_bytes: 800 } });
      await tick();
      expect(document.querySelectorAll('audio')).toHaveLength(2);
      expect(document.body.textContent).not.toContain('The original was replaced');
      const players = [...document.querySelectorAll('audio')];
      players[0].dispatchEvent(new Event('play'));
      expect(pause.mock.instances[pause.mock.instances.length - 1]).toBe(players[1]);
      players[1].dispatchEvent(new Event('play'));
      expect(pause.mock.instances[pause.mock.instances.length - 1]).toBe(players[0]);
      bitrate.value = '64';
      bitrate.dispatchEvent(new Event('change', { bubbles: true }));
      await tick();
      expect(document.querySelectorAll('.audio-player')[1].textContent).toContain('192 kbps');
      await new Promise((resolve) => setTimeout(resolve, PREVIEW_IDLE_DELAY_MS + 25));
      expect(mocks.invoke).not.toHaveBeenCalledWith('create_preview', expect.anything());
    } finally { await unmount(app); pause.mockRestore(); }
  });
});

async function addImage(item: InputItem = input) {
  await tick();
  await vi.waitFor(() => expect(buttonByText('Add files').disabled).toBe(false));
  buttonByText('Add files').click();
  await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('start_scan', expect.any(Object)));
  const request = (mocks.invoke.mock.calls.find(([command]) => command === 'start_scan')![1] as { request: ScanRequest }).request;
  emitScan({ type: 'items', scan_id: request.scan_id, items: [item] });
  emitScan({ type: 'finished', scan_id: request.scan_id, accepted: 1, issue_count: 0, cancelled: false, limit_reached: false });
  await tick();
}

function finishBatch(outputPath = 'C:\\images\\compressed\\sample.png') {
  mocks.eventHandlers.get('batch-item')!({ payload: { batch_id: 'batch', item_id: input.id, status: 'completed', output_path: outputPath, output_size: 800, saved_bytes: 224, error: null } });
  mocks.eventHandlers.get('batch-summary')!({ payload: { batch_id: 'batch', completed: 1, unchanged: 0, failed: 0, cancelled: 0, original_bytes: 1024, output_bytes: 800 } });
}

function emitScan(event: ScanEvent): void {
  const handler = mocks.eventHandlers.get('scan-event');
  if (!handler) throw new Error('scan-event listener was not registered');
  handler({ payload: event });
}

function buttonByText(text: string): HTMLButtonElement {
  const button = [...document.querySelectorAll<HTMLButtonElement>('button')]
    .find((candidate) => candidate.textContent?.includes(text));
  if (!button) throw new Error(`Button not found: ${text}`);
  return button;
}
