<script lang="ts">
  import { CheckCheck, ListFilter, RotateCcw, Trash2 } from 'lucide-svelte';
  import { formatBytes, savingsPercent } from '../lib/format';
  import { interpolate, type Messages } from '../lib/i18n';
  import type { QueueTotals } from '../lib/queue';
  import type { Language } from '../lib/types';

  export let t: Messages;
  export let language: Language;
  export let totals: QueueTotals;
  export let busy = false;
  export let onViewResults: () => void;
  export let onRetryFailed: () => void;
  export let onClearCompleted: () => void;

  $: done = totals.statuses.completed + totals.statuses.unchanged;
  $: originalBytes = totals.outputBytes + totals.savedBytes;
</script>

{#if done || totals.statuses.failed || totals.statuses.cancelled}
  <section class="result-summary" aria-label={t.queueResults}>
    <CheckCheck size={22} aria-hidden="true" />
    <div class="result-summary-copy">
      <strong>{t.queueResults} · {interpolate(t.savedSummary, { size: formatBytes(totals.savedBytes, language), percent: savingsPercent(originalBytes, totals.outputBytes) })}</strong>
      <span>{interpolate(t.resultCounts, { done: totals.statuses.completed, unchanged: totals.statuses.unchanged, failed: totals.statuses.failed, cancelled: totals.statuses.cancelled })}</span>
    </div>
    <div class="result-summary-actions">
      <button type="button" disabled={!done} on:click={onViewResults}><ListFilter size={14} aria-hidden="true" />{t.viewResults}</button>
      <button type="button" disabled={busy || !totals.statuses.failed} on:click={onRetryFailed}><RotateCcw size={14} aria-hidden="true" />{t.retryFailed}</button>
      <button type="button" disabled={busy || !done} on:click={onClearCompleted}><Trash2 size={14} aria-hidden="true" />{t.clearCompleted}</button>
    </div>
  </section>
{/if}
