use crate::model::{InputItem, OutputMode, SourceFingerprint};
use crate::scanner::{modified_ms, normalize_display_path, validate_output_subfolder};
use anyhow::{Context, Result, anyhow};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

const STALE_TEMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);

pub fn output_path(item: &InputItem, mode: OutputMode, subfolder: &str) -> Result<PathBuf> {
    let source = PathBuf::from(&item.source_path);
    if mode == OutputMode::Overwrite {
        return Ok(source);
    }
    validate_output_subfolder(subfolder)?;
    let relative = Path::new(&item.relative_path);
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(anyhow!("Input relative path is unsafe"));
    }
    let root = PathBuf::from(&item.input_root);
    Ok(root.join(subfolder).join(relative))
}

pub fn validate_item_mapping(item: &InputItem) -> Result<()> {
    let source = Path::new(&item.source_path);
    let source_metadata = fs::symlink_metadata(source)
        .with_context(|| format!("Failed to inspect {}", source.display()))?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(anyhow!("Source path is no longer a regular file"));
    }

    let root = dunce::canonicalize(&item.input_root)
        .with_context(|| format!("Failed to resolve input root {}", item.input_root))?;
    let canonical_source = dunce::canonicalize(source)
        .with_context(|| format!("Failed to resolve source {}", source.display()))?;
    let relative = Path::new(&item.relative_path);
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(anyhow!("Input relative path is unsafe"));
    }
    let expected = dunce::canonicalize(root.join(relative))?;
    if !same_path(&canonical_source, &expected) || !same_path(&canonical_source, source) {
        return Err(anyhow!(
            "Input root and relative path do not match the source file"
        ));
    }
    Ok(())
}

pub fn validate_output_target(
    item: &InputItem,
    mode: OutputMode,
    subfolder: &str,
    target: &Path,
) -> Result<()> {
    if mode == OutputMode::Overwrite {
        return Ok(());
    }
    let root = dunce::canonicalize(&item.input_root)?;
    let output_root = dunce::canonicalize(root.join(subfolder))?;
    let target_parent = target
        .parent()
        .ok_or_else(|| anyhow!("Output path has no parent directory"))?;
    let target_parent = dunce::canonicalize(target_parent)?;
    if !path_starts_with(&output_root, &root) || !path_starts_with(&target_parent, &output_root) {
        return Err(anyhow!("Output path escaped the selected input root"));
    }
    Ok(())
}

pub fn fingerprint(path: &Path) -> Result<SourceFingerprint> {
    let metadata =
        fs::metadata(path).with_context(|| format!("Failed to inspect {}", path.display()))?;
    Ok(SourceFingerprint {
        size: metadata.len(),
        modified_ms: modified_ms(metadata.modified().ok()),
    })
}

pub fn assert_unchanged(item: &InputItem) -> Result<()> {
    validate_item_mapping(item)?;
    let current = fingerprint(Path::new(&item.source_path))?;
    if current.size != item.original_size || current.modified_ms != item.modified_ms {
        return Err(anyhow!("Source file changed after it was added"));
    }
    Ok(())
}

pub fn content_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

pub fn file_content_hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn assert_content_unchanged(item: &InputItem, expected_hash: &str) -> Result<()> {
    assert_unchanged(item)?;
    if file_content_hash(Path::new(&item.source_path))? != expected_hash {
        return Err(anyhow!("Source file changed while it was being processed"));
    }
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize_display_path(left).eq_ignore_ascii_case(&normalize_display_path(right))
}

fn path_starts_with(path: &Path, root: &Path) -> bool {
    let path = normalize_display_path(path).to_lowercase();
    let mut root = normalize_display_path(root).to_lowercase();
    if !root.ends_with('\\') {
        root.push('\\');
    }
    path == root.trim_end_matches('\\') || path.starts_with(&root)
}

