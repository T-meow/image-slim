use crate::error::{AppError, AppResult, ErrorCode};
use crate::limits;
use crate::model::{ImageFormat, InputItem, ScanEvent, ScanRequest};
use anyhow::{Context, Result, anyhow};
use image::ImageReader;
use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::{DirEntry, WalkDir};

const HEADER_LIMIT: u64 = 512 * 1024;
const EVENT_CHUNK_SIZE: usize = 100;

#[derive(Clone, Default)]
pub struct ScanRegistry {
    scans: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl ScanRegistry {
    pub fn begin(&self, scan_id: String) -> Arc<AtomicBool> {
        let mut scans = self.scans.lock().expect("scan registry poisoned");
        for flag in scans.values() {
            flag.store(true, Ordering::SeqCst);
        }
        scans.clear();
        let cancelled = Arc::new(AtomicBool::new(false));
        scans.insert(scan_id, cancelled.clone());
        cancelled
    }

    pub fn cancel(&self, scan_id: &str) -> bool {
        let scans = self.scans.lock().expect("scan registry poisoned");
        scans.get(scan_id).is_some_and(|flag| {
            flag.store(true, Ordering::SeqCst);
            true
        })
    }

    pub fn finish(&self, scan_id: &str) {
        self.scans
            .lock()
            .expect("scan registry poisoned")
            .remove(scan_id);
    }
}

#[derive(Debug)]
enum ScanRoot {
    Directory(PathBuf),
    File { path: PathBuf, root: PathBuf },
}

pub fn scan_stream(
    request: ScanRequest,
    cancelled: Arc<AtomicBool>,
    mut emit: impl FnMut(ScanEvent),
) -> AppResult<()> {
    validate_output_subfolder(&request.output_subfolder)?;
    let scan_id = request.scan_id.clone();
    let mut issues = Vec::new();
    let roots = prepare_roots(&request.paths, &mut issues);
    let existing = request.existing_ids.into_iter().collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let capacity = request.remaining_capacity.min(limits::MAX_QUEUE_ITEMS);
    let mut pending_items = Vec::with_capacity(EVENT_CHUNK_SIZE);
    let mut pending_issues = issues;
    let mut visited = 0usize;
    let mut accepted = 0usize;
    let mut issue_count = pending_issues.len();
    let mut limit_reached = capacity == 0;
    let mut last_path = String::new();

    flush_issues(&scan_id, &mut pending_issues, &mut emit);

    'roots: for root in roots {
        if cancelled.load(Ordering::Relaxed) || limit_reached {
            break;
        }
        match root {
            ScanRoot::File { path, root } => {
                visited += 1;
                last_path = normalize_display_path(&path);
                match inspect_input(&path, &root, true) {
                    Ok(Some(item)) if seen.insert(item.id.clone()) => {
                        let is_existing = existing.contains(&item.id);
                        pending_items.push(item);
                        if !is_existing {
                            accepted += 1;
                        }
                    }
                    Ok(_) => {}
                    Err(issue) => {
                        pending_issues.push(issue);
                        issue_count += 1;
                    }
                }
                if accepted >= capacity {
                    limit_reached = true;
                }
                flush_if_needed(
                    &scan_id,
                    &mut pending_items,
                    &mut pending_issues,
                    visited,
                    accepted,
                    &last_path,
                    &mut emit,
                );
            }
            ScanRoot::Directory(root) => {
                let output_root = root.join(&request.output_subfolder);
                let walker = WalkDir::new(&root)
                    .follow_links(false)
                    .sort_by_file_name()
                    .into_iter()
                    .filter_entry(|entry| include_entry(entry, &output_root));
                for entry in walker {
                    if cancelled.load(Ordering::Relaxed) {
                        break 'roots;
                    }
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(error) => {
                            issue_count += 1;
                            pending_issues.push(
                                AppError::new(ErrorCode::WalkError)
                                    .detail(error)
                                    .retryable(true),
                            );
                            continue;
                        }
                    };
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    visited += 1;
                    last_path = normalize_display_path(entry.path());
                    match inspect_input(entry.path(), &root, false) {
                        Ok(Some(item)) if seen.insert(item.id.clone()) => {
                            let is_existing = existing.contains(&item.id);
                            pending_items.push(item);
                            if !is_existing {
                                accepted += 1;
                            }
                        }
                        Ok(_) => {}
                        Err(issue) => {
                            pending_issues.push(issue);
                            issue_count += 1;
                        }
                    }
                    if accepted >= capacity {
                        limit_reached = true;
                    }
                    flush_if_needed(
                        &scan_id,
                        &mut pending_items,
                        &mut pending_issues,
                        visited,
                        accepted,
                        &last_path,
                        &mut emit,
                    );
                    if limit_reached {
                        break 'roots;
                    }
                }
            }
        }
    }

    if limit_reached {
        issue_count += 1;
        pending_issues.push(
            AppError::new(ErrorCode::QueueLimitReached).param("limit", limits::MAX_QUEUE_ITEMS),
        );
    }
    flush_items(&scan_id, &mut pending_items, &mut emit);
    flush_issues(&scan_id, &mut pending_issues, &mut emit);
    emit(ScanEvent::Progress {
        scan_id: scan_id.clone(),
        visited,
        accepted,
        current_path: last_path,
    });
    emit(ScanEvent::Finished {
        scan_id,
        accepted,
        issue_count,
        cancelled: cancelled.load(Ordering::Relaxed),
        limit_reached,
    });
    Ok(())
}

