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
  getVersion: vi.fn(async () => '0.1.0')
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
      const addButton = buttonByText('Add images');
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
      expect(document.body.textContent).toContain('Drop PNG / JPEG / WEBP images or folders');
    } finally {
      unmount(app);
    }
  });
});

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
