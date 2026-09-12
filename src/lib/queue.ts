import { writable } from 'svelte/store';
import type { BatchSettings, InputItem, ItemProgress, TaskItem, TaskStatus } from './types';
import { isAudioFormat } from './types';

export type QueueFilter = 'all' | 'pending' | 'done' | 'failed' | 'cancelled';
export type QueueSort = 'path' | 'size' | 'saved';

export interface QueueTotals {
  statuses: Record<TaskStatus, number>;
  originalBytes: number;
  outputBytes: number;
  savedBytes: number;
}

export interface QueueSnapshot {
  version: number;
  ids: readonly string[];
  count: number;
  audioCount: number;
  totals: QueueTotals;
}

export class QueueController {
  private readonly items = new Map<string, TaskItem>();
  private orderedIds: readonly string[] = [];
  private version = 0;
  private audioCount = 0;
  private totalsState = emptyTotals();
  private readonly store = writable<QueueSnapshot>(this.createSnapshot());

  readonly subscribe = this.store.subscribe;

  get count(): number {
    return this.orderedIds.length;
  }

  get ids(): readonly string[] {
    return this.orderedIds;
  }

  get totals(): QueueTotals {
    return {
      ...this.totalsState,
      statuses: { ...this.totalsState.statuses }
    };
  }

  get(id: string): TaskItem | undefined {
    return this.items.get(id);
  }

  findByPath(path: string): TaskItem | undefined {
    const normalized = path.toLowerCase();
    return this.values().find((item) => item.source_path.toLowerCase() === normalized);
  }

  values(): TaskItem[] {
    return this.orderedIds.flatMap((id) => {
      const item = this.items.get(id);
      return item ? [item] : [];
    });
  }

  existingIds(): string[] {
    return [...this.orderedIds];
  }

  merge(inputs: InputItem[]): number {
    let added = 0;
    let changed = false;
    const newIds: string[] = [];
    for (const input of inputs) {
      const existing = this.items.get(input.id);
      if (!existing) {
        const item = asTask(input);
        this.items.set(input.id, item);
        newIds.push(input.id);
        this.adjustTotals(item, 1);
        added += 1;
        changed = true;
        continue;
      }
      if (preferredInput(existing, input) === input) {
        this.adjustTotals(existing, -1);
        const updated = existing.status === 'ready'
          ? { ...existing, ...input }
          : { ...existing, input_root: input.input_root, relative_path: input.relative_path };
        this.items.set(input.id, updated);
        this.adjustTotals(updated, 1);
        changed = true;
      }
    }
    if (changed) {
      newIds.sort((leftId, rightId) => this.compareIds(leftId, rightId));
      this.orderedIds = mergeSortedIds(
        this.orderedIds,
        newIds,
        (leftId, rightId) => this.compareIds(leftId, rightId)
      );
      this.bump();
    }
    return added;
  }

  update(progress: ItemProgress): void {
    const item = this.items.get(progress.item_id);
    if (!item) return;
    this.adjustTotals(item, -1);
    item.status = progress.status;
    item.output_path = progress.output_path ?? undefined;
    item.output_size = progress.output_size ?? undefined;
    item.saved_bytes = progress.saved_bytes;
    item.error = progress.error ?? undefined;
    this.adjustTotals(item, 1);
    this.bump();
  }

  beginAttempt(inputs: InputItem[], settings: BatchSettings): void {
    for (const input of inputs) {
      const previous = this.items.get(input.id);
      if (!previous) continue;
      this.adjustTotals(previous, -1);
      const item = { ...asTask(input), attempt: { ...settings } };
      this.items.set(input.id, item);
      this.adjustTotals(item, 1);
    }
    this.bump();
  }

  get hasAudio(): boolean {
    return this.audioCount > 0;
  }

  readyItems(): TaskItem[] {
    return this.values().filter((item) => ['ready', 'failed', 'cancelled'].includes(item.status));
  }

  remove(id: string): void {
    this.removeMany([id]);
  }

  removeMany(ids: readonly string[]): void {
    const removed = new Set(ids);
    let changed = false;
    for (const id of removed) {
      const item = this.items.get(id);
      if (!item) continue;
      this.adjustTotals(item, -1);
      this.items.delete(id);
      changed = true;
    }
    if (!changed) return;
    this.orderedIds = this.orderedIds.filter((id) => !removed.has(id));
    this.bump();
  }

  clear(): void {
    if (!this.items.size) return;
    this.items.clear();
    this.orderedIds = [];
    this.totalsState = emptyTotals();
    this.audioCount = 0;
    this.bump();
  }