fn prepare_roots(paths: &[String], issues: &mut Vec<AppError>) -> Vec<ScanRoot> {
    let mut directories = Vec::new();
    let mut files = Vec::new();

    for raw_path in paths {
        let requested = PathBuf::from(raw_path);
        let metadata = match fs::symlink_metadata(&requested) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                issues.push(AppError::new(ErrorCode::NotFound).path(&requested));
                continue;
            }
            Err(error) => {
                issues.push(AppError::io(error, &requested));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            issues.push(AppError::new(ErrorCode::Symlink).path(&requested));
            continue;
        }
        let path = match normalize_existing_path(&requested) {
            Ok(path) => path,
            Err(error) => {
                issues.push(
                    AppError::new(ErrorCode::IoFailed)
                        .path(&requested)
                        .detail(error)
                        .retryable(true),
                );
                continue;
            }
        };
        if metadata.is_dir() {
            directories.push(path);
        } else if metadata.is_file() {
            files.push(path);
        }
    }

    directories.sort_by_key(|path| normalize_display_path(path).to_lowercase());
    directories.dedup_by(|left, right| same_path(left, right));
    let mut outer_directories: Vec<PathBuf> = Vec::new();
    for directory in directories {
        if !outer_directories
            .iter()
            .any(|outer| path_is_within(&directory, outer))
        {
            outer_directories.push(directory);
        }
    }

    files.sort_by_key(|path| normalize_display_path(path).to_lowercase());
    files.dedup_by(|left, right| same_path(left, right));
    let mut roots = outer_directories
        .iter()
        .cloned()
        .map(ScanRoot::Directory)
        .collect::<Vec<_>>();
    for path in files {
        if outer_directories
            .iter()
            .any(|directory| path_is_within(&path, directory))
        {
            continue;
        }
        let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        roots.push(ScanRoot::File { path, root });
    }
    roots.sort_by_key(|root| match root {
        ScanRoot::Directory(path) => normalize_display_path(path).to_lowercase(),
        ScanRoot::File { path, .. } => normalize_display_path(path).to_lowercase(),
    });
    roots
}

