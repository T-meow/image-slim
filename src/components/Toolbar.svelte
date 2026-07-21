<script lang="ts">
  import {
    FileImage,
    FolderInput,
    Info,
    Languages,
    Moon,
    Sun,
    SunMoon,
    Trash2
  } from 'lucide-svelte';
  import type { Language, ThemePreference } from '../lib/types';
  import type { Messages } from '../lib/i18n';
  import iconUrl from '../../assets/icon.svg?url';

  export let t: Messages;
  export let language: Language;
  export let theme: ThemePreference;
  export let busy = false;
  export let hasItems = false;
  export let onAddFiles: () => void;
  export let onAddFolder: () => void;
  export let onClear: () => void;
  export let onAbout: () => void;
  export let onCycleTheme: () => void;
  export let onToggleLanguage: () => void;

  $: ThemeIcon = theme === 'light' ? Sun : theme === 'dark' ? Moon : SunMoon;
</script>

<header class="toolbar">
  <div class="brand" aria-label={t.appName}>
    <img src={iconUrl} alt="" />
    <span>{t.appName}</span>
  </div>

  <div class="toolbar-actions">
    <button class="primary" type="button" disabled={busy} on:click={onAddFiles}>
      <FileImage size={16} strokeWidth={1.9} aria-hidden="true" />
      <span>{t.addFiles}</span>
    </button>
    <button type="button" disabled={busy} on:click={onAddFolder}>
      <FolderInput size={16} strokeWidth={1.9} aria-hidden="true" />
      <span>{t.addFolder}</span>
    </button>
    <button class="icon-button" type="button" disabled={busy || !hasItems} title={t.clear} aria-label={t.clear} on:click={onClear}>
      <Trash2 size={16} strokeWidth={1.9} aria-hidden="true" />
    </button>
  </div>

  <div class="toolbar-end">
    <button class="icon-button" type="button" title={t.about} aria-label={t.about} on:click={onAbout}>
      <Info size={16} strokeWidth={1.9} aria-hidden="true" />
    </button>
    <button class="theme-button" type="button" title={`${t.theme}: ${t.themes[theme]}`} on:click={onCycleTheme}>
      <ThemeIcon size={16} strokeWidth={1.9} aria-hidden="true" />
      <span>{t.themes[theme]}</span>
    </button>
    <button class="language-button" type="button" title={t.language} aria-label={t.language} on:click={onToggleLanguage}>
      <Languages size={16} strokeWidth={1.9} aria-hidden="true" />
      <span>{language === 'zh' ? 'EN' : '中'}</span>
    </button>
  </div>
</header>