pub fn atomic_write_guarded(
    target: &Path,
    bytes: &[u8],
    before_write: impl Fn() -> Result<()>,
    before_replace: impl Fn() -> Result<()>,
) -> Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("Output path has no parent directory"))?;
    fs::create_dir_all(parent)?;
    before_write()?;
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    let temp = parent.join(format!(".{name}.image-slim-{}.tmp", Uuid::new_v4()));

    let write_result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        before_replace()?;
        replace_path(&temp, target)
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result
}

fn replace_path(temp: &Path, target: &Path) -> Result<()> {
    if !target.exists() {
        return fs::rename(temp, target).map_err(Into::into);
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };

        let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if result == 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| {
                format!(
                    "Failed to atomically replace {} with {}",
                    normalize_display_path(target),
                    normalize_display_path(temp)
                )
            });
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        fs::rename(temp, target)?;
        Ok(())
    }
}

pub fn cleanup_stale_temporary_files(root: &Path) -> Result<usize> {
    if !root.is_dir() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let old_enough = entry
            .metadata()?
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= STALE_TEMP_AGE);
        if old_enough && name.contains(".image-slim-") && name.ends_with(".tmp") {
            fs::remove_file(path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ImageFormat;
    use tempfile::tempdir;

    fn item(root: &Path) -> InputItem {
        InputItem {
            id: "1".into(),
            source_path: normalize_display_path(&root.join("nested").join("a.png")),
            input_root: normalize_display_path(root),
            relative_path: "nested\\a.png".into(),
            name: "a.png".into(),
            format: ImageFormat::Png,
            width: 1,
            height: 1,
            original_size: 0,
            modified_ms: 0,
        }
    }

    #[test]
    fn maps_subfolder_and_overwrite_paths() {
        let root = PathBuf::from("C:\\images");
        let item = item(&root);
        assert_eq!(
            output_path(&item, OutputMode::Subfolder, "compressed").unwrap(),
            root.join("compressed").join("nested\\a.png")
        );
        assert_eq!(
            output_path(&item, OutputMode::Overwrite, "compressed").unwrap(),
            PathBuf::from(&item.source_path)
        );
    }

    #[test]
    fn rejects_relative_path_traversal() {
        let root = PathBuf::from("C:\\images");
        let mut item = item(&root);
        item.relative_path = "..\\outside.png".into();
        assert!(output_path(&item, OutputMode::Subfolder, "compressed").is_err());
    }

    #[test]
    fn writes_and_replaces_atomically() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("image.bin");
        atomic_write_guarded(&target, b"first", || Ok(()), || Ok(())).unwrap();
        atomic_write_guarded(&target, b"second", || Ok(()), || Ok(())).unwrap();
        assert_eq!(fs::read(target).unwrap(), b"second");
    }

    #[test]
    fn guard_failure_keeps_existing_target_and_cleans_temp_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("image.bin");
        fs::write(&target, b"original").unwrap();
        let result = atomic_write_guarded(
            &target,
            b"replacement",
            || Ok(()),
            || Err(anyhow!("cancelled")),
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn detects_source_changes_after_scan() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("nested").join("a.png");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"before").unwrap();
        let metadata = fs::metadata(&source).unwrap();
        let mut item = item(dir.path());
        item.original_size = metadata.len();
        item.modified_ms = modified_ms(metadata.modified().ok());
        fs::write(&source, b"after-change").unwrap();
        assert!(assert_unchanged(&item).is_err());
    }

    #[test]
    fn detects_same_size_content_changes() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("nested").join("a.png");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"before").unwrap();
        let metadata = fs::metadata(&source).unwrap();
        let original_modified = metadata.modified().unwrap();
        let mut item = item(dir.path());
        item.original_size = metadata.len();
        item.modified_ms = modified_ms(metadata.modified().ok());
        let expected_hash = file_content_hash(&source).unwrap();
        fs::write(&source, b"after!").unwrap();
        let file = OpenOptions::new().write(true).open(&source).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(original_modified))
            .unwrap();
        assert!(assert_unchanged(&item).is_ok());
        assert!(assert_content_unchanged(&item, &expected_hash).is_err());
    }
}