fn inspect_input(path: &Path, root: &Path, direct: bool) -> AppResult<Option<InputItem>> {
    let extension_format = format_from_extension(path);
    let Some(extension_format) = extension_format else {
        return if direct {
            Err(AppError::new(ErrorCode::UnsupportedExtension).path(path))
        } else {
            Ok(None)
        };
    };
    let metadata = fs::metadata(path).map_err(|error| AppError::io(error, path))?;
    limits::validate_file_size(path, metadata.len())?;
    let (magic_format, width, height) = inspect_file(path)
        .map_err(|error| AppError::operation(ErrorCode::InvalidImage, error, path))?;
    if extension_format != magic_format {
        return Err(AppError::new(ErrorCode::FormatMismatch).path(path));
    }
    limits::validate_dimensions(path, width, height)?;

    let canonical = normalize_existing_path(path).map_err(|error| {
        AppError::new(ErrorCode::IoFailed)
            .path(path)
            .detail(error)
            .retryable(true)
    })?;
    let normalized_root = normalize_existing_path(root).unwrap_or_else(|_| root.to_path_buf());
    let relative = canonical
        .strip_prefix(&normalized_root)
        .unwrap_or_else(|_| canonical.file_name().map(Path::new).unwrap_or(&canonical));
    let source_path = normalize_display_path(&canonical);
    let name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    Ok(Some(InputItem {
        id: stable_id(&source_path),
        source_path,
        input_root: normalize_display_path(&normalized_root),
        relative_path: normalize_display_path(relative),
        name,
        format: magic_format,
        width,
        height,
        original_size: metadata.len(),
        modified_ms: modified_ms(metadata.modified().ok()),
    }))
}

pub fn validate_runtime_item(item: &InputItem) -> AppResult<()> {
    let path = Path::new(&item.source_path);
    let metadata = fs::symlink_metadata(path).map_err(|error| AppError::io(error, path))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::new(ErrorCode::SourceChanged)
            .path(path)
            .detail("Source path is no longer a regular file")
            .retryable(true));
    }
    limits::validate_file_size(path, metadata.len())?;
    let extension_format = format_from_extension(path)
        .ok_or_else(|| AppError::new(ErrorCode::UnsupportedExtension).path(path))?;
    let (actual_format, width, height) = inspect_file(path)
        .map_err(|error| AppError::operation(ErrorCode::InvalidImage, error, path))?;
    if extension_format != actual_format || item.format != actual_format {
        return Err(AppError::new(ErrorCode::FormatMismatch).path(path));
    }
    limits::validate_dimensions(path, width, height)?;
    let current_modified = modified_ms(metadata.modified().ok());
    if metadata.len() != item.original_size
        || current_modified != item.modified_ms
        || width != item.width
        || height != item.height
    {
        return Err(AppError::new(ErrorCode::SourceChanged)
            .path(path)
            .param("actual_width", width)
            .param("actual_height", height)
            .retryable(true));
    }
    Ok(())
}

fn flush_if_needed(
    scan_id: &str,
    items: &mut Vec<InputItem>,
    issues: &mut Vec<AppError>,
    visited: usize,
    accepted: usize,
    current_path: &str,
    emit: &mut impl FnMut(ScanEvent),
) {
    if items.len() >= EVENT_CHUNK_SIZE {
        flush_items(scan_id, items, emit);
    }
    if issues.len() >= EVENT_CHUNK_SIZE {
        flush_issues(scan_id, issues, emit);
    }
    if visited.is_multiple_of(EVENT_CHUNK_SIZE) {
        emit(ScanEvent::Progress {
            scan_id: scan_id.into(),
            visited,
            accepted,
            current_path: current_path.into(),
        });
    }
}

fn flush_items(scan_id: &str, items: &mut Vec<InputItem>, emit: &mut impl FnMut(ScanEvent)) {
    if items.is_empty() {
        return;
    }
    emit(ScanEvent::Items {
        scan_id: scan_id.into(),
        items: std::mem::take(items),
    });
}

fn flush_issues(scan_id: &str, issues: &mut Vec<AppError>, emit: &mut impl FnMut(ScanEvent)) {
    if issues.is_empty() {
        return;
    }
    emit(ScanEvent::Issues {
        scan_id: scan_id.into(),
        issues: std::mem::take(issues),
    });
}

fn include_entry(entry: &DirEntry, output_root: &Path) -> bool {
    !entry.file_type().is_symlink() && !same_path(entry.path(), output_root)
}

