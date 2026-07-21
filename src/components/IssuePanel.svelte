<script lang="ts">
  import { Copy, FolderOpen, Trash2, X } from 'lucide-svelte';
  import { errorText, type Messages } from '../lib/i18n';
  import type { AppError } from '../lib/types';

  export let t: Messages;
  export let issues: AppError[] = [];
  export let onClose: () => void;
  export let onClear: () => void;
  export let onReveal: (path: string) => void;
  let copied = false;

  async function copyDetails() {
    const report = issues.map((issue) => [
      errorText(issue, t),
      issue.path ?? '',
      issue.detail ?? ''
    ].filter(Boolean).join('\n')).join('\n\n');
    await navigator.clipboard.writeText(report);
    copied = true;
    window.setTimeout(() => copied = false, 1200);
  }

  function closeOnEscape(event: KeyboardEvent) {
    if (event.key === 'Escape') onClose();
  }
</script>

<svelte:window on:keydown={closeOnEscape} />

<div class="issue-backdrop" role="presentation" on:click|self={onClose}>
  <div class="issue-panel" role="dialog" aria-modal="true" aria-label={t.issuePanelTitle}>
    <header>
      <div>
        <h2>{t.issuePanelTitle}</h2>
        <span>{t.issueCount.replace('{count}', String(issues.length))}</span>
      </div>
      <button class="icon-button" type="button" title={t.close} aria-label={t.close} on:click={onClose}>
        <X size={17} aria-hidden="true" />
      </button>
    </header>

    <div class="issue-list">
      {#if issues.length === 0}
        <p class="issue-empty">{t.noIssues}</p>
      {:else}
        {#each issues as issue, index (`${issue.code}:${issue.path ?? ''}:${index}`)}
          <article class="issue-row">
            <div class="issue-copy">
              <strong>{errorText(issue, t)}</strong>
              {#if issue.path}<span title={issue.path}>{issue.path}</span>{/if}
              {#if issue.detail}<details><summary>{t.technicalDetails}</summary><code>{issue.detail}</code></details>{/if}
            </div>
            {#if issue.path}
              <button type="button" title={t.reveal} aria-label={t.reveal} on:click={() => onReveal(issue.path!)}>
                <FolderOpen size={15} aria-hidden="true" />
              </button>
            {/if}
          </article>
        {/each}
      {/if}
    </div>

    <footer>
      <button type="button" disabled={!issues.length} on:click={onClear}>
        <Trash2 size={15} aria-hidden="true" />{t.clearIssues}
      </button>
      <button type="button" disabled={!issues.length} on:click={copyDetails}>
        <Copy size={15} aria-hidden="true" />{copied ? t.copied : t.copyDetails}
      </button>
    </footer>
  </div>
</div>
