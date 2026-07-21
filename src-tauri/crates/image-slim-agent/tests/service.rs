use filetime::FileTime;
use image_slim_agent::protocol::{CompressRequest, Envelope, JobState, PlanRequest, StatusRequest};
use image_slim_agent::service::{AgentService, Clock};
use image_slim_core::error::ErrorCode;
use image_slim_core::model::{CompressionPreset, MetadataPolicy, OutputMode};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::tempdir;
use uuid::Uuid;

#[derive(Default)]
struct ManualClock {
    now: AtomicU64,
}

impl ManualClock {
    fn advance(&self, milliseconds: u64) {
        self.now.fetch_add(milliseconds, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn monotonic_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }

    fn unix_ms(&self) -> u64 {
        1_700_000_000_000 + self.monotonic_ms()
    }
}

#[test]
fn plans_and_compresses_without_exposing_the_file_list() {
    let temporary = tempdir().unwrap();
    let source = temporary.path().join("source.png");
    write_png(&source, [10, 20, 30, 255]);
    let service = AgentService::new(vec![temporary.path().to_path_buf()], false).unwrap();
    let plan_request = PlanRequest {
        request_id: Uuid::new_v4().to_string(),
        paths: vec![source.to_string_lossy().into_owned()],
        output_subfolder: "compressed".into(),
        issue_limit: 10,
        include_technical_detail: false,
    };

    let first_plan = service.plan_mcp(plan_request.clone());
    let second_plan = service.plan_mcp(plan_request);
    let first = first_plan.result.expect("planning failed");
    let second = second_plan.result.expect("planning failed");
    assert_eq!(first.accepted, 1);
    assert_eq!(first.plan_id, second.plan_id);
    let serialized = serde_json::to_string(&first).unwrap();
    let plan_id = first.plan_id.unwrap();
    assert!(!serialized.contains("source.png"));

    let request = compress_request(Some(plan_id), None);
    let first = service.compress_and_wait(request.clone());
    let second = service.compress(request);
    let first = first.result.expect("compression failed");
    let second = second.result.expect("compression failed");
    assert_eq!(first.job_id, second.job_id);
    assert!(matches!(
        first.state,
        JobState::Completed | JobState::Cancelled
    ));
    let job_id = first.job_id;
    assert!(
        temporary
            .path()
            .join("compressed")
            .join("source.png")
            .is_file()
    );
    assert!(
        service
            .status(StatusRequest {
                job_id,
                issue_cursor: 0,
                issue_limit: 10,
                include_technical_detail: false,
            })
            .ok
    );
}

#[test]
fn rejects_reused_request_id_with_different_input() {
    let temporary = tempdir().unwrap();
    let source = temporary.path().join("source.png");
    write_png(&source, [10, 20, 30, 255]);
    let service = AgentService::new(vec![temporary.path().to_path_buf()], false).unwrap();
    let request_id = Uuid::new_v4().to_string();
    let first = PlanRequest {
        request_id: request_id.clone(),
        paths: vec![source.to_string_lossy().into_owned()],
        output_subfolder: "compressed".into(),
        issue_limit: 10,
        include_technical_detail: false,
    };
    assert!(service.plan_mcp(first.clone()).ok);
    let mut changed = first;
    changed.issue_limit = 11;
    assert_error(service.plan_mcp(changed), ErrorCode::InvalidRequest);
}

#[test]
fn expires_plans_with_an_injected_clock() {
    let temporary = tempdir().unwrap();
    let source = temporary.path().join("source.png");
    write_png(&source, [10, 20, 30, 255]);
    let clock = Arc::new(ManualClock::default());
    let service =
        AgentService::with_clock(vec![temporary.path().to_path_buf()], false, clock.clone())
            .unwrap();
    let plan_id = service
        .plan_mcp(PlanRequest {
            request_id: Uuid::new_v4().to_string(),
            paths: vec![source.to_string_lossy().into_owned()],
            output_subfolder: "compressed".into(),
            issue_limit: 10,
            include_technical_detail: false,
        })
        .result
        .expect("planning failed")
        .plan_id
        .unwrap();
    clock.advance(30 * 60 * 1_000 + 1);
    assert_error(
        service.compress(compress_request(Some(plan_id), None)),
        ErrorCode::PlanExpired,
    );
}

#[test]
fn detects_same_size_same_timestamp_changes_after_planning() {
    let temporary = tempdir().unwrap();
    let source = temporary.path().join("source.png");
    write_png(&source, [10, 20, 30, 255]);
    let original = fs::metadata(&source).unwrap();
    let modified = FileTime::from_last_modification_time(&original);
    let original_size = original.len();
    let service = AgentService::new(vec![temporary.path().to_path_buf()], false).unwrap();
    let plan_id = service
        .plan_mcp(PlanRequest {
            request_id: Uuid::new_v4().to_string(),
            paths: vec![source.to_string_lossy().into_owned()],
            output_subfolder: "compressed".into(),
            issue_limit: 10,
            include_technical_detail: false,
        })
        .result
        .expect("planning failed")
        .plan_id
        .unwrap();

    write_png(&source, [30, 20, 10, 255]);
    assert_eq!(fs::metadata(&source).unwrap().len(), original_size);
    filetime::set_file_mtime(&source, modified).unwrap();
    let result = service.compress_and_wait(compress_request(Some(plan_id), None));
    let result = result.result.expect("job did not start");
    assert_eq!(result.failed, 1);
    assert_eq!(
        result.issue_code_counts.get(&ErrorCode::SourceChanged),
        Some(&1)
    );
}

#[test]
fn refuses_scans_without_an_allowed_root() {
    let temporary = tempdir().unwrap();
    let service = AgentService::new(Vec::new(), false).unwrap();
    assert_error(
        service.plan_cli(PlanRequest {
            request_id: Uuid::new_v4().to_string(),
            paths: vec![temporary.path().to_string_lossy().into_owned()],
            output_subfolder: "compressed".into(),
            issue_limit: 10,
            include_technical_detail: false,
        }),
        ErrorCode::RootNotAllowed,
    );
}

fn compress_request(plan_id: Option<String>, paths: Option<Vec<String>>) -> CompressRequest {
    CompressRequest {
        request_id: Uuid::new_v4().to_string(),
        plan_id,
        paths,
        preset: CompressionPreset::Balanced,
        output_mode: OutputMode::Subfolder,
        output_subfolder: "compressed".into(),
        metadata_policy: MetadataPolicy::Essential,
        allow_conflicts: false,
        wait_ms: 5_000,
    }
}

fn assert_error<T>(result: Envelope<T>, code: ErrorCode) {
    assert_eq!(result.error.expect("expected an error").code, code);
}

fn write_png(path: &Path, color: [u8; 4]) {
    let file = fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&color).unwrap();
}