fn inspect_file(path: &Path) -> Result<(ImageFormat, u32, u32)> {
    let mut header = Vec::new();
    File::open(path)
        .with_context(|| format!("Failed to open {}", path.display()))?
        .take(HEADER_LIMIT)
        .read_to_end(&mut header)?;
    let format =
        format_from_magic(&header).ok_or_else(|| anyhow!("Unsupported image signature"))?;

    match format {
        ImageFormat::Png if animated_png(&header) => {
            return Err(anyhow!("Animated PNG is not supported"));
        }
        ImageFormat::Jpeg => match jpeg_components(&header) {
            Some(1 | 3) => {}
            Some(4) => return Err(anyhow!("CMYK JPEG is not supported")),
            Some(_) => return Err(anyhow!("Unsupported JPEG component layout")),
            None => {}
        },
        ImageFormat::Webp if animated_webp(&header) => {
            return Err(anyhow!("Animated WebP is not supported"));
        }
        _ => {}
    }

    let (width, height) = if format == ImageFormat::Webp {
        webp_dimensions(&header)?
    } else {
        ImageReader::open(path)?
            .with_guessed_format()?
            .into_dimensions()?
    };
    Ok((format, width, height))
}

fn webp_dimensions(data: &[u8]) -> Result<(u32, u32)> {
    if format_from_magic(data) != Some(ImageFormat::Webp) {
        return Err(anyhow!("Invalid WebP header"));
    }
    let mut offset = 12usize;
    while offset.saturating_add(8) <= data.len() {
        let kind = &data[offset..offset + 4];
        let length = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let content = offset + 8;
        let end = content
            .checked_add(length)
            .ok_or_else(|| anyhow!("Invalid WebP chunk size"))?;
        if end > data.len() {
            return Err(anyhow!("Truncated WebP header"));
        }

        let dimensions = match kind {
            b"VP8X" if length >= 10 => {
                let width = u32::from_le_bytes([
                    data[content + 4],
                    data[content + 5],
                    data[content + 6],
                    0,
                ]) + 1;
                let height = u32::from_le_bytes([
                    data[content + 7],
                    data[content + 8],
                    data[content + 9],
                    0,
                ]) + 1;
                Some((width, height))
            }
            b"VP8L" if length >= 5 && data[content] == 0x2f => {
                let size = u32::from_le_bytes(data[content + 1..content + 5].try_into().unwrap());
                Some(((size & 0x3fff) + 1, ((size >> 14) & 0x3fff) + 1))
            }
            b"VP8 " if length >= 10 && data[content + 3..content + 6] == [0x9d, 0x01, 0x2a] => {
                let width = u16::from_le_bytes([data[content + 6], data[content + 7]]) & 0x3fff;
                let height = u16::from_le_bytes([data[content + 8], data[content + 9]]) & 0x3fff;
                Some((u32::from(width), u32::from(height)))
            }
            _ => None,
        };
        if let Some((width, height)) = dimensions {
            if width == 0 || height == 0 {
                return Err(anyhow!("WebP dimensions are invalid"));
            }
            return Ok((width, height));
        }
        offset = end
            .checked_add(length & 1)
            .ok_or_else(|| anyhow!("Invalid WebP padding"))?;
    }
    Err(anyhow!("WebP dimensions could not be read"))
}

pub fn format_from_extension(path: &Path) -> Option<ImageFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "webp" => Some(ImageFormat::Webp),
        _ => None,
    }
}

pub fn format_from_magic(data: &[u8]) -> Option<ImageFormat> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(ImageFormat::Jpeg)
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some(ImageFormat::Webp)
    } else {
        None
    }
}

fn animated_png(data: &[u8]) -> bool {
    if data.len() < 20 {
        return false;
    }
    let mut offset = 8usize;
    while offset.saturating_add(12) <= data.len() {
        let length = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = &data[offset + 4..offset + 8];
        if kind == b"acTL" {
            return true;
        }
        if kind == b"IDAT" {
            return false;
        }
        offset = match offset.checked_add(length + 12) {
            Some(next) => next,
            None => return false,
        };
    }
    false
}

fn animated_webp(data: &[u8]) -> bool {
    data.len() >= 21 && &data[12..16] == b"VP8X" && data[20] & 0x02 != 0
}

