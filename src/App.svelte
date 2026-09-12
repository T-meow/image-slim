<script lang="ts">
  import { onMount } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { confirm, message, open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import { AlertTriangle, CircleStop, Music2, Play, XCircle } from 'lucide-svelte';
  import Toolbar from './components/Toolbar.svelte';
  import SettingsBar from './components/SettingsBar.svelte';
  import TaskList from './components/TaskList.svelte';
  import ComparisonPreview from './components/ComparisonPreview.svelte';
  import AudioPreview from './components/AudioPreview.svelte';
  import IssuePanel from './components/IssuePanel.svelte';
  import ResultSummary from './components/ResultSummary.svelte';
  import { copy, errorText, interpolate, type Messages } from './lib/i18n';
  import { formatBytes, validSubfolderName } from './lib/format';
  import { PreviewController } from './lib/preview-controller';
  import { QueueController, type QueueFilter, type QueueSort } from './lib/queue';
  import { SessionController, sessionBusy, type SessionState } from './lib/session';
  import { WorkflowController } from './lib/workflow';
  import { isAudioFormat, sourceWasOverwritten } from './lib/types';
  import {
    LANGUAGE_KEY,
    AUDIO_BITRATE_KEY,
    METADATA_KEY,
    OUTPUT_FOLDER_KEY,
    OUTPUT_MODE_KEY,
    PRESET_KEY,
    THEME_KEY,
    applyTheme,
    initialLanguage,
    initialAudioBitrate,
    initialMetadataPolicy,
    initialTheme
  } from './lib/settings';
  import { getCapabilities, inTauri, normalizeAppError, revealPath } from './lib/tauri';
  import type {
    AppCapabilities,
    BatchSummary,
    CompressionPreset,
    ItemProgress,
    Language,
    MetadataPolicy,
    OutputMode,
    ScanEvent,
    TaskItem,
    ThemePreference
  } from './lib/types';

  const queue = new QueueController();
  const session = new SessionController();
  const preview = new PreviewController((error) => session.addIssues([error]));
  const fallbackCapabilities: AppCapabilities = {
    formats: [
      { format: 'png', extensions: ['png'] },
      { format: 'jpeg', extensions: ['jpg', 'jpeg'] },
      { format: 'webp', extensions: ['webp'] },
      { format: 'mp3', extensions: ['mp3'] },
      { format: 'wav', extensions: ['wav'] },
      { format: 'flac', extensions: ['flac'] },
      { format: 'm4a', extensions: ['m4a'] },
      { format: 'ogg', extensions: ['ogg'] }
    ],
    presets: ['lossless', 'balanced', 'strong'],
    limits: {
      max_file_bytes: 512 * 1024 * 1024,
      max_pixels: 100_000_000,
      max_dimension: 65_535,
      max_queue_items: 10_000
    }
  };

  let capabilities = fallbackCapabilities;
  let language: Language = initialLanguage();
  let theme: ThemePreference = initialTheme();
  let preset: CompressionPreset = readPreset();
  let audioBitrate = initialAudioBitrate();
  let outputMode: OutputMode = readOutputMode();
  let outputSubfolder = localStorage.getItem(OUTPUT_FOLDER_KEY) || 'compressed';
  let metadataPolicy: MetadataPolicy = initialMetadataPolicy();
  let selectedId = '';
  let dropActive = false;
  let comparePosition = 50;
  let zoom = 1;
  let lastPreviewKey = '';
  let issuesOpen = false;
  let ipcReady = !inTauri;
  let queueFilter: QueueFilter = 'all';
  let queueSearch = '';
  let queueSort: QueueSort = 'path';

  const workflow = new WorkflowController(queue, session, preview, {
    settings: () => ({ preset, audio_bitrate_kbps: audioBitrate, output_mode: outputMode, output_subfolder: outputSubfolder, metadata_policy: metadataPolicy }),
    messages: () => copy[language],
    capabilities: () => capabilities,
    confirm: (request, count) => confirm(interpolate(
      request.output_mode === 'overwrite' ? copy[language].overwriteConfirm : copy[language].outputConflictConfirm,
      { count }
    ), { title: copy[language].appName, kind: 'warning' })
  });

  $: t = copy[language];
  $: busy = !ipcReady || sessionBusy($session);
  $: readyCount = $queue.totals.statuses.ready + $queue.totals.statuses.failed + $queue.totals.statuses.cancelled;
  $: selectedItem = itemForVersion($queue.version, selectedId);
  $: supportedFormats = capabilities.formats
    .map((capability) => capability.format.toUpperCase())
    .join(language === 'zh' ? '、' : ' / ');
  $: subfolderNameValid = validSubfolderName(outputSubfolder);
  $: folderValid = (outputMode === 'overwrite' && !$queue.audioCount) || subfolderNameValid;
  $: previewKey = selectedItem
    && !busy
    && !sourceWasOverwritten(selectedItem)
    && !isAudioFormat(selectedItem.format)
    ? `${selectedItem.id}:${selectedItem.modified_ms}:${preset}:${metadataPolicy}`
    : '';
  $: if (previewKey !== lastPreviewKey) {
    lastPreviewKey = previewKey;
    comparePosition = 50;
    zoom = 1;
    preview.schedule(previewKey ? selectedItem : undefined, preset, metadataPolicy);
  }
  $: summaryText = buildSummaryText($session, $queue.count, language, t);

  function itemForVersion(_version: number, id: string): TaskItem | undefined {
    return queue.get(id);
  }

  function readPreset(): CompressionPreset {
    const saved = localStorage.getItem(PRESET_KEY);
    return saved === 'lossless' || saved === 'strong' || saved === 'balanced' ? saved : 'balanced';
  }

  function readOutputMode(): OutputMode {
    return localStorage.getItem(OUTPUT_MODE_KEY) === 'overwrite' ? 'overwrite' : 'subfolder';
  }

  async function chooseFiles() {
    if (!inTauri || busy) return;
    const extensions = capabilities.formats.flatMap((format) => format.extensions);
    const selected = await open({
      multiple: true,
      title: t.addFiles,
      filters: [{ name: capabilities.formats.map((format) => format.format.toUpperCase()).join(' / '), extensions }]
    });
    await addPaths(normalizeDialogSelection(selected));
  }

  async function chooseFolder() {
    if (!inTauri || busy) return;
    const selected = await open({ directory: true, multiple: true, title: t.addFolder });
    await addPaths(normalizeDialogSelection(selected));
  }

  function normalizeDialogSelection(value: string | string[] | null): string[] {
    if (!value) return [];
    return Array.isArray(value) ? value : [value];
  }

  async function addPaths(paths: string[]) {
    if (busy) return;
    await workflow.addPaths(paths);
  }

  function handleScanEvent(event: ScanEvent) {
    workflow.handleScan(event);
    if (event.type === 'finished' && !selectedId && queue.ids[0]) selectedId = queue.ids[0];
  }

  function clearItems() {
    if (busy) return;
    preview.cancel();
    queue.clear();
    selectedId = '';
    queueFilter = 'all';
    queueSearch = '';
    session.resetSummary();
    session.setNotice('');
  }

  function removeItem(id: string) {
    if (busy) return;
    const index = $queue.ids.indexOf(id);
    queue.remove(id);
    if (selectedId === id) {
      preview.cancel();
      selectedId = $queue.ids[Math.min(index, $queue.count - 1)] ?? '';
    }
  }

  function clearCompleted() {
    if (busy) return;
    queue.removeMany(queue.values().filter((item) => item.status === 'completed' || item.status === 'unchanged').map((item) => item.id));
    if (!queue.get(selectedId)) selectedId = queue.ids[0] ?? '';
    session.resetSummary();
    session.setNotice('');
    queueFilter = 'all';
    queueSearch = '';
  }

  function viewResults() {
    queueFilter = 'done';
    queueSearch = '';
    queueSort = 'saved';
    const first = queue.values().find((item) => item.status === 'completed' || item.status === 'unchanged');
    if (first) selectedId = first.id;
  }

  function retryFailed() {
    return workflow.retryItems(queue.values().filter((item) => item.status === 'failed').map((item) => item.id));
  }

  function setPreset(value: CompressionPreset) {
    preset = value;
    localStorage.setItem(PRESET_KEY, value);
  }

  function setAudioBitrate(value: number) {
    if (![64, 128, 192].includes(value)) return;
    audioBitrate = value;
    localStorage.setItem(AUDIO_BITRATE_KEY, String(value));
  }

  function setOutputMode(value: OutputMode) {
    outputMode = value;
    localStorage.setItem(OUTPUT_MODE_KEY, value);
  }

  function setOutputSubfolder(value: string) {
    outputSubfolder = value;
    localStorage.setItem(OUTPUT_FOLDER_KEY, value);
  }

  function setPreserveSupported(value: boolean) {
    metadataPolicy = value ? 'supported' : 'essential';
    localStorage.setItem(METADATA_KEY, metadataPolicy);
  }

  function cycleTheme() {
    const order: ThemePreference[] = ['system', 'light', 'dark'];
    theme = order[(order.indexOf(theme) + 1) % order.length];
    localStorage.setItem(THEME_KEY, theme);
    applyTheme(theme);
  }

  function toggleLanguage() {
    language = language === 'zh' ? 'en' : 'zh';
    localStorage.setItem(LANGUAGE_KEY, language);
    document.documentElement.lang = language === 'zh' ? 'zh-CN' : 'en';
  }

  async function showAbout() {
    if (!inTauri) return;
    const version = await getVersion();
    await message(interpolate(t.aboutText, { version }), { title: t.about, kind: 'info' });
  }

  async function reveal(pathOrItem: string | TaskItem) {
    const path = typeof pathOrItem === 'string'
      ? pathOrItem
      : pathOrItem.output_path ?? pathOrItem.source_path;
    try {
      await revealPath(path);
    } catch (error) {
      const appError = normalizeAppError(error);
      session.addIssues([appError]);
      session.setNotice(`${t.operationErrors.reveal}: ${errorText(appError, t)}`, true);
    }
  }

  function buildSummaryText(
    state: SessionState,
    queueCount: number,
    activeLanguage: Language,
    messages: Messages
  ): string {
    if (state.stopping) return messages.stopping;
    if (state.starting) return messages.starting;
    if (state.scanning) {
      return interpolate(messages.scanning, {
        visited: state.scanVisited,
        accepted: state.scanAccepted
      });
    }
    if (state.running) {
      return interpolate(messages.summaryRunning, { done: state.batchDone, total: state.batchTotal });
    }
    if (state.notice) return state.notice;
    if (state.lastSummary) {
      return interpolate(messages.summaryDone, {
        done: state.lastSummary.completed,
        unchanged: state.lastSummary.unchanged,
        failed: state.lastSummary.failed,
        cancelled: state.lastSummary.cancelled,
        saved: formatBytes(
          Math.max(0, state.lastSummary.original_bytes - state.lastSummary.output_bytes),
          activeLanguage
        )
      });
    }
    return queueCount
      ? interpolate(messages.summaryQueued, { count: queueCount })
      : messages.summaryReady;
  }

  async function registerDragDrop(unlisteners: Array<() => void>) {
    try {
      const unlisten = await getCurrentWindow().onDragDropEvent((event) => {
        if (event.payload.type === 'enter' || event.payload.type === 'over') {
          dropActive = !busy && folderValid;
          return;
        }
        dropActive = false;
        if (event.payload.type === 'drop') void addPaths(event.payload.paths);
      });
      unlisteners.push(unlisten);
    } catch (error) {
      const appError = normalizeAppError(error);
      session.addIssues([appError]);
      session.setNotice(`${t.operationErrors.scan}: ${errorText(appError, t)}`, true);
    }
  }

  async function registerIpcListeners(
    unlisteners: Array<() => void>,
    isDisposed: () => boolean
  ) {
    const registrations = await Promise.allSettled([
      listen<ScanEvent>('scan-event', (event) => handleScanEvent(event.payload)),
      listen<ItemProgress>('batch-item', (event) => workflow.handleProgress(event.payload)),
      listen<BatchSummary>('batch-summary', (event) => workflow.handleSummary(event.payload))
    ]);
    const readyUnlisteners = registrations.flatMap((registration) =>
      registration.status === 'fulfilled' ? [registration.value] : []
    );
    if (isDisposed()) {
      readyUnlisteners.forEach((unlisten) => unlisten());
      return;
    }
    const errors = registrations.flatMap((registration) =>
      registration.status === 'rejected' ? [normalizeAppError(registration.reason)] : []
    );
    if (errors.length) {
      readyUnlisteners.forEach((unlisten) => unlisten());
      session.addIssues(errors);
      session.setNotice(
        `${t.operationErrors.initialize}: ${errorText(errors[0], t)}`,
        true
      );
      return;
    }
    unlisteners.push(...readyUnlisteners);
    ipcReady = true;
  }

  onMount(() => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    applyTheme(theme);
    document.documentElement.lang = language === 'zh' ? 'zh-CN' : 'en';
    const media = matchMedia('(prefers-color-scheme: dark)');
    const handleSystemTheme = () => theme === 'system' && applyTheme(theme);
    media.addEventListener('change', handleSystemTheme);

    if (inTauri) {
      void getCapabilities().then((value) => capabilities = value).catch((error) => {
        session.addIssues([normalizeAppError(error)]);
      });
      void registerDragDrop(unlisteners);
      void registerIpcListeners(unlisteners, () => disposed);
    }

    return () => {
      disposed = true;
      media.removeEventListener('change', handleSystemTheme);
      preview.dispose();
      unlisteners.forEach((unlisten) => unlisten());
    };
  });
