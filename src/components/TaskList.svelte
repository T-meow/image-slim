<script lang="ts">
  import { onMount } from 'svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { FolderOpen, Music2, RotateCcw, Search, X } from 'lucide-svelte';
  import { formatBytes, formatDuration, savingsPercent } from '../lib/format';
  import { errorText, interpolate, type Messages } from '../lib/i18n';
  import { virtualRange, visibleTaskIds, type QueueFilter, type QueueSort, type QueueTotals } from '../lib/queue';
  import type { Language, TaskItem } from '../lib/types';
  import { isAudioFormat } from '../lib/types';

  const ROW_HEIGHT = 62;

  export let t: Messages;
  export let ids: readonly string[] = [];
  export let version = 0;
  export let formats = '';
  export let selectedId = '';
  export let language: Language;
  export let busy = false;
  export let totals: QueueTotals | undefined = undefined;
  export let filter: QueueFilter = 'all';
  export let query = '';
  export let sort: QueueSort = 'path';
  export let getItem: (id: string) => TaskItem | undefined;
  export let onSelect: (id: string) => void;
  export let onRemove: (id: string) => void;
  export let onRetry: (id: string) => void;
  export let onReveal: (item: TaskItem) => void;

  let scrollElement: HTMLDivElement;
  let scrollTop = 0;
  let viewportHeight = 1;
  let lastViewKey = '';
  const filters: QueueFilter[] = ['all', 'pending', 'done', 'failed', 'cancelled'];
  const sorts: QueueSort[] = ['path', 'size', 'saved'];
  $: filteredIds = idsForVersion(version, ids, filter, query, sort);
  $: range = virtualRange(filteredIds.length, scrollTop, viewportHeight, ROW_HEIGHT);
  $: visibleRows = rowsForVersion(version, filteredIds, range.start, range.end);
  $: filterCounts = {
    all: ids.length,
    pending: (totals?.statuses.ready ?? 0) + (totals?.statuses.processing ?? 0),
    done: (totals?.statuses.completed ?? 0) + (totals?.statuses.unchanged ?? 0),
    failed: totals?.statuses.failed ?? 0,
    cancelled: totals?.statuses.cancelled ?? 0
  };
  $: viewKey = JSON.stringify([filter, query, sort]);
  $: if (viewKey !== lastViewKey) {
    lastViewKey = viewKey;
    scrollTop = 0;
    if (scrollElement) scrollElement.scrollTop = 0;
  }

  function idsForVersion(_version: number, orderedIds: readonly string[], activeFilter: QueueFilter, search: string, order: QueueSort) {
    return visibleTaskIds(orderedIds, getItem, activeFilter, search, order);
  }

  function resetFilters() {
    filter = 'all';
    query = '';
    sort = 'path';
  }

  function rowsForVersion(_version: number, orderedIds: readonly string[], start: number, end: number) {
    return orderedIds.slice(start, end).flatMap((id, offset) => {
      const item = getItem(id);
      return item ? [{ item, index: start + offset }] : [];
    });
  }

  onMount(() => {
    const observer = new ResizeObserver(() => {
      viewportHeight = scrollElement.clientHeight;
    });
    observer.observe(scrollElement);
    viewportHeight = scrollElement.clientHeight;
    return () => observer.disconnect();
  });
</script>

