<script lang="ts">
  import { onDestroy } from 'svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { Music2 } from 'lucide-svelte';
  import { formatBytes, formatDuration } from '../lib/format';
  import type { Messages } from '../lib/i18n';
  import type { Language, TaskItem } from '../lib/types';

  export let t: Messages;
  export let item: TaskItem;
  export let language: Language;
  export let paused = false;

  let sourcePlayer: HTMLAudioElement | undefined;
  let resultPlayer: HTMLAudioElement | undefined;
  let sourceFailed = false;
  let resultFailed = false;
  let previousPlayerKey = '';
  $: playerKey = `${item.id}:${item.modified_ms}:${item.output_path ?? ''}:${item.output_size ?? ''}`;
  $: if (playerKey !== previousPlayerKey) {
    pausePlayback();
    previousPlayerKey = playerKey;
    sourceFailed = false;
    resultFailed = false;
  }
  $: if (paused) pausePlayback();
  $: hasResult = item.status === 'completed' && item.output_path && item.output_path !== item.source_path;

  function pausePlayback() {
    sourcePlayer?.pause();
    resultPlayer?.pause();
  }
  onDestroy(pausePlayback);
</script>

<section class="preview-panel audio-preview" aria-label={t.audioPreview}>
  <div class="panel-heading"><h2>{t.audioPreview}</h2><span>{item.format.toUpperCase()} → MP3</span></div>
  <div class="audio-content">
    <div class="audio-symbol"><Music2 size={42} aria-hidden="true" /></div>
    <h3 title={item.source_path}>{item.name}</h3>
    <dl class="audio-facts">
      <div><dt>{t.audioDuration}</dt><dd>{formatDuration(item.audio?.duration_ms)}</dd></div>
      <div><dt>{t.audioSampleRate}</dt><dd>{item.audio ? `${item.audio.sample_rate / 1000} kHz` : '—'}</dd></div>
      <div><dt>{t.audioChannels}</dt><dd>{item.audio?.channels === 1 ? t.audioMono : item.audio?.channels === 2 ? t.audioStereo : '—'}</dd></div>
    </dl>
    {#if paused}
      <p class="audio-hint">{t.audioPaused}</p>
    {:else}
      {#key playerKey}
        <div class="audio-player">
          <div><strong>{t.audioOriginal}</strong><span>{formatBytes(item.original_size, language)}</span></div>
          <audio bind:this={sourcePlayer} controls preload="metadata" src={convertFileSrc(item.source_path)}
            aria-label={t.audioOriginal} on:play={() => resultPlayer?.pause()} on:error={() => sourceFailed = true}></audio>
          {#if sourceFailed}<p class="audio-hint">{t.audioPlaybackFailed}</p>{/if}
        </div>
        {#if hasResult && item.output_path}
          <div class="audio-player">
            <div><strong>{t.savedResult}</strong><span>MP3 · {item.attempt?.audio_bitrate_kbps ?? 128} kbps · {formatBytes(item.output_size ?? 0, language)}</span></div>
            <audio bind:this={resultPlayer} controls preload="metadata" src={convertFileSrc(item.output_path)}
              aria-label={t.savedResult} on:play={() => sourcePlayer?.pause()} on:error={() => resultFailed = true}></audio>
            {#if resultFailed}<p class="audio-hint">{t.audioPlaybackFailed}</p>{/if}
          </div>
        {:else}
          <p class="audio-hint">{item.status === 'unchanged' ? t.audioUnchanged : t.audioResultHint}</p>
        {/if}
      {/key}
    {/if}
    <p class="audio-hint">{t.audioLossyHint}</p>
  </div>
</section>