</script>

<svelte:head><title>image-slim</title></svelte:head>

<main class="app-shell" class:drop-active={dropActive} data-drop-text={t.dropText}>
  <Toolbar
    {t}
    {language}
    {theme}
    {busy}
    hasItems={$queue.count > 0}
    onAddFiles={chooseFiles}
    onAddFolder={chooseFolder}
    onClear={clearItems}
    onAbout={showAbout}
    onCycleTheme={cycleTheme}
    onToggleLanguage={toggleLanguage}
  />
  <div class="settings-section">
  <SettingsBar
    {t}
    {preset}
    {outputMode}
    {outputSubfolder}
    preserveSupported={metadataPolicy === 'supported'}
    disabled={busy}
    folderValid={subfolderNameValid}
    hasAudio={$queue.audioCount > 0}
    onPreset={setPreset}
    onOutputMode={setOutputMode}
    onOutputSubfolder={setOutputSubfolder}
    onPreserveSupported={setPreserveSupported}
  />

  <p class="settings-note">{t.presetTitles[preset]}<span> · {t.settingsNextRun}</span></p>
  <section class="audio-settings" aria-label={t.audioSettings}>
    <label><Music2 size={15} aria-hidden="true" /><span>{t.audioOutput}</span>
      <select aria-label={t.audioBitrate} value={audioBitrate} disabled={busy}
        on:change={(event) => setAudioBitrate(Number(event.currentTarget.value))}>
        <option value={192}>{t.audioBitrates.high}</option>
        <option value={128}>{t.audioBitrates.balanced}</option>
        <option value={64}>{t.audioBitrates.strong}</option>
      </select>
    </label>
    <span>{t.audioOutputHint}</span>
  </section>
  </div>

  <div class="workspace">
    <TaskList
      {t}
      ids={$queue.ids}
      version={$queue.version}
      totals={$queue.totals}
      bind:filter={queueFilter}
      bind:query={queueSearch}
      bind:sort={queueSort}
      formats={supportedFormats}
      {selectedId}
      {language}
      {busy}
      getItem={(id) => queue.get(id)}
      onSelect={(id) => selectedId = id}
      onRemove={removeItem}
      onRetry={(id) => workflow.retryItems([id])}
      onReveal={(item) => reveal(item)}
    />
    {#if selectedItem && isAudioFormat(selectedItem.format)}
      <AudioPreview {t} item={selectedItem} {language} paused={busy} />
    {:else}
    <ComparisonPreview
      {t}
      item={selectedItem}
      result={$preview.result}
      loading={$preview.loading}
      paused={busy}
      error={$preview.error}
      {language}
      {comparePosition}
      {zoom}
      onCompare={(value) => comparePosition = value}
      onZoom={(value) => zoom = value}
    />
    {/if}
  </div>

  <div class="results-area">
    <ResultSummary {t} {language} {busy} totals={$queue.totals}
      onViewResults={viewResults} onRetryFailed={retryFailed} onClearCompleted={clearCompleted} />
  </div>
  <footer class="status-bar">
    <div class="status-copy" title={summaryText} role="status">
      {#if $session.noticeIsError}<XCircle size={14} aria-hidden="true" />{/if}
      <span>{summaryText}</span>
    </div>
    <button
      class="issues-button"
      class:has-issues={$session.issues.length > 0}
      type="button"
      title={interpolate(t.issueCount, { count: $session.issues.length })}
      aria-label={interpolate(t.issueCount, { count: $session.issues.length })}
      on:click={() => issuesOpen = true}
    >
      <AlertTriangle size={15} aria-hidden="true" />
      <span>{$session.issues.length}</span>
    </button>
    <div class="batch-progress" class:active={$session.running} aria-hidden="true">
      <span style={`width:${$session.batchTotal ? ($session.batchDone / $session.batchTotal) * 100 : 0}%`}></span>
    </div>
    {#if $session.running || $session.scanning}
      <button class="cancel-button" type="button" disabled={$session.stopping} on:click={() => workflow.cancel()}>
        <CircleStop size={16} aria-hidden="true" />{$session.scanning ? t.cancelScan : t.cancel}
      </button>
    {:else}
      <button class="start-button" type="button" disabled={busy || !readyCount || !folderValid} on:click={() => workflow.runReady()}>
        <Play size={16} fill="currentColor" aria-hidden="true" />{$session.starting ? t.starting : readyCount ? interpolate(t.startCount, { count: readyCount }) : $queue.count ? t.allProcessed : t.start}
      </button>
    {/if}
  </footer>
</main>

{#if issuesOpen}
  <IssuePanel
    {t}
    issues={$session.issues}
    onClose={() => issuesOpen = false}
    onClear={() => session.clearIssues()}
    onReveal={(path) => reveal(path)}
  />
{/if}