  private adjustTotals(item: TaskItem, direction: 1 | -1): void {
    if (isAudioFormat(item.format)) this.audioCount += direction;
    this.totalsState.statuses[item.status] += direction;
    this.totalsState.originalBytes += direction * item.original_size;
    this.totalsState.outputBytes += direction * (item.output_size ?? 0);
    this.totalsState.savedBytes += direction * item.saved_bytes;
  }

  private compareIds(leftId: string, rightId: string): number {
    const left = this.items.get(leftId)?.source_path ?? '';
    const right = this.items.get(rightId)?.source_path ?? '';
    return comparePaths(left, right);
  }

  private bump(): void {
    this.version += 1;
    this.store.set(this.createSnapshot());
  }

  private createSnapshot(): QueueSnapshot {
    return {
      version: this.version,
      ids: this.orderedIds,
      count: this.orderedIds.length,
      audioCount: this.audioCount,
      totals: cloneTotals(this.totalsState)
    };
  }
}

export function virtualRange(
  count: number,
  scrollTop: number,
  viewportHeight: number,
  rowHeight = 62,
  overscan = 8
): { start: number; end: number } {
  const clampedTop = Math.max(0, Math.min(scrollTop, count * rowHeight - viewportHeight));
  const start = Math.max(0, Math.floor(clampedTop / rowHeight) - overscan);
  const visible = Math.ceil(viewportHeight / rowHeight) + overscan * 2;
  return { start, end: Math.min(count, start + visible) };
}

export function taskMatchesFilter(item: TaskItem, filter: QueueFilter): boolean {
  if (filter === 'all') return true;
  if (filter === 'pending') return item.status === 'ready' || item.status === 'processing';
  if (filter === 'done') return item.status === 'completed' || item.status === 'unchanged';
  return item.status === filter;
}

export function visibleTaskIds(
  ids: readonly string[],
  getItem: (id: string) => TaskItem | undefined,
  filter: QueueFilter,
  query: string,
  sort: QueueSort
): readonly string[] {
  const search = query.trim().toLowerCase();
  if (filter === 'all' && !search && sort === 'path') return ids;
  const items = ids.flatMap((id) => {
    const item = getItem(id);
    return item && taskMatchesFilter(item, filter)
      && (!search || item.source_path.toLowerCase().includes(search) || item.name.toLowerCase().includes(search))
      ? [item] : [];
  });
  if (sort !== 'path') {
    items.sort((left, right) => (sort === 'size'
      ? right.original_size - left.original_size
      : right.saved_bytes - left.saved_bytes) || comparePaths(left.source_path, right.source_path));
  }
  return items.map((item) => item.id);
}

function asTask(input: InputItem): TaskItem {
  return { ...input, status: 'ready', saved_bytes: 0 };
}

function preferredInput(existing: InputItem, candidate: InputItem): InputItem {
  const existingDepth = relativeDepth(existing.relative_path);
  const candidateDepth = relativeDepth(candidate.relative_path);
  if (candidateDepth !== existingDepth) return candidateDepth > existingDepth ? candidate : existing;
  return comparePaths(candidate.input_root, existing.input_root) < 0
    ? candidate
    : existing;
}

function relativeDepth(path: string): number {
  return path.split(/[\\/]+/).filter(Boolean).length;
}

function comparePaths(left: string, right: string): number {
  const normalizedLeft = left.replace(/\//g, '\\').toLowerCase();
  const normalizedRight = right.replace(/\//g, '\\').toLowerCase();
  return normalizedLeft < normalizedRight ? -1 : normalizedLeft > normalizedRight ? 1 : 0;
}

function mergeSortedIds(
  existing: readonly string[],
  incoming: readonly string[],
  compare: (left: string, right: string) => number
): string[] {
  if (!incoming.length) return [...existing];
  const merged: string[] = [];
  let existingIndex = 0;
  let incomingIndex = 0;
  while (existingIndex < existing.length && incomingIndex < incoming.length) {
    if (compare(existing[existingIndex], incoming[incomingIndex]) <= 0) {
      merged.push(existing[existingIndex++]);
    } else {
      merged.push(incoming[incomingIndex++]);
    }
  }
  merged.push(...existing.slice(existingIndex), ...incoming.slice(incomingIndex));
  return merged;
}

function emptyTotals(): QueueTotals {
  return {
    statuses: {
      ready: 0,
      processing: 0,
      completed: 0,
      unchanged: 0,
      failed: 0,
      cancelled: 0
    },
    originalBytes: 0,
    outputBytes: 0,
    savedBytes: 0
  };
}

function cloneTotals(totals: QueueTotals): QueueTotals {
  return {
    ...totals,
    statuses: { ...totals.statuses }
  };
}
