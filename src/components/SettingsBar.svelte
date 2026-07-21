<script lang="ts">
  import { Database, FolderOutput, Gauge, ShieldCheck } from 'lucide-svelte';
  import type { CompressionPreset, OutputMode } from '../lib/types';
  import type { Messages } from '../lib/i18n';

  export let t: Messages;
  export let preset: CompressionPreset;
  export let outputMode: OutputMode;
  export let outputSubfolder: string;
  export let preserveSupported = false;
  export let disabled = false;
  export let folderValid = true;
  export let onPreset: (value: CompressionPreset) => void;
  export let onOutputMode: (value: OutputMode) => void;
  export let onOutputSubfolder: (value: string) => void;
  export let onPreserveSupported: (value: boolean) => void;

  const presets: CompressionPreset[] = ['lossless', 'balanced', 'strong'];
  const outputModes: OutputMode[] = ['subfolder', 'overwrite'];
</script>

<section class="settings-bar" aria-label={t.compressionSettings}>
  <div class="settings-group preset-group">
    <span class="settings-label"><Gauge size={14} aria-hidden="true" />{t.status}</span>
    <div class="segmented" role="group" aria-label={t.compressionPreset}>
      {#each presets as value}
        <button
          type="button"
          class:active={preset === value}
          title={t.presetTitles[value]}
          disabled={disabled}
          on:click={() => onPreset(value)}
        >{t.presets[value]}</button>
      {/each}
    </div>
  </div>

  <div class="settings-divider"></div>

  <div class="settings-group output-group">
    <span class="settings-label"><FolderOutput size={14} aria-hidden="true" />{t.output}</span>
    <div class="segmented" role="group" aria-label={t.outputMode}>
      {#each outputModes as value}
        <button
          type="button"
          class:active={outputMode === value}
          disabled={disabled}
          on:click={() => onOutputMode(value)}
        >{t.outputModes[value]}</button>
      {/each}
    </div>
    {#if outputMode === 'subfolder'}
      <label class="folder-input" class:invalid={!folderValid}>
        <span class="sr-only">{t.folderName}</span>
        <input
          value={outputSubfolder}
          disabled={disabled}
          aria-invalid={!folderValid}
          title={folderValid ? t.folderName : t.invalidFolder}
          on:input={(event) => onOutputSubfolder(event.currentTarget.value)}
        />
      </label>
    {/if}
  </div>

  <label class="metadata-toggle" title={t.preserveSupported}>
    <input
      type="checkbox"
      checked={preserveSupported}
      disabled={disabled}
      on:change={(event) => onPreserveSupported(event.currentTarget.checked)}
    />
    <span class="toggle-track"><span></span></span>
    {#if preserveSupported}<Database size={14} aria-hidden="true" />{:else}<ShieldCheck size={14} aria-hidden="true" />{/if}
    <span>{t.preserveSupported}</span>
  </label>
</section>
