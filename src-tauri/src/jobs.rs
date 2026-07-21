use crate::codecs;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::limits;
use crate::model::{
    BatchRequest, BatchStartResult, BatchStartStatus, BatchSummary, CompressionPreset, InputItem,
    ItemProgress, MetadataPolicy, OutputMode, TaskStatus,
};
use crate::output;
use crate::preview::PreviewCache;
use crate::scheduler::{BatchGuard, WorkScheduler};
use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

const WORKER_COUNT: usize = 2;
type ActivePreview = Option<(String, Arc<AtomicBool>)>;

#[derive(Clone, Default)]
pub struct JobRegistry {
    jobs: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

#[derive(Clone, Default)]
pub struct PreviewRegistry {
    active: Arc<Mutex<ActivePreview>>,
    execution: Arc<Mutex<()>>,
}

impl PreviewRegistry {
    pub fn begin(&self, request_id: String) -> Arc<AtomicBool> {
        let mut active = self.active.lock().expect("preview registry poisoned");
        if let Some((_, cancelled)) = active.take() {
            cancelled.store(true, Ordering::SeqCst);
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        *active = Some((request_id, cancelled.clone()));
        cancelled
    }

    pub fn finish(&self, request_id: &str) {
        let mut active = self.active.lock().expect("preview registry poisoned");
        if matches!(active.as_ref(), Some((active_id, _)) if active_id == request_id) {
            active.take();
        }
    }

    pub fn cancel(&self) -> bool {
        let mut active = self.active.lock().expect("preview registry poisoned");
        if let Some((_, cancelled)) = active.take() {
            cancelled.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    pub fn execution(&self) -> Arc<Mutex<()>> {
        self.execution.clone()
    }
}

impl JobRegistry {
    pub fn cancel(&self, batch_id: &str) -> bool {
        let jobs = self.jobs.lock().expect("job registry poisoned");
        if let Some(flag) = jobs.get(batch_id) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    fn try_insert(&self, batch_id: String, flag: Arc<AtomicBool>) -> bool {
        let mut jobs = self.jobs.lock().expect("job registry poisoned");
        if !jobs.is_empty() {
            return false;
        }
        jobs.insert(batch_id, flag);
        true
    }

    fn remove(&self, batch_id: &str) {
        self.jobs
            .lock()
            .expect("job registry poisoned")
            .remove(batch_id);
    }
}

#[derive(Default)]
struct Counters {
    completed: usize,
    unchanged: usize,
    failed: usize,
    cancelled: usize,
    original_bytes: u64,
    output_bytes: u64,
}

#[derive(Clone)]
struct BatchOptions {
    preset: CompressionPreset,
    output_mode: OutputMode,
    output_subfolder: String,
    metadata_policy: MetadataPolicy,
    allow_conflicts: bool,
    expected_conflicts: HashMap<String, crate::model::SourceFingerprint>,
}

impl From<&BatchRequest> for BatchOptions {
    fn from(request: &BatchRequest) -> Self {
        Self {
            preset: request.preset,
            output_mode: request.output_mode,
            output_subfolder: request.output_subfolder.clone(),
            metadata_policy: request.metadata_policy,
            allow_conflicts: request.allow_conflicts,
            expected_conflicts: HashMap::new(),
        }
    }
}

pub fn start(
    app: AppHandle,
    registry: JobRegistry,
    scheduler: WorkScheduler,
    cache: PreviewCache,
    request: BatchRequest,
) -> AppResult<BatchStartResult> {
    if request.items.is_empty() {
        return Err(AppError::new(ErrorCode::Internal).detail("The batch is empty"));
    }
    limits::validate_queue_size(request.items.len())?;
    if request.output_mode == OutputMode::Subfolder {
        crate::scanner::validate_output_subfolder(&request.output_subfolder)?;
    }
    for item in &request.items {
        limits::validate_item(item)?;
        output::validate_item_mapping(item).map_err(|error| {
            AppError::operation(ErrorCode::SourceChanged, error, &item.source_path).retryable(true)
        })?;
    }

    let mut options = BatchOptions::from(&request);
    let targets = request
        .items
        .iter()
        .map(|item| {
            output::output_path(item, options.output_mode, &options.output_subfolder)
                .map_err(AppError::internal)
        })
        .collect::<AppResult<Vec<_>>>()?;
    let conflict_count = targets.iter().filter(|target| target.exists()).count();
    if conflict_count > 0 && !options.allow_conflicts {
        return Ok(BatchStartResult {
            status: BatchStartStatus::Conflicts,
            batch_id: None,
            conflict_count,
        });
    }
    if options.output_mode == OutputMode::Subfolder && options.allow_conflicts {
        for target in targets.iter().filter(|target| target.exists()) {
            if !target.is_file() {
                return Err(AppError::new(ErrorCode::OutputConflict)
                    .path(target)
                    .detail("The output target is not a regular file")
                    .retryable(true));
            }
            let fingerprint = output::fingerprint(target).map_err(|error| {
                AppError::new(ErrorCode::OutputConflict)
                    .path(target)
                    .detail(error)
                    .retryable(true)
            })?;
            options
                .expected_conflicts
                .insert(conflict_key(target), fingerprint);
        }
    }

    let mut cleaned_directories = HashSet::new();
    for target in &targets {
        if let Some(parent) = target.parent() {
            let parent = parent.to_path_buf();
            if cleaned_directories.insert(parent.clone()) {
                let _ = output::cleanup_stale_temporary_files(&parent);
            }
        }
    }

    let batch_id = Uuid::new_v4().to_string();
    let cancelled = Arc::new(AtomicBool::new(false));
    if !registry.try_insert(batch_id.clone(), cancelled.clone()) {
        return Err(AppError::new(ErrorCode::BatchRunning).retryable(true));
    }
    let batch_guard = match scheduler.begin_batch() {
        Ok(guard) => guard,
        Err(error) => {
            registry.remove(&batch_id);
            return Err(error);
        }
    };

    let worker_batch_id = batch_id.clone();
    let items = Arc::new(request.items);
    let options = Arc::new(options);
    std::thread::spawn(move || {
        run_batch(
            app,
            registry,
            scheduler,
            cache,
            items,
            options,
            worker_batch_id,
            cancelled,
            batch_guard,
        );
    });

    Ok(BatchStartResult {
        status: BatchStartStatus::Started,
        batch_id: Some(batch_id),
        conflict_count,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_batch(
    app: AppHandle,
    registry: JobRegistry,
    scheduler: WorkScheduler,
    cache: PreviewCache,
    items: Arc<Vec<InputItem>>,
    options: Arc<BatchOptions>,
    batch_id: String,
    cancelled: Arc<AtomicBool>,
    batch_guard: BatchGuard,
) {
    let next_index = Arc::new(AtomicUsize::new(0));
    let counters = Arc::new(Mutex::new(Counters::default()));

    std::thread::scope(|scope| {
        for _ in 0..WORKER_COUNT.min(items.len()) {
            let app = app.clone();
            let items = items.clone();
            let options = options.clone();
            let next_index = next_index.clone();
            let counters = counters.clone();
            let cancelled = cancelled.clone();
            let batch_id = batch_id.clone();
            let scheduler = scheduler.clone();
            let cache = cache.clone();
            scope.spawn(move || {
                loop {
                    let index = next_index.fetch_add(1, Ordering::SeqCst);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    if cancelled.load(Ordering::SeqCst) {
                        emit_cancelled(&app, &batch_id, item, &counters);
                        continue;
                    }
                    process_item(
                        &app,
                        &batch_id,
                        item,
                        &options,
                        cancelled.clone(),
                        &counters,
                        &scheduler,
                        &cache,
                    );
                }
            });
        }
    });

    let counters = counters.lock().expect("batch counters poisoned");
    let summary = BatchSummary {
        batch_id: batch_id.clone(),
        completed: counters.completed,
        unchanged: counters.unchanged,
        failed: counters.failed,
        cancelled: counters.cancelled,
        original_bytes: counters.original_bytes,
        output_bytes: counters.output_bytes,
    };
    drop(batch_guard);
    registry.remove(&batch_id);
    let _ = app.emit("batch-summary", summary);
}

#[allow(clippy::too_many_arguments)]
fn process_item(
    app: &AppHandle,
    batch_id: &str,
    item: &InputItem,
    options: &BatchOptions,
    cancelled: Arc<AtomicBool>,
    counters: &Mutex<Counters>,
    scheduler: &WorkScheduler,
    cache: &PreviewCache,
) {
    let _ = app.emit(
        "batch-item",
        ItemProgress {
            batch_id: batch_id.into(),
            item_id: item.id.clone(),
            status: TaskStatus::Processing,
            output_path: None,
            output_size: None,
            saved_bytes: 0,
            error: None,
        },
    );

    let outcome = process_item_inner(item, options, cancelled.clone(), scheduler, cache);
    let progress = match outcome {
        Ok((status, output_path, output_size)) => {
            let mut totals = counters.lock().expect("batch counters poisoned");
            totals.original_bytes = totals.original_bytes.saturating_add(item.original_size);
            totals.output_bytes = totals.output_bytes.saturating_add(output_size);
            if status == TaskStatus::Completed {
                totals.completed += 1;
            } else {
                totals.unchanged += 1;
            }
            ItemProgress {
                batch_id: batch_id.into(),
                item_id: item.id.clone(),
                status,
                output_path: Some(output_path),
                output_size: Some(output_size),
                saved_bytes: item.original_size.saturating_sub(output_size),
                error: None,
            }
        }
        Err(error) if error.code == ErrorCode::Cancelled => {
            counters.lock().expect("batch counters poisoned").cancelled += 1;
            ItemProgress {
                batch_id: batch_id.into(),
                item_id: item.id.clone(),
                status: TaskStatus::Cancelled,
                output_path: None,
                output_size: None,
                saved_bytes: 0,
                error: None,
            }
        }
        Err(error) => {
            counters.lock().expect("batch counters poisoned").failed += 1;
            ItemProgress {
                batch_id: batch_id.into(),
                item_id: item.id.clone(),
                status: TaskStatus::Failed,
                output_path: None,
                output_size: None,
                saved_bytes: 0,
                error: Some(error),
            }
        }
    };
    let _ = app.emit("batch-item", progress);
}

fn process_item_inner(
    item: &InputItem,
    options: &BatchOptions,
    cancelled: Arc<AtomicBool>,
    scheduler: &WorkScheduler,
    cache: &PreviewCache,
) -> AppResult<(TaskStatus, String, u64)> {
    crate::scanner::validate_runtime_item(item)?;
    output::assert_unchanged(item).map_err(|error| source_changed(item, error))?;
    let _permit = scheduler.acquire_batch(item)?;
    ensure_active(&cancelled, item)?;

    let source_path = Path::new(&item.source_path);
    let source = fs::read(source_path).map_err(|error| AppError::io(error, source_path))?;
    let source_hash = output::content_hash(&source);
    let encoded = if let Some(candidate) =
        cache.candidate(item, options.preset, options.metadata_policy, &source_hash)
    {
        candidate
    } else {
        codecs::compress(
            &source,
            item.format,
            (item.width, item.height),
            options.preset,
            options.metadata_policy,
            cancelled.clone(),
        )
        .map_err(|error| codec_error(item, error, &cancelled))?
    };
    ensure_active(&cancelled, item)?;

    let target = output::output_path(item, options.output_mode, &options.output_subfolder)
        .map_err(AppError::internal)?;
    let preflight = || -> Result<()> {
        if cancelled.load(Ordering::SeqCst) {
            return Err(anyhow!("cancelled"));
        }
        output::assert_unchanged(item)
    };
    let final_guard = || -> Result<()> {
        if cancelled.load(Ordering::SeqCst) {
            return Err(anyhow!("cancelled"));
        }
        output::assert_content_unchanged(item, &source_hash)?;
        validate_output_conflict(options, &target)?;
        output::validate_output_target(
            item,
            options.output_mode,
            &options.output_subfolder,
            &target,
        )
    };

    if encoded.len() >= source.len() {
        if options.output_mode == OutputMode::Subfolder {
            output::atomic_write_guarded(&target, &source, preflight, final_guard)
                .map_err(|error| output_error(item, error, &cancelled))?;
        } else {
            final_guard().map_err(|error| output_error(item, error, &cancelled))?;
        }
        return Ok((
            TaskStatus::Unchanged,
            crate::scanner::normalize_display_path(
                if options.output_mode == OutputMode::Overwrite {
                    source_path
                } else {
                    &target
                },
            ),
            source.len() as u64,
        ));
    }

    output::atomic_write_guarded(&target, &encoded, preflight, final_guard)
        .map_err(|error| output_error(item, error, &cancelled))?;
    Ok((
        TaskStatus::Completed,
        crate::scanner::normalize_display_path(&target),
        encoded.len() as u64,
    ))
}

fn emit_cancelled(app: &AppHandle, batch_id: &str, item: &InputItem, counters: &Mutex<Counters>) {
    counters.lock().expect("batch counters poisoned").cancelled += 1;
    let _ = app.emit(
        "batch-item",
        ItemProgress {
            batch_id: batch_id.into(),
            item_id: item.id.clone(),
            status: TaskStatus::Cancelled,
            output_path: None,
            output_size: None,
            saved_bytes: 0,
            error: None,
        },
    );
}

fn ensure_active(cancelled: &AtomicBool, item: &InputItem) -> AppResult<()> {
    if cancelled.load(Ordering::SeqCst) {
        Err(AppError::new(ErrorCode::Cancelled)
            .path(&item.source_path)
            .retryable(true))
    } else {
        Ok(())
    }
}

fn source_changed(item: &InputItem, error: anyhow::Error) -> AppError {
    AppError::operation(ErrorCode::SourceChanged, error, &item.source_path).retryable(true)
}

fn codec_error(item: &InputItem, error: anyhow::Error, cancelled: &AtomicBool) -> AppError {
    if cancelled.load(Ordering::SeqCst) || error.to_string() == "cancelled" {
        AppError::new(ErrorCode::Cancelled)
            .path(&item.source_path)
            .retryable(true)
    } else {
        AppError::new(ErrorCode::CodecFailed)
            .path(&item.source_path)
            .detail(error)
            .retryable(true)
    }
}

fn output_error(item: &InputItem, error: anyhow::Error, cancelled: &AtomicBool) -> AppError {
    let detail = error.to_string();
    if cancelled.load(Ordering::SeqCst) || detail == "cancelled" {
        AppError::new(ErrorCode::Cancelled)
            .path(&item.source_path)
            .retryable(true)
    } else if detail.contains("Source file changed") {
        source_changed(item, error)
    } else if detail.contains("Output conflict") {
        AppError::new(ErrorCode::OutputConflict)
            .path(&item.source_path)
            .detail(error)
            .retryable(true)
    } else {
        AppError::operation(ErrorCode::IoFailed, error, &item.source_path).retryable(true)
    }
}

fn validate_output_conflict(options: &BatchOptions, target: &Path) -> Result<()> {
    if options.output_mode != OutputMode::Subfolder || !target.exists() {
        return Ok(());
    }
    let expected = options
        .expected_conflicts
        .get(&conflict_key(target))
        .ok_or_else(|| anyhow!("Output conflict: a new output file appeared"))?;
    let current = output::fingerprint(target)?;
    if current != *expected {
        return Err(anyhow!("Output conflict: an existing output file changed"));
    }
    Ok(())
}

fn conflict_key(path: &Path) -> String {
    crate::scanner::normalize_display_path(path).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ImageFormat;
    use tempfile::tempdir;
    use webpx::{Encoder, Unstoppable};

    #[test]
    fn starting_a_preview_cancels_the_previous_request() {
        let registry = PreviewRegistry::default();
        let first = registry.begin("first".into());
        let second = registry.begin("second".into());
        assert!(first.load(Ordering::SeqCst));
        assert!(!second.load(Ordering::SeqCst));
        registry.finish("first");
        let third = registry.begin("third".into());
        assert!(second.load(Ordering::SeqCst));
        assert!(!third.load(Ordering::SeqCst));
        assert!(registry.cancel());
        assert!(third.load(Ordering::SeqCst));
    }

    #[test]
    fn no_gain_copies_the_original_to_the_output_folder() {
        let temporary = tempdir().unwrap();
        let source_path = temporary.path().join("tiny.webp");
        let seed = Encoder::new_rgba(&[1, 2, 3, 255], 1, 1)
            .lossless(true)
            .method(6)
            .exact(true)
            .encode(Unstoppable)
            .unwrap();
        let source = codecs::compress(
            &seed,
            ImageFormat::Webp,
            (1, 1),
            CompressionPreset::Lossless,
            MetadataPolicy::Essential,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        fs::write(&source_path, &source).unwrap();
        let metadata = fs::metadata(&source_path).unwrap();
        let item = InputItem {
            id: "tiny".into(),
            source_path: crate::scanner::normalize_display_path(&source_path),
            input_root: crate::scanner::normalize_display_path(temporary.path()),
            relative_path: "tiny.webp".into(),
            name: "tiny.webp".into(),
            format: ImageFormat::Webp,
            width: 1,
            height: 1,
            original_size: metadata.len(),
            modified_ms: crate::scanner::modified_ms(metadata.modified().ok()),
        };
        let options = BatchOptions {
            preset: CompressionPreset::Lossless,
            output_mode: OutputMode::Subfolder,
            output_subfolder: "compressed".into(),
            metadata_policy: MetadataPolicy::Essential,
            allow_conflicts: false,
            expected_conflicts: HashMap::new(),
        };
        let scheduler = WorkScheduler::with_budget(8 * 1024 * 1024 * 1024);
        let (status, output_path, output_size) = process_item_inner(
            &item,
            &options,
            Arc::new(AtomicBool::new(false)),
            &scheduler,
            &PreviewCache::default(),
        )
        .unwrap();
        assert_eq!(status, TaskStatus::Unchanged);
        assert_eq!(output_size, source.len() as u64);
        assert_eq!(fs::read(output_path).unwrap(), source);
    }

    #[test]
    fn batch_processing_rejects_forged_dimensions_before_decoding() {
        let temporary = tempdir().unwrap();
        let source_path = temporary.path().join("forged.png");
        let file = fs::File::create(&source_path).unwrap();
        let mut encoder = png::Encoder::new(file, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255; 16]).unwrap();
        drop(writer);
        let metadata = fs::metadata(&source_path).unwrap();
        let item = InputItem {
            id: "forged".into(),
            source_path: crate::scanner::normalize_display_path(&source_path),
            input_root: crate::scanner::normalize_display_path(temporary.path()),
            relative_path: "forged.png".into(),
            name: "forged.png".into(),
            format: ImageFormat::Png,
            width: 1,
            height: 2,
            original_size: metadata.len(),
            modified_ms: crate::scanner::modified_ms(metadata.modified().ok()),
        };
        let options = BatchOptions {
            preset: CompressionPreset::Balanced,
            output_mode: OutputMode::Subfolder,
            output_subfolder: "compressed".into(),
            metadata_policy: MetadataPolicy::Essential,
            allow_conflicts: false,
            expected_conflicts: HashMap::new(),
        };
        let scheduler = WorkScheduler::with_budget(8 * 1024 * 1024 * 1024);

        let error = process_item_inner(
            &item,
            &options,
            Arc::new(AtomicBool::new(false)),
            &scheduler,
            &PreviewCache::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::SourceChanged);
        assert!(!temporary.path().join("compressed").exists());
    }

    #[test]
    fn detects_new_and_changed_output_conflicts() {
        let temporary = tempdir().unwrap();
        let target = temporary.path().join("output.png");
        let mut options = BatchOptions {
            preset: CompressionPreset::Balanced,
            output_mode: OutputMode::Subfolder,
            output_subfolder: "compressed".into(),
            metadata_policy: MetadataPolicy::Essential,
            allow_conflicts: true,
            expected_conflicts: HashMap::new(),
        };

        fs::write(&target, b"first").unwrap();
        assert!(validate_output_conflict(&options, &target).is_err());
        options
            .expected_conflicts
            .insert(conflict_key(&target), output::fingerprint(&target).unwrap());
        assert!(validate_output_conflict(&options, &target).is_ok());
        fs::write(&target, b"changed").unwrap();
        assert!(validate_output_conflict(&options, &target).is_err());
    }
}
