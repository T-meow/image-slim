<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { Image as ImageIcon, Minus, Plus } from 'lucide-svelte';
  import { formatBytes, savingsPercent } from '../lib/format';
  import { errorText, interpolate, type Messages } from '../lib/i18n';
  import { sourceWasOverwritten, type AppError, type Language, type PreviewResult, type TaskItem } from '../lib/types';

  export let t: Messages;
  export let item: TaskItem | undefined;
  export let result: PreviewResult | undefined;
  export let loading = false;
  export let error: AppError | undefined;
  export let paused = false;
  export let language: Language;
  export let comparePosition = 50;
  export let zoom = 1;
  export let onCompare: (value: number) => void;
  export let onZoom: (value: number) => void;

  $: originalUrl = result
    ? convertFileSrc(result.source_preview_path)
    : '';
  $: resultUrl = result ? convertFileSrc(result.candidate_preview_path) : '';
  let canvas: HTMLDivElement;
  let viewportWidth = 1;
  let viewportHeight = 1;
  let activePointer: number | undefined;
  $: savedOnly = item ? sourceWasOverwritten(item) : false;
  $: savedUrl = savedOnly && item?.output_path ? convertFileSrc(item.output_path) : '';
  $: fitScale = item ? Math.min(Math.max(1, viewportWidth - 32) / item.width, Math.max(1, viewportHeight - 32) / item.height) : 1;
  $: imageWidth = (item?.width ?? 1) * fitScale * zoom;
  $: imageHeight = (item?.height ?? 1) * fitScale * zoom;
  $: previewStyle = `--compare-position:${comparePosition}%;--preview-width:${imageWidth}px;--preview-height:${imageHeight}px;width:max(100%, ${imageWidth + 32}px);height:max(100%, ${imageHeight + 32}px)`;

  function measureStage(node: HTMLDivElement) {
    const update = () => {
      viewportWidth = node.clientWidth;
      viewportHeight = node.clientHeight;
    };
    const observer = new ResizeObserver(update);
    observer.observe(node);
    update();
    return { destroy: () => observer.disconnect() };
  }

  function updateCompare(clientX: number) {
    const rect = canvas.getBoundingClientRect();
    if (rect.width) onCompare(Math.max(0, Math.min(100, (clientX - rect.left) / rect.width * 100)));
  }

  function beginDrag(event: PointerEvent & { currentTarget: HTMLButtonElement }) {
    if (event.button !== 0) return;
    activePointer = event.pointerId;
    event.currentTarget.setPointerCapture(event.pointerId);
    updateCompare(event.clientX);
  }

  function moveDrag(event: PointerEvent) {
    if (event.pointerId === activePointer) updateCompare(event.clientX);
  }

  function endDrag(event: PointerEvent) {
    if (event.pointerId === activePointer) activePointer = undefined;
  }

  function compareKey(event: KeyboardEvent) {
    const values: Record<string, number> = { ArrowLeft: comparePosition - 5, ArrowRight: comparePosition + 5, Home: 0, End: 100 };
    if (!(event.key in values)) return;
    event.preventDefault();
    onCompare(Math.max(0, Math.min(100, values[event.key])));
  }
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
        <button class="zoom-reset" type="button" title={t.viewZoom} aria-label={t.fitPreview} on:click={() => onZoom(1)}>{zoom === 1 ? t.fitPreview : `${zoom}×`}</button>
        <button type="button" title={t.zoomIn} aria-label={t.zoomIn} disabled={zoom >= 3} on:click={() => onZoom(Math.min(3, zoom + 0.25))}>
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
    <div class="preview-stage" use:measureStage>
    <div class="preview-canvas" bind:this={canvas} style={previewStyle}>
      <div class="preview-image original-layer">
        {#if savedUrl}<img src={savedUrl} alt={t.savedResult} />
        {:else if originalUrl}<img src={originalUrl} alt={t.original} />{/if}
      </div>
      {#if resultUrl && !error}
        <div class="preview-image result-layer">
          <img src={resultUrl} alt={t.result} />
        </div>
        <div class="compare-line">
          <button class="compare-handle" type="button" aria-label={t.compare} title={t.compare}
            on:pointerdown={beginDrag} on:pointermove={moveDrag} on:pointerup={endDrag}
            on:pointercancel={endDrag} on:lostpointercapture={endDrag} on:keydown={compareKey}></button>
        </div>
      {/if}
      <span class="preview-label original">{savedOnly ? t.savedResult : t.original}</span>
      {#if !savedOnly}<span class="preview-label result">{result?.would_replace === false ? t.candidate : t.result}</span>{/if}
      {#if paused}<div class="preview-message">{t.previewPaused}</div>{/if}
      {#if savedOnly && !paused}<div class="preview-message">{t.sourceReplaced}</div>{/if}
      {#if loading}<div class="preview-message">{t.previewLoading}</div>{/if}
      {#if error}<div class="preview-message error">{errorText(error, t)}</div>{/if}
      {#if result && !result.would_replace}<div class="preview-message candidate-not-used">{t.candidateNotUsed}</div>{/if}
    </div>
    </div>

    <div class="preview-footer">
      <p class="preview-hint">{savedOnly ? t.sourceReplaced : t.previewHint}</p>
      <div class="preview-sizes">
        <span><i class="original-swatch"></i>{t.original} <strong>{formatBytes(item.original_size, language)}</strong></span>
        <span><i class="result-swatch"></i>{savedOnly ? t.savedResult : result?.would_replace === false ? t.candidate : t.result} <strong>{savedOnly ? formatBytes(item.output_size ?? 0, language) : result ? formatBytes(result.candidate_size, language) : '—'}</strong></span>
        {#if result?.would_replace}<span class="preview-saving">{interpolate(t.expectedSaving, { percent: savingsPercent(result.source_size, result.candidate_size) })}</span>{/if}
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
