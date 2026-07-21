use crate::codecs;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::limits;
use crate::model::{CompressionPreset, InputItem, MetadataPolicy, PreviewRequest, PreviewResult};
use crate::output;
use crate::scheduler::WorkScheduler;
use image::ImageEncoder;
use image::imageops::FilterType;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct PreviewCache {
    entries: Arc<Mutex<VecDeque<CacheEntry>>>,
}

#[derive(Clone)]
struct CacheEntry {
    key: String,
    source_hash: String,
    candidate_hash: String,
    candidate_path: PathBuf,
    source_preview_path: PathBuf,
    candidate_preview_path: PathBuf,
    source_size: u64,
    candidate_size: u64,
    stored_bytes: u64,
}

impl PreviewCache {
    pub fn preview(
        &self,
        item: &InputItem,
        preset: CompressionPreset,
        metadata_policy: MetadataPolicy,
        source_hash: &str,
    ) -> Option<PreviewResult> {
        let key = cache_key(source_hash, preset, metadata_policy);
        let mut entries = self.entries.lock().expect("preview cache poisoned");
        let index = entries
            .iter()
            .position(|entry| entry.key == key && entry.source_hash == source_hash)?;
        let entry = entries.remove(index)?;
        if !valid_entry(&entry, true) {
            remove_entry_files(&entry);
            return None;
        }
        let result = PreviewResult {
            source_preview_path: crate::scanner::normalize_display_path(&entry.source_preview_path),
            candidate_preview_path: crate::scanner::normalize_display_path(
                &entry.candidate_preview_path,
            ),
            source_size: entry.source_size,
            candidate_size: entry.candidate_size,
            would_replace: entry.candidate_size < entry.source_size,
            cache_key: entry.key.clone(),
            width: item.width,
            height: item.height,
        };
        entries.push_front(entry);
        Some(result)
    }

    pub fn candidate(
        &self,
        item: &InputItem,
        preset: CompressionPreset,
        metadata_policy: MetadataPolicy,
        source_hash: &str,
    ) -> Option<Vec<u8>> {
        let key = cache_key(source_hash, preset, metadata_policy);
        let mut entries = self.entries.lock().expect("preview cache poisoned");
        let index = entries
            .iter()
            .position(|entry| entry.key == key && entry.source_hash == source_hash)?;
        let entry = entries.remove(index)?;
        let bytes = match fs::read(&entry.candidate_path) {
            Ok(bytes) => bytes,
            Err(_) => {
                remove_entry_files(&entry);
                return None;
            }
        };
        if item.original_size > limits::MAX_FILE_BYTES
            || bytes.len() as u64 != entry.candidate_size
            || output::content_hash(&bytes) != entry.candidate_hash
        {
            remove_entry_files(&entry);
            return None;
        }
        entries.push_front(entry);
        Some(bytes)
    }

    fn insert(&self, entry: CacheEntry) {
        let mut entries = self.entries.lock().expect("preview cache poisoned");
        if let Some(index) = entries.iter().position(|item| item.key == entry.key)
            && let Some(old) = entries.remove(index)
            && old.candidate_path != entry.candidate_path
        {
            remove_entry_files(&old);
        }
        entries.push_front(entry);
        while entries.len() > limits::PREVIEW_CACHE_MAX_ITEMS
            || entries.iter().map(|entry| entry.stored_bytes).sum::<u64>()
                > limits::PREVIEW_CACHE_MAX_BYTES
        {
            if let Some(old) = entries.pop_back() {
                remove_entry_files(&old);
            }
        }
    }

    pub fn clear(&self, cache_root: &Path) -> AppResult<()> {
        self.entries.lock().expect("preview cache poisoned").clear();
        let directory = cache_root.join("previews");
        if directory.exists() {
            fs::remove_dir_all(&directory).map_err(|error| AppError::io(error, &directory))?;
        }
        Ok(())
    }
}

