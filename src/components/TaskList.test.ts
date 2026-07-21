import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, tick, unmount } from 'svelte';
import TaskList from './TaskList.svelte';
import { copy } from '../lib/i18n';
import type { TaskItem } from '../lib/types';

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path: string) => `asset://${encodeURIComponent(path)}`
}));

const task: TaskItem = {
  id: 'sample',
  source_path: 'C:\\images\\sample.Jpg',
  input_root: 'C:\\images',
  relative_path: 'sample.Jpg',
  name: 'sample.Jpg',
  format: 'jpeg',
  width: 800,
  height: 600,
  original_size: 1024,
  modified_ms: 1,
  status: 'ready',
  saved_bytes: 0
};

describe('TaskList', () => {
  beforeEach(() => {
    document.body.innerHTML = '<div id="test-root"></div>';
    vi.stubGlobal('ResizeObserver', class {
      observe() {}
      disconnect() {}
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    document.body.innerHTML = '';
  });

  it('renders the observed scroll container while the queue is empty', async () => {
    const app = mount(TaskList, {
      target: document.getElementById('test-root')!,
      props: {
        t: copy.en,
        ids: [],
        version: 0,
        formats: 'PNG / JPEG / WEBP',
        selectedId: '',
        language: 'en',
        busy: false,
        getItem: () => undefined,
        onSelect: () => undefined,
        onRemove: () => undefined,
        onRetry: () => undefined,
        onReveal: () => undefined
      }
    });
    await tick();

    expect(document.querySelector('.task-scroll.empty')).not.toBeNull();
    expect(document.body.textContent).toContain('Drop PNG / JPEG / WEBP images or folders');
    unmount(app);
  });

  it('loads visible task thumbnails lazily and decodes them asynchronously', async () => {
    const app = mount(TaskList, {
      target: document.getElementById('test-root')!,
      props: {
        t: copy.en,
        ids: [task.id],
        version: 1,
        formats: 'PNG / JPEG / WEBP',
        selectedId: task.id,
        language: 'en',
        busy: false,
        getItem: (id: string) => id === task.id ? task : undefined,
        onSelect: () => undefined,
        onRemove: () => undefined,
        onRetry: () => undefined,
        onReveal: () => undefined
      }
    });
    await tick();

    const thumbnail = document.querySelector<HTMLImageElement>('.thumbnail img');
    expect(thumbnail?.getAttribute('src')).toBe('asset://C%3A%5Cimages%5Csample.Jpg');
    expect(thumbnail?.getAttribute('decoding')).toBe('async');
    expect(thumbnail?.getAttribute('loading')).toBe('lazy');
    unmount(app);
  });
});