fn jpeg_components(data: &[u8]) -> Option<u8> {
    if data.len() < 12 || !data.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut offset = 2usize;
    while offset + 9 < data.len() {
        while offset < data.len() && data[offset] != 0xff {
            offset += 1;
        }
        while offset < data.len() && data[offset] == 0xff {
            offset += 1;
        }
        if offset >= data.len() {
            return None;
        }
        let marker = data[offset];
        offset += 1;
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        if (0xd0..=0xd7).contains(&marker) || marker == 0x01 {
            continue;
        }
        if offset + 2 > data.len() {
            return None;
        }
        let length = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        if length < 2 || offset + length > data.len() {
            return None;
        }
        let is_sof = matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        );
        if is_sof && length >= 8 {
            return data.get(offset + 7).copied();
        }
        offset += length;
    }
    None
}

pub fn validate_output_subfolder(name: &str) -> AppResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." || trimmed != name {
        return Err(AppError::new(ErrorCode::InvalidOutputFolder));
    }
    if trimmed.ends_with('.')
        || trimmed.encode_utf16().count() > 255
        || trimmed.contains(['/', '\\'])
        || trimmed
            .chars()
            .any(|character| character.is_control() || "<>:\"|?*".contains(character))
    {
        return Err(AppError::new(ErrorCode::InvalidOutputFolder));
    }
    let stem = trimmed
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved.contains(&stem.as_str()) {
        return Err(AppError::new(ErrorCode::InvalidOutputFolder));
    }
    Ok(())
}

fn stable_id(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_lowercase().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
    dunce::canonicalize(path).with_context(|| format!("Failed to resolve {}", path.display()))
}

pub fn normalize_display_path(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize_display_path(left).eq_ignore_ascii_case(&normalize_display_path(right))
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    let path = normalize_display_path(path).to_lowercase();
    let mut root = normalize_display_path(root).to_lowercase();
    if !root.ends_with('\\') {
        root.push('\\');
    }
    path == root.trim_end_matches('\\') || path.starts_with(&root)
}