pub fn create(
    cache_root: PathBuf,
    request: PreviewRequest,
    cancelled: Arc<AtomicBool>,
    scheduler: &WorkScheduler,
    cache: &PreviewCache,
) -> AppResult<PreviewResult> {
    scheduler.ensure_preview_allowed()?;
    crate::scanner::validate_runtime_item(&request.item)?;
    output::validate_item_mapping(&request.item).map_err(|error| {
        AppError::operation(ErrorCode::SourceChanged, error, &request.item.source_path)
            .retryable(true)
    })?;
    let _permit = scheduler.acquire_preview(&request.item)?;
    ensure_active(&cancelled)?;

    let source_path = Path::new(&request.item.source_path);
    let source = fs::read(source_path).map_err(|error| AppError::io(error, source_path))?;
    let source_hash = output::content_hash(&source);
    if let Some(result) = cache.preview(
        &request.item,
        request.preset,
        request.metadata_policy,
        &source_hash,
    ) {
        return Ok(result);
    }
    let candidate = codecs::compress(
        &source,
        request.item.format,
        (request.item.width, request.item.height),
        request.preset,
        request.metadata_policy,
        cancelled.clone(),
    )
    .map_err(|error| map_codec_error(error, &request.item, &cancelled))?;
    ensure_active(&cancelled)?;

    let key = cache_key(&source_hash, request.preset, request.metadata_policy);
    let directory = cache_root.join("previews");
    fs::create_dir_all(&directory).map_err(|error| AppError::io(error, &directory))?;
    let extension = request.item.format.extension();
    let candidate_path = directory.join(format!("{key}-candidate.{extension}"));
    let source_preview_path = directory.join(format!("{key}-source.png"));
    let candidate_preview_path = directory.join(format!("{key}-candidate.png"));
    let source_preview = display_preview(&source)?;
    ensure_active(&cancelled)?;
    let candidate_preview = display_preview(&candidate)?;

    let preflight = || ensure_active_anyhow(&cancelled);
    let write_result = (|| -> anyhow::Result<()> {
        output::atomic_write_guarded(&candidate_path, &candidate, preflight, preflight)?;
        output::atomic_write_guarded(&source_preview_path, &source_preview, preflight, preflight)?;
        output::atomic_write_guarded(
            &candidate_preview_path,
            &candidate_preview,
            preflight,
            preflight,
        )?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&candidate_path);
        let _ = fs::remove_file(&source_preview_path);
        let _ = fs::remove_file(&candidate_preview_path);
        return Err(map_cache_write_error(error, &request.item, &cancelled));
    }

    let candidate_hash = output::content_hash(&candidate);
    cache.insert(CacheEntry {
        key: key.clone(),
        source_hash,
        candidate_hash,
        candidate_path,
        source_preview_path: source_preview_path.clone(),
        candidate_preview_path: candidate_preview_path.clone(),
        source_size: source.len() as u64,
        candidate_size: candidate.len() as u64,
        stored_bytes: candidate
            .len()
            .saturating_add(source_preview.len())
            .saturating_add(candidate_preview.len()) as u64,
    });

    Ok(PreviewResult {
        source_preview_path: crate::scanner::normalize_display_path(&source_preview_path),
        candidate_preview_path: crate::scanner::normalize_display_path(&candidate_preview_path),
        source_size: source.len() as u64,
        candidate_size: candidate.len() as u64,
        would_replace: candidate.len() < source.len(),
        cache_key: key,
        width: request.item.width,
        height: request.item.height,
    })
}

fn cache_key(
    source_hash: &str,
    preset: CompressionPreset,
    metadata_policy: MetadataPolicy,
) -> String {
    let value = format!("{source_hash}:{preset:?}:{metadata_policy:?}");
    blake3::hash(value.as_bytes()).to_hex().to_string()
}

fn display_preview(source: &[u8]) -> AppResult<Vec<u8>> {
    let image = image::load_from_memory(source)
        .map_err(|error| AppError::new(ErrorCode::CodecFailed).detail(error))?;
    let resized =
        if image.width() > limits::PREVIEW_MAX_EDGE || image.height() > limits::PREVIEW_MAX_EDGE {
            image.resize(
                limits::PREVIEW_MAX_EDGE,
                limits::PREVIEW_MAX_EDGE,
                FilterType::Lanczos3,
            )
        } else {
            image
        };
    let rgba = resized.to_rgba8();
    let mut encoded = Vec::new();
    image::codecs::png::PngEncoder::new(&mut encoded)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|error| AppError::new(ErrorCode::CodecFailed).detail(error))?;
    Ok(encoded)
}

