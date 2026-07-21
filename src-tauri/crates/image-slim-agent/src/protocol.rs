use image_slim_core::error::{AppError, ErrorCode};
use image_slim_core::model::{
    AppCapabilities, CompressionPreset, ImageFormat, MetadataPolicy, OutputMode,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const AGENT_PROTOCOL_VERSION: u32 = 1;
pub const DEFAULT_OUTPUT_SUBFOLDER: &str = "compressed";
pub const DEFAULT_ISSUE_LIMIT: usize = 10;
pub const MAX_ISSUE_LIMIT: usize = 50;
pub const DEFAULT_WAIT_MS: u64 = 1_000;
pub const MAX_WAIT_MS: u64 = 5_000;

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
pub struct Envelope<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AppError>,
}

impl<T> Envelope<T> {
    pub fn success(result: T) -> Self {
        Self {
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(error: AppError) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(error),
        }
    }

    pub fn is_success(&self) -> bool {
        self.ok
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub request_id: String,
    pub paths: Vec<String>,
    #[serde(default = "default_output_subfolder")]
    pub output_subfolder: String,
    #[serde(default = "default_issue_limit")]
    pub issue_limit: usize,
    #[serde(default)]
    pub include_technical_detail: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompressRequest {
    pub request_id: String,
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub paths: Option<Vec<String>>,
    #[serde(default = "default_preset")]
    pub preset: CompressionPreset,
    #[serde(default = "default_output_mode")]
    pub output_mode: OutputMode,
    #[serde(default = "default_output_subfolder")]
    pub output_subfolder: String,
    #[serde(default = "default_metadata_policy")]
    pub metadata_policy: MetadataPolicy,
    #[serde(default)]
    pub allow_conflicts: bool,
    #[serde(default = "default_wait_ms")]
    pub wait_ms: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    pub job_id: String,
    #[serde(default)]
    pub issue_cursor: usize,
    #[serde(default = "default_issue_limit")]
    pub issue_limit: usize,
    #[serde(default)]
    pub include_technical_detail: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancelRequest {
    pub job_id: String,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct AgentCapabilities {
    pub agent_protocol_version: u32,
    pub app_version: String,
    pub core: AppCapabilities,
    pub metadata_policies: Vec<MetadataPolicy>,
    pub allowed_roots: Vec<String>,
    pub allow_overwrite: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
pub struct PlanResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
    pub visited: usize,
    pub accepted: usize,
    pub input_bytes: u64,
    pub format_counts: BTreeMap<ImageFormat, usize>,
    pub issue_count: usize,
    pub issue_code_counts: BTreeMap<ErrorCode, usize>,
    pub issues: Vec<AppError>,
    pub next_issue_cursor: Option<usize>,
    pub limit_reached: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
pub struct JobStatus {
    pub job_id: String,
    pub state: JobState,
    pub total: usize,
    pub completed: usize,
    pub unchanged: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub original_bytes: u64,
    pub output_bytes: u64,
    pub saved_bytes: u64,
    pub issue_count: usize,
    pub issue_code_counts: BTreeMap<ErrorCode, usize>,
    pub issues: Vec<AppError>,
    pub next_issue_cursor: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
pub struct CancelResult {
    pub job_id: String,
    pub accepted: bool,
    pub state: JobState,
}

pub fn default_output_subfolder() -> String {
    DEFAULT_OUTPUT_SUBFOLDER.into()
}

pub const fn default_issue_limit() -> usize {
    DEFAULT_ISSUE_LIMIT
}

pub const fn default_wait_ms() -> u64 {
    DEFAULT_WAIT_MS
}

const fn default_preset() -> CompressionPreset {
    CompressionPreset::Balanced
}

const fn default_output_mode() -> OutputMode {
    OutputMode::Subfolder
}

const fn default_metadata_policy() -> MetadataPolicy {
    MetadataPolicy::Essential
}
