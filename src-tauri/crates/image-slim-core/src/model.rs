use crate::error::AppError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use ts_rs::TS;

#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd, Serialize, TS,
)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

impl ImageFormat {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Webp => "WebP",
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CompressionPreset {
    Lossless,
    Balanced,
    Strong,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    Subfolder,
    Overwrite,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MetadataPolicy {
    Essential,
    Supported,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Ready,
    Processing,
    Completed,
    Unchanged,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BatchStartStatus {
    Conflicts,
    Started,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
pub struct InputItem {
    pub id: String,
    pub source_path: String,
    pub input_root: String,
    pub relative_path: String,
    pub name: String,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    #[ts(type = "number")]
    pub original_size: u64,
    #[ts(type = "number")]
    pub modified_ms: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
pub struct ScanRequest {
    pub scan_id: String,
    pub paths: Vec<String>,
    pub output_subfolder: String,
    pub existing_ids: Vec<String>,
    pub remaining_capacity: usize,
}

#[derive(Clone, Debug, JsonSchema, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScanEvent {
    Items {
        scan_id: String,
        items: Vec<InputItem>,
    },
    Issues {
        scan_id: String,
        issues: Vec<AppError>,
    },
    Progress {
        scan_id: String,
        visited: usize,
        accepted: usize,
        current_path: String,
    },
    Finished {
        scan_id: String,
        accepted: usize,
        issue_count: usize,
        cancelled: bool,
        limit_reached: bool,
    },
}

#[derive(Clone, Debug, Deserialize, JsonSchema, TS)]
pub struct PreviewRequest {
    pub request_id: String,
    pub item: InputItem,
    pub preset: CompressionPreset,
    pub metadata_policy: MetadataPolicy,
}

#[derive(Clone, Debug, JsonSchema, Serialize, TS)]
pub struct PreviewResult {
    pub source_preview_path: String,
    pub candidate_preview_path: String,
    #[ts(type = "number")]
    pub source_size: u64,
    #[ts(type = "number")]
    pub candidate_size: u64,
    pub would_replace: bool,
    pub cache_key: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, TS)]
pub struct BatchRequest {
    pub items: Vec<InputItem>,
    pub preset: CompressionPreset,
    pub output_mode: OutputMode,
    pub output_subfolder: String,
    pub metadata_policy: MetadataPolicy,
    #[serde(default)]
    pub allow_conflicts: bool,
}

#[derive(Clone, Debug, JsonSchema, Serialize, TS)]
pub struct BatchStartResult {
    pub status: BatchStartStatus,
    pub batch_id: Option<String>,
    pub conflict_count: usize,
}

#[derive(Clone, Debug, JsonSchema, Serialize, TS)]
pub struct ItemProgress {
    pub batch_id: String,
    pub item_id: String,
    pub status: TaskStatus,
    pub output_path: Option<String>,
    #[ts(type = "number | null")]
    pub output_size: Option<u64>,
    #[ts(type = "number")]
    pub saved_bytes: u64,
    pub error: Option<AppError>,
}

#[derive(Clone, Debug, JsonSchema, Serialize, TS)]
pub struct BatchSummary {
    pub batch_id: String,
    pub completed: usize,
    pub unchanged: usize,
    pub failed: usize,
    pub cancelled: usize,
    #[ts(type = "number")]
    pub original_bytes: u64,
    #[ts(type = "number")]
    pub output_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq)]
pub struct SourceFingerprint {
    pub size: u64,
    pub modified_ms: u64,
}

#[derive(Clone, Debug, JsonSchema, Serialize, TS)]
pub struct FormatCapability {
    pub format: ImageFormat,
    pub extensions: Vec<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize, TS)]
pub struct InputLimits {
    #[ts(type = "number")]
    pub max_file_bytes: u64,
    #[ts(type = "number")]
    pub max_pixels: u64,
    pub max_dimension: u32,
    pub max_queue_items: usize,
}

#[derive(Clone, Debug, JsonSchema, Serialize, TS)]
pub struct AppCapabilities {
    pub formats: Vec<FormatCapability>,
    pub presets: Vec<CompressionPreset>,
    pub limits: InputLimits,
}
