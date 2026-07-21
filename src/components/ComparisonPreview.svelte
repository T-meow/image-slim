<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { Image as ImageIcon, Minus, Plus } from 'lucide-svelte';
  import { formatBytes } from '../lib/format';
  import { errorText, type Messages } from '../lib/i18n';
  import type { AppError, Language, PreviewResult, TaskItem } from '../lib/types';

  export let t: Messages;
  export let item: TaskItem | undefined;
  export let result: PreviewResult | undefined;
  export let loading = false;
  export let error: AppError | undefined;
  export let language: Language;
  export let comparePosition = 50;
  export let zoom = 1;
  export let onCompare: (value: number) => void;
  export let onZoom: (value: number) => void;

  $: originalUrl = result
    ? convertFileSrc(result.source_preview_path)
    : '';
  $: resultUrl = result ? convertFileSrc(result.candidate_preview_path) : '';
  $: previewStyle = `--compare-position:${comparePosition}%;--preview-zoom-size:${zoom * 100}%`;
</script>

<section class="preview-panel" aria-label={t.preview}>
  <div class="panel-heading preview-heading">
    <div>
      <h2>{t.preview}</h2>
      {#if item}<span>{item.name}</span>{/if}
    </div>
    {#if item}
      <div class="zoom-controls">
        <button type="button" title={t.zoomOut} aria-label={t.zoomOut} disabled={zoom <= 0.5} on:click={() => onZoom(Math.max(0.5, zoom - 0.25))}>
          <Minus size={14} aria-hidden="true" />
        </button>
        <span>{Math.round(zoom * 100)}%</span>
        <button type="button" title={t.zoomIn} aria-label={t.zoomIn} disabled={zoom >= 2} on:click={() => onZoom(Math.min(2, zoom + 0.25))}>
          <Plus size={14} aria-hidden="true" />
        </button>
      </div>
    {/if}
  </div>

  {#if !item}
    <div class="preview-empty">
      <ImageIcon size={42} strokeWidth={1.35} aria-hidden="true" />
      <span>{t.previewEmpty}</span>
    </div>
  {:else}
    <div class="preview-stage" style={previewStyle}>
      <div class="preview-image original-layer">
        {#if originalUrl}<img src={originalUrl} alt={t.original} />{/if}
      </div>
      {#if resultUrl && !error}
        <div class="preview-image result-layer">
          <img src={resultUrl} alt={t.result} />
        </div>
        <div class="compare-line"><span></span></div>
      {/if}
      <span class="preview-label original">{t.original}</span>
      <span class="preview-label result">{result?.would_replace === false ? t.candidate : t.result}</span>
      {#if loading}<div class="preview-message">{t.previewLoading}</div>{/if}
      {#if error}<div class="preview-message error">{errorText(error, t)}</div>{/if}
      {#if result && !result.would_replace}<div class="preview-message candidate-not-used">{t.candidateNotUsed}</div>{/if}
    </div>

    <div class="preview-footer">
      <div class="preview-sizes">
        <span><i class="original-swatch"></i>{t.original} <strong>{formatBytes(item.original_size, language)}</strong></span>
        <span><i class="result-swatch"></i>{result?.would_replace === false ? t.candidate : t.result} <strong>{result ? formatBytes(result.candidate_size, language) : '—'}</strong></span>
      </div>
      <label class="compare-slider">
        <span class="sr-only">{t.compare}</span>
        <input
          type="range"
          min="0"
          max="100"
          value={comparePosition}
          disabled={!result}
          aria-label={t.compare}
          on:input={(event) => onCompare(Number(event.currentTarget.value))}
        />
      </label>
    </div>
  {/if}
</section>
