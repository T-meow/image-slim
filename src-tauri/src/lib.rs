use image_slim_core::EventSink;
use image_slim_core::batch::{self, JobRegistry, PreviewRegistry};
use image_slim_core::error::{AppError, AppResult, ErrorCode};
use image_slim_core::model::{
    AppCapabilities, BatchRequest, BatchStartResult, BatchSummary, ItemProgress, PreviewRequest,
    PreviewResult, ScanEvent, ScanRequest,
};
use image_slim_core::preview::{self, PreviewCache};
use image_slim_core::scanner::{self, ScanRegistry};
use image_slim_core::scheduler::WorkScheduler;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone)]
struct TauriEventSink {
    app: AppHandle,
}

impl EventSink for TauriEventSink {
    fn scan_event(&self, event: ScanEvent) {
        let _ = self.app.emit("scan-event", event);
    }

    fn item_progress(&self, progress: ItemProgress) {
        let _ = self.app.emit("batch-item", progress);
    }

    fn batch_summary(&self, summary: BatchSummary) {
        let _ = self.app.emit("batch-summary", summary);
    }
}

#[tauri::command]
fn start_scan(
    app: AppHandle,
    registry: State<'_, ScanRegistry>,
    request: ScanRequest,
) -> AppResult<()> {
    if request.scan_id.trim().is_empty() || request.paths.is_empty() {
        return Err(AppError::new(ErrorCode::Internal).detail("The scan request is empty"));
    }
    let registry = registry.inner().clone();
    let scan_id = request.scan_id.clone();
    let cancelled = registry.begin(scan_id.clone());
    let sink = TauriEventSink { app };
    std::thread::spawn(move || {
        let result = scanner::scan_stream(request, cancelled.clone(), |event| {
            sink.scan_event(event);
        });
        if let Err(error) = result {
            sink.scan_event(ScanEvent::Issues {
                scan_id: scan_id.clone(),
                issues: vec![error],
            });
            sink.scan_event(ScanEvent::Finished {
                scan_id: scan_id.clone(),
                accepted: 0,
                issue_count: 1,
                cancelled: cancelled.load(std::sync::atomic::Ordering::SeqCst),
                limit_reached: false,
            });
        }
        registry.finish(&scan_id);
    });
    Ok(())
}

#[tauri::command]
fn cancel_scan(registry: State<'_, ScanRegistry>, scan_id: String) -> bool {
    registry.cancel(&scan_id)
}

#[tauri::command]
async fn create_preview(
    app: AppHandle,
    registry: State<'_, PreviewRegistry>,
    scheduler: State<'_, WorkScheduler>,
    cache: State<'_, PreviewCache>,
    request: PreviewRequest,
) -> AppResult<PreviewResult> {
    scheduler.ensure_preview_allowed()?;
    let registry = registry.inner().clone();
    let scheduler = scheduler.inner().clone();
    let cache = cache.inner().clone();
    let request_id = request.request_id.clone();
    let cancelled = registry.begin(request_id.clone());
    let execution = registry.execution();
    let cache_root = app.path().app_cache_dir().map_err(AppError::internal)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _execution_guard = execution.lock().expect("preview execution poisoned");
        if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(AppError::new(ErrorCode::Cancelled).retryable(true));
        }
        preview::create(cache_root, request, cancelled, &scheduler, &cache)
    })
    .await
    .map_err(AppError::internal)?;
    registry.finish(&request_id);
    result
}

#[tauri::command]
fn cancel_preview(registry: State<'_, PreviewRegistry>) -> bool {
    registry.cancel()
}

#[tauri::command]
fn start_batch(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    preview_registry: State<'_, PreviewRegistry>,
    scheduler: State<'_, WorkScheduler>,
    cache: State<'_, PreviewCache>,
    request: BatchRequest,
) -> AppResult<BatchStartResult> {
    preview_registry.cancel();
    batch::start(
        Arc::new(TauriEventSink { app }),
        registry.inner().clone(),
        scheduler.inner().clone(),
        cache.inner().clone(),
        request,
    )
}

#[tauri::command]
fn cancel_batch(registry: State<'_, JobRegistry>, batch_id: String) -> bool {
    registry.cancel(&batch_id)
}

#[tauri::command]
fn get_capabilities() -> AppCapabilities {
    image_slim_core::capabilities()
}

#[tauri::command]
fn reveal_path(path: String) -> AppResult<()> {
    let selected = Path::new(&path);
    let target = if selected.is_dir() {
        selected
    } else {
        selected.parent().unwrap_or(selected)
    };
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(target)
            .spawn()
            .map_err(|error| AppError::io(error, target))?;
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(JobRegistry::default())
        .manage(PreviewRegistry::default())
        .manage(ScanRegistry::default())
        .manage(WorkScheduler::default())
        .manage(PreviewCache::default())
        .setup(|app| {
            let cache_root = app.path().app_cache_dir()?;
            let cache = app.state::<PreviewCache>();
            let _ = cache.clear(&cache_root);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_scan,
            cancel_scan,
            create_preview,
            cancel_preview,
            start_batch,
            cancel_batch,
            get_capabilities,
            reveal_path,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run image-slim");
}
