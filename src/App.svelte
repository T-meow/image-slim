<script lang="ts">
  import { onMount } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { confirm, message, open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import { AlertTriangle, CircleStop, Play, XCircle } from 'lucide-svelte';
  import Toolbar from './components/Toolbar.svelte';
  import SettingsBar from './components/SettingsBar.svelte';
  import TaskList from './components/TaskList.svelte';
  import ComparisonPreview from './components/ComparisonPreview.svelte';
  import IssuePanel from './components/IssuePanel.svelte';
  import { copy, errorText, interpolate, type Messages } from './lib/i18n';
  import { formatBytes, validSubfolderName } from './lib/format';
  import { PreviewController } from './lib/preview-controller';
  import { QueueController } from './lib/queue';
  import { SessionController, type SessionState } from './lib/session';
  import {
    LANGUAGE_KEY,
    METADATA_KEY,
    OUTPUT_FOLDER_KEY,
    OUTPUT_MODE_KEY,
    PRESET_KEY,
    THEME_KEY,
    applyTheme,
    initialLanguage,
    initialMetadataPolicy,
    initialTheme
  } from './lib/settings';
  import {
    cancelBatch,
    cancelScan,
    getCapabilities,
    inTauri,
    normalizeAppError,
    revealPath,
    startBatch,
    startScan
  } from './lib/tauri';
  import type {
    AppCapabilities,
    AppError,
    BatchRequest,
    BatchSummary,
    CompressionPreset,
    InputItem,
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
      { format: 'webp', extensions: ['webp'] }
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
  let outputMode: OutputMode = readOutputMode();
  let outputSubfolder = localStorage.getItem(OUTPUT_FOLDER_KEY) || 'compressed';
  let metadataPolicy: MetadataPolicy = initialMetadataPolicy();
  let selectedId = '';
  let dropActive = false;
  let comparePosition = 50;
  let zoom = 1;
  let lastPreviewKey = '';
  let issuesOpen = false;
  let retryOriginal: TaskItem | undefined;
  let pendingProgress: ItemProgress[] = [];
  let pendingSummaries: BatchSummary[] = [];
  let ipcReady = !inTauri;

  $: t = copy[language];
  $: selectedItem = itemForVersion($queue.version, selectedId);
  $: supportedFormats = capabilities.formats
    .map((capability) => capability.format.toUpperCase())
    .join(language === 'zh' ? '、' : ' / ');
  $: subfolderNameValid = validSubfolderName(outputSubfolder);
  $: folderValid = outputMode === 'overwrite' || subfolderNameValid;
  $: previewKey = selectedItem
    && ipcReady
    && !$session.running
    && !$session.scanning
    && !$session.stopping
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
    if (!inTauri || !ipcReady) return;
    const extensions = capabilities.formats.flatMap((format) => format.extensions);
    const selected = await open({
      multiple: true,
      title: t.addFiles,
      filters: [{ name: capabilities.formats.map((format) => format.format.toUpperCase()).join(' / '), extensions }]
    });
    await addPaths(normalizeDialogSelection(selected));
  }

  async function chooseFolder() {
    if (!inTauri || !ipcReady) return;
    const selected = await open({ directory: true, multiple: true, title: t.addFolder });
    await addPaths(normalizeDialogSelection(selected));
  }

  function normalizeDialogSelection(value: string | string[] | null): string[] {
    if (!value) return [];
    return Array.isArray(value) ? value : [value];
  }

  async function addPaths(paths: string[]) {
    if (!paths.length || !folderValid || !ipcReady || $session.scanning) return;
    const scanId = crypto.randomUUID();
    session.beginScan(scanId);
    try {
      await startScan({
        scan_id: scanId,
        paths,
        output_subfolder: subfolderNameValid ? outputSubfolder.trim() : 'compressed',
        existing_ids: queue.existingIds(),
        remaining_capacity: Math.max(0, capabilities.limits.max_queue_items - $queue.count)
      });
    } catch (error) {
      const appError = normalizeAppError(error);
      session.addIssues([appError]);
      session.setNotice(`${t.operationErrors.scan}: ${errorText(appError, t)}`, true);
      session.finishScan(scanId);
    }
  }

  function handleScanEvent(event: ScanEvent) {
    if (event.scan_id !== $session.activeScanId) return;
    if (event.type === 'items') {
      const inputs = retryOriginal
        ? event.items.map((item) => samePath(item.source_path, retryOriginal!.source_path) ? {
            ...item,
            input_root: retryOriginal!.input_root,
            relative_path: retryOriginal!.relative_path
          } : item)
        : event.items;
      queue.merge(inputs);
      return;
    }
    if (event.type === 'issues') {
      session.addIssues(event.issues);
      return;
    }
    if (event.type === 'progress') {
      session.updateScan(event.visited, event.accepted, event.current_path);
      return;
    }

    session.finishScan(event.scan_id);
    if (event.cancelled) {
      session.setNotice(t.scanCancelled);
    } else {
      let notice = interpolate(t.summaryQueued, { count: event.accepted });
      if (event.issue_count) notice += interpolate(t.summaryWarnings, { count: event.issue_count });
      session.setNotice(notice);
    }
    if (!selectedId && $queue.ids[0]) selectedId = $queue.ids[0];
    if (retryOriginal) {
      const refreshed = queue.findByPath(retryOriginal.source_path);
      retryOriginal = undefined;
      if (refreshed) void runBatch([refreshed]);
    }
  }

  function clearItems() {
    const scanId = $session.activeScanId;
    if (scanId) {
      void cancelScan(scanId);
      session.finishScan(scanId);
    }
    preview.cancel();
    queue.clear();
    selectedId = '';
    retryOriginal = undefined;
    session.resetSummary();
    session.setNotice('');
  }

  function removeItem(id: string) {
    const index = $queue.ids.indexOf(id);
    queue.remove(id);
    if (selectedId === id) {
      preview.cancel();
      selectedId = $queue.ids[Math.min(index, $queue.count - 1)] ?? '';
    }
  }

  async function retryItem(id: string) {
    const item = queue.get(id);
    if (!item || $session.running || $session.scanning) return;
    retryOriginal = { ...item };
    queue.remove(id);
    if (selectedId === id) selectedId = '';
    await addPaths([item.source_path]);
  }

  function setPreset(value: CompressionPreset) {
    preset = value;
    localStorage.setItem(PRESET_KEY, value);
    resetResults();
  }

  function setOutputMode(value: OutputMode) {
    outputMode = value;
    localStorage.setItem(OUTPUT_MODE_KEY, value);
    resetResults();
  }

  function setOutputSubfolder(value: string) {
    outputSubfolder = value;
    localStorage.setItem(OUTPUT_FOLDER_KEY, value);
    resetResults();
  }

  function setPreserveSupported(value: boolean) {
    metadataPolicy = value ? 'supported' : 'essential';
    localStorage.setItem(METADATA_KEY, metadataPolicy);
    resetResults();
  }

  function resetResults() {
    if ($session.running) return;
    queue.resetResults();
    session.resetSummary();
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

  async function startReadyItems() {
    const pending = queue.readyItems();
    if (!pending.length) {
      session.setNotice(t.nothingToRun);
      return;
    }
    await runBatch(pending);
  }

  async function runBatch(batchItems: TaskItem[], allowConflicts = false) {
    if (!folderValid || $session.running || $session.scanning || !batchItems.length) return;
    const stoppingPreview = $preview.loading;
    preview.cancel();
    if (stoppingPreview) session.requestStop();
    const request: BatchRequest = {
      items: batchItems.map(stripTaskState),
      preset,
      output_mode: outputMode,
      output_subfolder: outputSubfolder.trim(),
      metadata_policy: metadataPolicy,
      allow_conflicts: allowConflicts
    };
    try {
      const result = await startBatch(request);
      if (result.status === 'conflicts') {
        session.clearStopping();
        const template = outputMode === 'overwrite' ? t.overwriteConfirm : t.outputConflictConfirm;
        const approved = await confirm(interpolate(template, { count: result.conflict_count }), {
          title: t.appName,
          kind: 'warning'
        });
        if (approved) {
          await runBatch(batchItems, true);
        } else {
          lastPreviewKey = '';
        }
        return;
      }
      const batchId = result.batch_id ?? '';
      session.beginBatch(batchId, batchItems.length);
      const bufferedProgress = pendingProgress.filter((event) => event.batch_id === batchId);
      pendingProgress = pendingProgress.filter((event) => event.batch_id !== batchId);
      bufferedProgress.forEach(handleItemProgress);
      const bufferedSummary = pendingSummaries.find((event) => event.batch_id === batchId);
      pendingSummaries = pendingSummaries.filter((event) => event.batch_id !== batchId);
      if (bufferedSummary) handleBatchSummary(bufferedSummary);
    } catch (error) {
      session.clearStopping();
      const appError = normalizeAppError(error);
      session.addIssues([appError]);
      session.setNotice(`${t.operationErrors.start}: ${errorText(appError, t)}`, true);
    }
  }

  function stripTaskState(item: TaskItem): InputItem {
    const { status: _status, output_path: _outputPath, output_size: _outputSize, saved_bytes: _savedBytes, error: _error, ...input } = item;
    return input;
  }

  async function cancelActiveWork() {
    session.requestStop();
    if ($session.activeScanId) await cancelScan($session.activeScanId);
    if ($session.activeBatchId) await cancelBatch($session.activeBatchId);
  }

  function handleItemProgress(progress: ItemProgress) {
    if (!$session.activeBatchId) {
      pendingProgress.push(progress);
      return;
    }
    if (progress.batch_id !== $session.activeBatchId) return;
    queue.update(progress);
    if (progress.error) session.addIssues([progress.error]);
    if (['completed', 'unchanged', 'failed', 'cancelled'].includes(progress.status)) {
      session.itemFinished(progress.item_id);
    }
  }

  function handleBatchSummary(summary: BatchSummary) {
    if (!$session.activeBatchId) {
      pendingSummaries.push(summary);
      return;
    }
    if (summary.batch_id !== $session.activeBatchId) return;
    session.finishBatch(summary);
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
    if (state.scanning) {
      return interpolate(messages.scanning, {
        visited: state.scanVisited,
        accepted: state.scanAccepted
      });
    }
    if (state.running) {
      return interpolate(messages.summaryRunning, { done: state.batchDone, total: state.batchTotal });
    }
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
    if (state.notice) return state.notice;
    return queueCount
      ? interpolate(messages.summaryQueued, { count: queueCount })
      : messages.summaryReady;
  }

  function samePath(left: string, right: string): boolean {
    return left.localeCompare(right, undefined, { sensitivity: 'base' }) === 0;
  }

  async function registerDragDrop(unlisteners: Array<() => void>) {
    try {
      const unlisten = await getCurrentWindow().onDragDropEvent((event) => {
        if (event.payload.type === 'enter' || event.payload.type === 'over') {
          dropActive = true;
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
      listen<ItemProgress>('batch-item', (event) => handleItemProgress(event.payload)),
      listen<BatchSummary>('batch-summary', (event) => handleBatchSummary(event.payload))
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
    busy={!ipcReady || $session.running || $session.scanning}
    hasItems={$queue.count > 0}
    onAddFiles={chooseFiles}
    onAddFolder={chooseFolder}
    onClear={clearItems}
    onAbout={showAbout}
    onCycleTheme={cycleTheme}
    onToggleLanguage={toggleLanguage}
  />
  <SettingsBar
    {t}
    {preset}
    {outputMode}
    {outputSubfolder}
    preserveSupported={metadataPolicy === 'supported'}
    disabled={$session.running || $session.scanning}
    folderValid={subfolderNameValid}
    onPreset={setPreset}
    onOutputMode={setOutputMode}
    onOutputSubfolder={setOutputSubfolder}
    onPreserveSupported={setPreserveSupported}
  />

  <div class="workspace">
    <TaskList
      {t}
      ids={$queue.ids}
      version={$queue.version}
      formats={supportedFormats}
      {selectedId}
      {language}
      busy={$session.running || $session.scanning}
      getItem={(id) => queue.get(id)}
      onSelect={(id) => selectedId = id}
      onRemove={removeItem}
      onRetry={retryItem}
      onReveal={(item) => reveal(item)}
    />
    <ComparisonPreview
      {t}
      item={selectedItem}
      result={$preview.result}
      loading={$preview.loading}
      error={$preview.error}
      {language}
      {comparePosition}
      {zoom}
      onCompare={(value) => comparePosition = value}
      onZoom={(value) => zoom = value}
    />
  </div>

  <footer class="status-bar">
    <div class="status-copy" title={summaryText}>
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
    <div class="batch-progress" aria-hidden={!$session.running}>
      <span style={`width:${$session.batchTotal ? ($session.batchDone / $session.batchTotal) * 100 : 0}%`}></span>
    </div>
    {#if $session.running || $session.scanning}
      <button class="cancel-button" type="button" disabled={$session.stopping} on:click={cancelActiveWork}>
        <CircleStop size={16} aria-hidden="true" />{$session.scanning ? t.cancelScan : t.cancel}
      </button>
    {:else}
      <button class="start-button" type="button" disabled={!ipcReady || $session.stopping || !$queue.count || !folderValid} on:click={startReadyItems}>
        <Play size={16} fill="currentColor" aria-hidden="true" />{t.start}
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
