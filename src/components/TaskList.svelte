<script lang="ts">
  import { onMount } from 'svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { FolderOpen, RotateCcw, X } from 'lucide-svelte';
  import { formatBytes, savingsPercent } from '../lib/format';
  import { errorText, interpolate, type Messages } from '../lib/i18n';
  import { virtualRange } from '../lib/queue';
  import type { Language, TaskItem } from '../lib/types';

  const ROW_HEIGHT = 62;

  export let t: Messages;
  export let ids: readonly string[] = [];
  export let version = 0;
  export let formats = '';
  export let selectedId = '';
  export let language: Language;
  export let busy = false;
  export let getItem: (id: string) => TaskItem | undefined;
  export let onSelect: (id: string) => void;
  export let onRemove: (id: string) => void;
  export let onRetry: (id: string) => void;
  export let onReveal: (item: TaskItem) => void;

  let scrollElement: HTMLDivElement;
  let scrollTop = 0;
  let viewportHeight = 1;
  $: range = virtualRange(ids.length, scrollTop, viewportHeight, ROW_HEIGHT);
  $: visibleRows = rowsForVersion(version, ids, range.start, range.end);

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
    <span>{ids.length} {t.images}</span>
  </div>

  {#if ids.length > 0}
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
    {:else}
      <div class="task-virtual-space" style={`height:${ids.length * ROW_HEIGHT}px`}>
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
                  <img
                    src={convertFileSrc(row.item.source_path)}
                    alt=""
                    loading="lazy"
                    decoding="async"
                  />
                </span>
                <span class="task-name">
                  <strong title={row.item.source_path}>{row.item.name}</strong>
                  <span>{row.item.format.toUpperCase()} · {row.item.width}×{row.item.height}</span>
                </span>
              </span>
              <span class="task-size">
                <strong>{formatBytes(row.item.output_size ?? row.item.original_size, language)}</strong>
                {#if row.item.output_size}
                  <span class:saved={row.item.saved_bytes > 0}>-{savingsPercent(row.item.original_size, row.item.output_size)}%</span>
                {:else}
                  <span aria-hidden="true">—</span>
                {/if}
              </span>
              <span class="task-state">
                <span class="status-dot {row.item.status}"></span>
                <span title={row.item.error ? errorText(row.item.error, t) : undefined}>{t.statuses[row.item.status]}</span>
              </span>
            </button>
            <div class="row-actions">
              {#if row.item.status === 'failed' || row.item.status === 'cancelled'}
                <button type="button" title={t.retry} aria-label={t.retry} disabled={busy} on:click={() => onRetry(row.item.id)}>
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