pub fn modified_ms(time: Option<SystemTime>) -> u64 {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn collect_scan(paths: Vec<String>) -> (Vec<InputItem>, Vec<AppError>, ScanEvent) {
        collect_request(ScanRequest {
            scan_id: "test".into(),
            paths,
            output_subfolder: "compressed".into(),
            existing_ids: Vec::new(),
            remaining_capacity: limits::MAX_QUEUE_ITEMS,
        })
    }

    fn collect_request(request: ScanRequest) -> (Vec<InputItem>, Vec<AppError>, ScanEvent) {
        let mut items = Vec::new();
        let mut issues = Vec::new();
        let mut finished = None;
        scan_stream(
            request,
            Arc::new(AtomicBool::new(false)),
            |event| match event {
                ScanEvent::Items { items: chunk, .. } => items.extend(chunk),
                ScanEvent::Issues { issues: chunk, .. } => issues.extend(chunk),
                ScanEvent::Finished { .. } => finished = Some(event),
                ScanEvent::Progress { .. } => {}
            },
        )
        .unwrap();
        (items, issues, finished.unwrap())
    }

    #[test]
    fn detects_supported_signatures() {
        assert_eq!(
            format_from_magic(b"\x89PNG\r\n\x1a\nrest"),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            format_from_magic(&[0xff, 0xd8, 0xff, 0xe0]),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(format_from_magic(b"GIF89a"), None);
    }

    #[test]
    fn validates_windows_folder_names() {
        assert!(validate_output_subfolder("compressed").is_ok());
        assert!(validate_output_subfolder("CON").is_err());
        assert!(validate_output_subfolder("results.").is_err());
        assert!(validate_output_subfolder("../out").is_err());
    }

    #[test]
    fn overlapping_inputs_always_use_the_outermost_root() {
        let temporary = tempdir().unwrap();
        let root = temporary.path().join("图像集");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let image = nested.join("a.png");
        write_test_png(&image);

        let forward = collect_scan(vec![
            normalize_display_path(&root),
            normalize_display_path(&image),
        ]);
        let reverse = collect_scan(vec![
            normalize_display_path(&image),
            normalize_display_path(&root),
        ]);
        assert_eq!(forward.0.len(), 1);
        assert_eq!(reverse.0.len(), 1);
        assert_eq!(forward.0[0].input_root, reverse.0[0].input_root);
        assert!(forward.0[0].relative_path.ends_with("nested\\a.png"));
        assert!(forward.1.is_empty());
    }

    #[test]
    fn cancellation_finishes_without_processing_files() {
        let temporary = tempdir().unwrap();
        let image = temporary.path().join("a.png");
        write_test_png(&image);
        let cancelled = Arc::new(AtomicBool::new(true));
        let request = ScanRequest {
            scan_id: "cancel".into(),
            paths: vec![normalize_display_path(temporary.path())],
            output_subfolder: "compressed".into(),
            existing_ids: Vec::new(),
            remaining_capacity: 10,
        };
        let mut finished_cancelled = false;
        scan_stream(request, cancelled, |event| {
            if let ScanEvent::Finished { cancelled, .. } = event {
                finished_cancelled = cancelled;
            }
        })
        .unwrap();
        assert!(finished_cancelled);
    }

    #[test]
    fn sorts_paths_and_emits_item_batches_of_at_most_one_hundred() {
        let temporary = tempdir().unwrap();
        for index in (0..205).rev() {
            write_test_png(&temporary.path().join(format!("图片-{index:03}.png")));
        }
        let request = ScanRequest {
            scan_id: "chunks".into(),
            paths: vec![normalize_display_path(temporary.path())],
            output_subfolder: "compressed".into(),
            existing_ids: Vec::new(),
            remaining_capacity: limits::MAX_QUEUE_ITEMS,
        };
        let mut names = Vec::new();
        let mut chunk_sizes = Vec::new();
        scan_stream(request, Arc::new(AtomicBool::new(false)), |event| {
            if let ScanEvent::Items { items, .. } = event {
                chunk_sizes.push(items.len());
                names.extend(items.into_iter().map(|item| item.name));
            }
        })
        .unwrap();

        assert_eq!(names.len(), 205);
        assert!(chunk_sizes.iter().all(|size| *size <= EVENT_CHUNK_SIZE));
        assert!(names.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn existing_ids_do_not_consume_remaining_queue_capacity() {
        let temporary = tempdir().unwrap();
        let first_path = temporary.path().join("a.png");
        let second_path = temporary.path().join("b.png");
        write_test_png(&first_path);
        write_test_png(&second_path);
        let existing = collect_scan(vec![normalize_display_path(&first_path)]).0[0]
            .id
            .clone();
        let (items, issues, finished) = collect_request(ScanRequest {
            scan_id: "capacity".into(),
            paths: vec![normalize_display_path(temporary.path())],
            output_subfolder: "compressed".into(),
            existing_ids: vec![existing],
            remaining_capacity: 1,
        });

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "a.png");
        assert_eq!(items[1].name, "b.png");
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == ErrorCode::QueueLimitReached)
        );
        assert!(matches!(
            finished,
            ScanEvent::Finished {
                accepted: 1,
                limit_reached: true,
                ..
            }
        ));
    }

    #[test]
    fn existing_id_is_reemitted_with_a_more_complete_outer_mapping() {
        let temporary = tempdir().unwrap();
        let nested = temporary.path().join("nested");
        fs::create_dir_all(&nested).unwrap();
        let image = nested.join("a.png");
        write_test_png(&image);
        let original = collect_scan(vec![normalize_display_path(&image)])
            .0
            .remove(0);
        assert_eq!(original.relative_path, "a.png");

        let (items, _, finished) = collect_request(ScanRequest {
            scan_id: "remap".into(),
            paths: vec![normalize_display_path(temporary.path())],
            output_subfolder: "compressed".into(),
            existing_ids: vec![original.id],
            remaining_capacity: 10,
        });

        assert_eq!(items.len(), 1);
        assert!(items[0].relative_path.ends_with("nested\\a.png"));
        assert!(matches!(finished, ScanEvent::Finished { accepted: 0, .. }));
    }

    #[test]
    fn runtime_validation_rejects_forged_dimensions() {
        let temporary = tempdir().unwrap();
        let image = temporary.path().join("a.png");
        write_test_png(&image);
        let mut item = collect_scan(vec![normalize_display_path(&image)])
            .0
            .remove(0);
        item.width = 1;

        assert_eq!(
            validate_runtime_item(&item).unwrap_err().code,
            ErrorCode::SourceChanged
        );
    }

    fn write_test_png(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255; 16]).unwrap();
    }
}
