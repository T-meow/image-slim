use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    UnsupportedExtension,
    FileTooLarge,
    PixelLimitExceeded,
    DimensionLimitExceeded,
    QueueLimitReached,
    InvalidImage,
    FormatMismatch,
    Symlink,
    WalkError,
    InvalidOutputFolder,
    SourceChanged,
    OutputConflict,
    PermissionDenied,
    CodecFailed,
    Cancelled,
    InsufficientMemory,
    BatchRunning,
    PreviewPaused,
    IoFailed,
    Internal,
}

impl ErrorCode {
    pub const ALL: &'static [Self] = &[
        Self::NotFound,
        Self::UnsupportedExtension,
        Self::FileTooLarge,
        Self::PixelLimitExceeded,
        Self::DimensionLimitExceeded,
        Self::QueueLimitReached,
        Self::InvalidImage,
        Self::FormatMismatch,
        Self::Symlink,
        Self::WalkError,
        Self::InvalidOutputFolder,
        Self::SourceChanged,
        Self::OutputConflict,
        Self::PermissionDenied,
        Self::CodecFailed,
        Self::Cancelled,
        Self::InsufficientMemory,
        Self::BatchRunning,
        Self::PreviewPaused,
        Self::IoFailed,
        Self::Internal,
    ];
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub struct AppError {
    pub code: ErrorCode,
    pub params: BTreeMap<String, String>,
    pub path: Option<String>,
    pub detail: Option<String>,
    pub retryable: bool,
}

impl AppError {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            params: BTreeMap::new(),
            path: None,
            detail: None,
            retryable: false,
        }
    }

    pub fn param(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.params.insert(key.into(), value.to_string());
        self
    }

    pub fn path(mut self, path: impl AsRef<Path>) -> Self {
        self.path = Some(path.as_ref().to_string_lossy().replace('/', "\\"));
        self
    }

    pub fn detail(mut self, detail: impl ToString) -> Self {
        self.detail = Some(detail.to_string());
        self
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    pub fn io(error: std::io::Error, path: impl AsRef<Path>) -> Self {
        let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
            ErrorCode::PermissionDenied
        } else {
            ErrorCode::IoFailed
        };
        Self::new(code).path(path).detail(error).retryable(true)
    }

    pub fn internal(error: impl fmt::Display) -> Self {
        Self::new(ErrorCode::Internal).detail(error)
    }

    pub fn operation(code: ErrorCode, error: anyhow::Error, path: impl AsRef<Path>) -> Self {
        let permission_denied = error.chain().any(|cause| {
            cause
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
        });
        Self::new(if permission_denied {
            ErrorCode::PermissionDenied
        } else {
            code
        })
        .path(path)
        .detail(error)
        .retryable(permission_denied)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(detail) = &self.detail {
            formatter.write_str(detail)
        } else {
            write!(formatter, "{:?}", self.code)
        }
    }
}

impl std::error::Error for AppError {}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_permission_errors_through_context_chains() {
        let error = anyhow::Error::from(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ))
        .context("could not inspect image");
        let mapped = AppError::operation(ErrorCode::InvalidImage, error, "image.png");
        assert_eq!(mapped.code, ErrorCode::PermissionDenied);
        assert!(mapped.retryable);
        assert!(mapped.detail.unwrap().contains("could not inspect image"));
    }
}
