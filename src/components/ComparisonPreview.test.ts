import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, tick, unmount } from 'svelte';
import ComparisonPreview from './ComparisonPreview.svelte';
import { copy } from '../lib/i18n';
import type { PreviewResult, TaskItem } from '../lib/types';

vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path: string) => `asset://${encodeURIComponent(path)}` }));
const item: TaskItem = { id: 'a', source_path: 'C:\\a.png', input_root: 'C:\\', relative_path: 'a.png', name: 'a.png', format: 'png', width: 4000, height: 3000, original_size: 1000, modified_ms: 1, status: 'ready', saved_bytes: 0 };
const result: PreviewResult = { source_preview_path: 'C:\\cache\\source.png', candidate_preview_path: 'C:\\cache\\result.png', source_size: 1000, candidate_size: 600, would_replace: true, cache_key: 'a', width: 4000, height: 3000 };
let app: ReturnType<typeof mount>;

beforeEach(() => {
  document.body.innerHTML = '<div id="test-root"></div>';
  vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
});
afterEach(async () => { await unmount(app); vi.unstubAllGlobals(); document.body.innerHTML = ''; });

describe('ComparisonPreview', () => {
  it('moves the divider by pointer and keyboard, clamps positions, and ends cancelled drags', async () => {
    const onCompare = vi.fn();
    app = mount(ComparisonPreview, { target: document.getElementById('test-root')!, props: { t: copy.en, item, result, error: undefined, language: 'en', onCompare, onZoom: vi.fn() } });
    await tick();
    const canvas = document.querySelector<HTMLDivElement>('.preview-canvas')!;
    canvas.getBoundingClientRect = () => ({ left: 100, width: 800 } as DOMRect);
    const handle = document.querySelector<HTMLButtonElement>('.compare-handle')!;
    handle.setPointerCapture = vi.fn();
    function pointer(type: string, clientX: number) {
      const event = new MouseEvent(type, { clientX, bubbles: true, button: 0 });
      Object.defineProperty(event, 'pointerId', { value: 7 });
      handle.dispatchEvent(event);
    }
    pointer('pointerdown', 300);
    expect(onCompare).toHaveBeenLastCalledWith(25);
    pointer('pointermove', 1200);
    expect(onCompare).toHaveBeenLastCalledWith(100);
    pointer('pointercancel', 1200);
    pointer('pointermove', 0);
    expect(onCompare).toHaveBeenCalledTimes(2);
    handle.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
    expect(onCompare).toHaveBeenLastCalledWith(45);
    handle.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
    expect(onCompare).toHaveBeenLastCalledWith(0);
    expect(document.body.textContent).toContain(copy.en.previewHint);
  });

  it('shows the saved file and explains that an overwritten original cannot be compared', () => {
    const overwritten: TaskItem = { ...item, status: 'completed', saved_bytes: 400, output_size: 600, output_path: item.source_path, attempt: { preset: 'balanced', output_mode: 'overwrite', output_subfolder: 'compressed', metadata_policy: 'essential' } };
    app = mount(ComparisonPreview, { target: document.getElementById('test-root')!, props: { t: copy.en, item: overwritten, result: undefined, error: undefined, language: 'en', onCompare: vi.fn(), onZoom: vi.fn() } });
    expect(document.querySelector('img')?.getAttribute('alt')).toBe(copy.en.savedResult);
    expect(document.querySelector('.compare-handle')).toBeNull();
    expect(document.body.textContent).toContain(copy.en.sourceReplaced);
    expect(document.querySelector<HTMLInputElement>('input[type="range"]')?.disabled).toBe(true);
  });
});