<section class="task-panel" aria-label={t.queue}>
  <div class="panel-heading">
    <h2>{t.queue}</h2>
    <span>{interpolate(t.filteredCount, { visible: filteredIds.length, total: ids.length })}</span>
  </div>

  {#if ids.length > 0}
    <div class="queue-controls">
      <div class="queue-search-row">
        <label class="queue-search">
          <Search size={14} aria-hidden="true" />
          <input type="search" bind:value={query} placeholder={t.searchTasks} aria-label={t.searchTasks} />
        </label>
        <select bind:value={sort} aria-label={t.queueSort}>
          {#each sorts as value}<option value={value}>{t.queueSorts[value]}</option>{/each}
        </select>
      </div>
      <div class="queue-filters" role="group" aria-label={t.queueFilter}>
        {#each filters as value}
          <button type="button" class:active={filter === value} aria-pressed={filter === value} on:click={() => filter = value}>
            {t.queueFilters[value]}<span>{filterCounts[value]}</span>
          </button>
        {/each}
      </div>
    </div>
    <div class="task-table-header" aria-hidden="true">
      <span>{t.queue}</span>
      <span>{t.size}</span>
      <span>{t.status}</span>
      <span>{t.actions}</span>
    </div>
  {/if}
  <div
    class="task-scroll"
    class:empty={ids.length === 0}
    bind:this={scrollElement}
    on:scroll={(event) => scrollTop = event.currentTarget.scrollTop}
  >
    {#if ids.length === 0}
      <div class="empty-queue">
        <div class="empty-mark"><span></span><span></span><span></span></div>
        <strong>{interpolate(t.addHint, { formats })}</strong>
        <p>{interpolate(t.supportedHint, { formats })}</p>
      </div>
    {:else if !filteredIds.length}
      <div class="empty-queue">
        <strong>{t.noMatchingTasks}</strong>
        <button type="button" on:click={resetFilters}>{t.resetFilters}</button>
      </div>
    {:else}
      <div class="task-virtual-space" style={`height:${filteredIds.length * ROW_HEIGHT}px`}>
        {#each visibleRows as row (row.item.id)}
          <div
            class="task-row"
            class:selected={selectedId === row.item.id}
            style={`transform:translateY(${row.index * ROW_HEIGHT}px)`}
          >
            <button
              class="task-select"
              type="button"
              aria-pressed={selectedId === row.item.id}
              aria-label={interpolate(t.selectTask, { name: row.item.name })}
              on:click={() => onSelect(row.item.id)}
            >
              <span class="task-identity">
                <span class="thumbnail">
                  {#if isAudioFormat(row.item.format)}
                    <Music2 size={22} aria-hidden="true" />
                  {:else}
                  <img
                    src={convertFileSrc(row.item.source_path)}
                    alt=""
                    loading="lazy"
                    decoding="async"
                  />
                  {/if}
                </span>
                <span class="task-name">
                  <strong title={row.item.source_path}>{row.item.name}</strong>
                  <span>{row.item.format.toUpperCase()} · {isAudioFormat(row.item.format)
                    ? formatDuration(row.item.audio?.duration_ms) : `${row.item.width}×${row.item.height}`}</span>
                </span>
              </span>
              <span class="task-size">
                {#if row.item.output_size !== undefined}
                  <span>{formatBytes(row.item.original_size, language)} →</span>
                  <strong>{formatBytes(row.item.output_size, language)} <small class:saved={row.item.saved_bytes > 0}>−{savingsPercent(row.item.original_size, row.item.output_size)}%</small></strong>
                {:else}
                  <strong>{formatBytes(row.item.original_size, language)}</strong>
                  <span aria-hidden="true">—</span>
                {/if}
              </span>
              <span class="task-state">
                <span class="status-dot {row.item.status}"></span>
                <span title={row.item.error ? errorText(row.item.error, t) : undefined}>{t.statuses[row.item.status]}</span>
              </span>
            </button>
            <div class="row-actions">
              {#if row.item.status !== 'ready' && row.item.status !== 'processing'}
                <button type="button" title={row.item.output_path ? t.reprocess : t.retry} aria-label={row.item.output_path ? t.reprocess : t.retry} disabled={busy} on:click={() => onRetry(row.item.id)}>
                  <RotateCcw size={15} aria-hidden="true" />
                </button>
              {/if}
              {#if row.item.output_path}
                <button type="button" title={t.reveal} aria-label={t.reveal} on:click={() => onReveal(row.item)}>
                  <FolderOpen size={15} aria-hidden="true" />
                </button>
              {/if}
              <button type="button" title={t.remove} aria-label={t.remove} disabled={busy} on:click={() => onRemove(row.item.id)}>
                <X size={15} aria-hidden="true" />
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</section>