fn ensure_active(cancelled: &AtomicBool) -> AppResult<()> {
    if cancelled.load(Ordering::SeqCst) {
        Err(AppError::new(ErrorCode::Cancelled).retryable(true))
    } else {
        Ok(())
    }
}

fn ensure_active_anyhow(cancelled: &AtomicBool) -> anyhow::Result<()> {
    if cancelled.load(Ordering::SeqCst) {
        Err(anyhow::anyhow!("cancelled"))
    } else {
        Ok(())
    }
}

fn map_codec_error(error: anyhow::Error, item: &InputItem, cancelled: &AtomicBool) -> AppError {
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

fn map_cache_write_error(
    error: anyhow::Error,
    item: &InputItem,
    cancelled: &AtomicBool,
) -> AppError {
    if cancelled.load(Ordering::SeqCst) || error.to_string() == "cancelled" {
        AppError::new(ErrorCode::Cancelled)
            .path(&item.source_path)
            .retryable(true)
    } else {
        AppError::operation(ErrorCode::IoFailed, error, &item.source_path).retryable(true)
    }
}

fn remove_entry_files(entry: &CacheEntry) {
    let _ = fs::remove_file(&entry.candidate_path);
    let _ = fs::remove_file(&entry.source_preview_path);
    let _ = fs::remove_file(&entry.candidate_preview_path);
}

fn valid_entry(entry: &CacheEntry, require_previews: bool) -> bool {
    if require_previews
        && (!entry.source_preview_path.is_file() || !entry.candidate_preview_path.is_file())
    {
        return false;
    }
    let Ok(candidate) = fs::read(&entry.candidate_path) else {
        return false;
    };
    candidate.len() as u64 == entry.candidate_size
        && output::content_hash(&candidate) == entry.candidate_hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ImageFormat;
    use tempfile::tempdir;

    fn item(root: &Path) -> InputItem {
        InputItem {
            id: "preview".into(),
            source_path: root.join("source.png").to_string_lossy().into_owned(),
            input_root: root.to_string_lossy().into_owned(),
            relative_path: "source.png".into(),
            name: "source.png".into(),
            format: ImageFormat::Png,
            width: 2,
            height: 2,
            original_size: 3,
            modified_ms: 0,
        }
    }

    #[test]
    fn preview_cache_hits_and_invalidates_changed_candidates() {
        let temporary = tempdir().unwrap();
        let source_hash = output::content_hash(b"src");
        let key = cache_key(
            &source_hash,
            CompressionPreset::Balanced,
            MetadataPolicy::Essential,
        );
        let candidate_path = temporary.path().join("candidate.png");
        let source_preview_path = temporary.path().join("source-preview.png");
        let candidate_preview_path = temporary.path().join("candidate-preview.png");
        fs::write(&candidate_path, b"large").unwrap();
        fs::write(&source_preview_path, b"preview").unwrap();
        fs::write(&candidate_preview_path, b"preview").unwrap();
        let cache = PreviewCache::default();
        cache.insert(CacheEntry {
            key,
            source_hash: source_hash.clone(),
            candidate_hash: output::content_hash(b"large"),
            candidate_path: candidate_path.clone(),
            source_preview_path: source_preview_path.clone(),
            candidate_preview_path: candidate_preview_path.clone(),
            source_size: 3,
            candidate_size: 5,
            stored_bytes: 19,
        });

        let hit = cache
            .preview(
                &item(temporary.path()),
                CompressionPreset::Balanced,
                MetadataPolicy::Essential,
                &source_hash,
            )
            .unwrap();
        assert!(!hit.would_replace);
        assert_eq!(
            cache
                .candidate(
                    &item(temporary.path()),
                    CompressionPreset::Balanced,
                    MetadataPolicy::Essential,
                    &source_hash,
                )
                .unwrap(),
            b"large"
        );

        fs::write(&candidate_path, b"wrong").unwrap();
        assert!(
            cache
                .preview(
                    &item(temporary.path()),
                    CompressionPreset::Balanced,
                    MetadataPolicy::Essential,
                    &source_hash,
                )
                .is_none()
        );
        assert!(!candidate_path.exists());
        assert!(!source_preview_path.exists());
        assert!(!candidate_preview_path.exists());
    }
}
