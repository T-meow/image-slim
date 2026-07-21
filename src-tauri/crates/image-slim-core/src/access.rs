use crate::error::{AppError, AppResult, ErrorCode};
use crate::model::{InputItem, OutputMode};
use crate::scanner::normalize_display_path;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AccessPolicy {
    roots: Vec<PathBuf>,
    allow_overwrite: bool,
}

impl AccessPolicy {
    pub fn new(roots: impl IntoIterator<Item = PathBuf>, allow_overwrite: bool) -> AppResult<Self> {
        let mut normalized: Vec<PathBuf> = Vec::new();
        for root in roots {
            if !root.is_absolute() {
                return Err(AppError::new(ErrorCode::InvalidRequest)
                    .path(&root)
                    .detail("Allowed roots must be absolute directories"));
            }
            let metadata =
                std::fs::symlink_metadata(&root).map_err(|error| AppError::io(error, &root))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(AppError::new(ErrorCode::RootNotAllowed)
                    .path(&root)
                    .detail("Allowed roots must be regular directories"));
            }
            let canonical = dunce::canonicalize(&root).map_err(|error| {
                AppError::new(ErrorCode::RootNotAllowed)
                    .path(&root)
                    .detail(error)
            })?;
            if !normalized.iter().any(|item| same_path(item, &canonical)) {
                normalized.push(canonical);
            }
        }
        normalized.sort_by_key(|path| normalize_display_path(path).to_lowercase());
        Ok(Self {
            roots: normalized,
            allow_overwrite,
        })
    }

    pub fn roots(&self) -> Vec<String> {
        self.roots
            .iter()
            .map(|path| normalize_display_path(path))
            .collect()
    }

    pub const fn allow_overwrite(&self) -> bool {
        self.allow_overwrite
    }

    pub fn ensure_paths(&self, paths: &[String]) -> AppResult<()> {
        if self.roots.is_empty() {
            return Err(AppError::new(ErrorCode::RootNotAllowed));
        }
        for raw in paths {
            let path = Path::new(raw);
            if !path.is_absolute() {
                return Err(AppError::new(ErrorCode::InvalidRequest)
                    .path(path)
                    .detail("Input paths must be absolute"));
            }
            let canonical = match std::fs::symlink_metadata(path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() {
                        return Err(AppError::new(ErrorCode::Symlink).path(path));
                    }
                    dunce::canonicalize(path).map_err(|error| AppError::io(error, path))?
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    canonicalize_nearest(path)?
                }
                Err(error) => return Err(AppError::io(error, path)),
            };
            self.ensure_canonical_path(&canonical)?;
        }
        Ok(())
    }

    pub fn ensure_item(&self, item: &InputItem) -> AppResult<()> {
        let canonical = dunce::canonicalize(&item.source_path)
            .map_err(|error| AppError::io(error, &item.source_path))?;
        self.ensure_canonical_path(&canonical)
    }

    pub fn ensure_output_mode(&self, mode: OutputMode) -> AppResult<()> {
        if mode == OutputMode::Overwrite && !self.allow_overwrite {
            Err(AppError::new(ErrorCode::OverwriteNotAllowed))
        } else {
            Ok(())
        }
    }

    fn ensure_canonical_path(&self, path: &Path) -> AppResult<()> {
        if self.roots.iter().any(|root| path_is_within(path, root)) {
            Ok(())
        } else {
            Err(AppError::new(ErrorCode::RootNotAllowed).path(path))
        }
    }
}

fn canonicalize_nearest(path: &Path) -> AppResult<PathBuf> {
    let mut existing = path;
    let mut suffix = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            return Err(AppError::new(ErrorCode::RootNotAllowed).path(path));
        };
        suffix.push(name.to_os_string());
        let Some(parent) = existing.parent() else {
            return Err(AppError::new(ErrorCode::RootNotAllowed).path(path));
        };
        existing = parent;
    }
    let mut canonical = dunce::canonicalize(existing).map_err(|error| AppError::io(error, path))?;
    for part in suffix.into_iter().rev() {
        canonical.push(part);
    }
    Ok(canonical)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn permits_missing_inputs_only_below_an_allowed_root() {
        let allowed = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let policy = AccessPolicy::new(vec![allowed.path().to_path_buf()], false).unwrap();
        assert!(
            policy
                .ensure_paths(&[allowed
                    .path()
                    .join("missing.png")
                    .to_string_lossy()
                    .into_owned()])
                .is_ok()
        );
        let error = policy
            .ensure_paths(&[outside
                .path()
                .join("missing.png")
                .to_string_lossy()
                .into_owned()])
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::RootNotAllowed);
    }

    #[test]
    fn overwrite_requires_process_level_permission() {
        let root = tempdir().unwrap();
        let denied = AccessPolicy::new(vec![root.path().to_path_buf()], false).unwrap();
        assert_eq!(
            denied
                .ensure_output_mode(OutputMode::Overwrite)
                .unwrap_err()
                .code,
            ErrorCode::OverwriteNotAllowed
        );
        let allowed = AccessPolicy::new(vec![root.path().to_path_buf()], true).unwrap();
        assert!(allowed.ensure_output_mode(OutputMode::Overwrite).is_ok());
    }
}
