import { describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import { QueueController, virtualRange } from './queue';
import type { InputItem } from './types';

function input(index: number, overrides: Partial<InputItem> = {}): InputItem {
  return {
    id: String(index),
    source_path: `C:\\images\\${index}.png`,
    input_root: 'C:\\images',
    relative_path: `${index}.png`,
    name: `${index}.png`,
    format: 'png',
    width: 10,
    height: 10,
    original_size: 100,
    modified_ms: 1,
    ...overrides
  };
}

describe('QueueController', () => {
  it('publishes immutable queue snapshots for UI reactivity', () => {
    const queue = new QueueController();
    const initial = get(queue);

    queue.merge([input(1)]);
    const merged = get(queue);

    expect(merged).not.toBe(initial);
    expect(merged).toMatchObject({ version: 1, count: 1, ids: ['1'] });
    expect(merged.totals.statuses.ready).toBe(1);

    queue.update({
      batch_id: 'batch',
      item_id: '1',
      status: 'completed',
      output_path: 'C:\\images\\compressed\\1.png',
      output_size: 60,
      saved_bytes: 40,
      error: null
    });
    const updated = get(queue);

    expect(updated).not.toBe(merged);
    expect(updated.ids).toBe(merged.ids);
    expect(updated).toMatchObject({ version: 2, count: 1 });
    expect(updated.totals.statuses.completed).toBe(1);
  });

  it('maintains aggregate counts and bytes incrementally', () => {
    const queue = new QueueController();
    queue.merge([input(1), input(2, { original_size: 200 })]);
    expect(queue.totals).toEqual({
      statuses: {
        ready: 2,
        processing: 0,
        completed: 0,
        unchanged: 0,
        failed: 0,
        cancelled: 0
      },
      originalBytes: 300,
      outputBytes: 0,
      savedBytes: 0
    });

    queue.update({
      batch_id: 'batch',
      item_id: '1',
      status: 'completed',
      output_path: 'C:\\images\\compressed\\1.png',
      output_size: 60,
      saved_bytes: 40,
      error: null
    });
    expect(queue.totals.statuses).toMatchObject({ ready: 1, completed: 1 });
    expect(queue.totals.outputBytes).toBe(60);
    expect(queue.totals.savedBytes).toBe(40);

    queue.remove('1');
    expect(queue.totals.originalBytes).toBe(200);
    expect(queue.totals.outputBytes).toBe(0);
    expect(queue.totals.statuses.completed).toBe(0);
  });

  it('keeps the more complete relative mapping across additions', () => {
    const queue = new QueueController();
    queue.merge([input(1)]);
    const added = queue.merge([input(1, {
      input_root: 'C:\\',
      relative_path: 'images\\1.png'
    })]);

    expect(added).toBe(0);
    expect(queue.get('1')?.input_root).toBe('C:\\');
    expect(queue.get('1')?.relative_path).toBe('images\\1.png');
    expect(queue.count).toBe(1);
  });

  it('linearly merges later batches into stable path order', () => {
    const queue = new QueueController();
    queue.merge([input(2), input(4)]);
    queue.merge([input(3), input(1)]);

    expect(queue.ids).toEqual(['1', '2', '3', '4']);
  });

  it('keeps the rendered window bounded for 10,000 rows', () => {
    const queue = new QueueController();
    queue.merge(Array.from({ length: 10_000 }, (_, index) => input(index)));
    const range = virtualRange(queue.count, 62 * 5_000, 620);

    expect(queue.count).toBe(10_000);
    expect(range.end - range.start).toBeLessThanOrEqual(26);
    expect(range.start).toBeGreaterThan(0);
    expect(range.end).toBeLessThan(10_000);
  });
});
