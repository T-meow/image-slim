use crate::protocol::{
    AGENT_PROTOCOL_VERSION, AgentCapabilities, CancelRequest, CancelResult, CompressRequest,
    Envelope, JobState, JobStatus, MAX_ISSUE_LIMIT, MAX_WAIT_MS, PlanRequest, PlanResult,
    StatusRequest,
};
use image_slim_core::EventSink;
use image_slim_core::access::AccessPolicy;
use image_slim_core::batch::BatchEngine;
use image_slim_core::error::{AppError, AppResult, ErrorCode};
use image_slim_core::limits;
use image_slim_core::model::{
    BatchRequest, BatchStartStatus, BatchSummary, InputItem, ItemProgress, ScanEvent, ScanRequest,
    TaskStatus,
};
use image_slim_core::{output, scanner};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const PLAN_TTL_MS: u64 = 30 * 60 * 1_000;
const RESULT_TTL_MS: u64 = 60 * 60 * 1_000;
const MAX_PLANS: usize = 4;
const MAX_COMPLETED_JOBS: usize = 32;
const MAX_REQUESTS: usize = 128;
const MAX_PATHS: usize = 1_000;
const MAX_RESPONSE_BYTES: usize = 32 * 1024;
const MAX_DETAIL_BYTES: usize = 2 * 1024;
const MAX_PATH_BYTES: usize = 4 * 1024;

pub trait Clock: Send + Sync {
    fn monotonic_ms(&self) -> u64;
    fn unix_ms(&self) -> u64;
}

#[derive(Debug)]
pub struct SystemClock {
    started: Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn monotonic_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn unix_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

#[derive(Clone)]
pub struct AgentService {
    inner: Arc<AgentInner>,
}

struct AgentInner {
    policy: AccessPolicy,
    engine: BatchEngine,
    plans: Mutex<VecDeque<StoredPlan>>,
    jobs: Mutex<HashMap<String, Arc<JobRecord>>>,
    requests: Mutex<VecDeque<CachedRequest>>,
    clock: Arc<dyn Clock>,
}

#[derive(Clone)]
struct PlannedItem {
    item: InputItem,
    source_hash: String,
}

#[derive(Clone)]
struct StoredPlan {
    id: String,
    expires_at: u64,
    items: Vec<PlannedItem>,
    issues: Vec<AppError>,
}

struct ScanOutcome {
    result: PlanResult,
    items: Vec<PlannedItem>,
    issues: Vec<AppError>,
}

struct CachedRequest {
    request_id: String,
    operation: &'static str,
    payload: Value,
    expires_at: u64,
    value: CachedValue,
}

enum CachedValue {
    Json(Value),
    Job(String),
}

struct JobRecord {
    data: Mutex<JobData>,
    changed: Condvar,
    clock: Arc<dyn Clock>,
}

struct JobData {
    job_id: String,
    state: JobState,
    total: usize,
    completed: usize,
    unchanged: usize,
    failed: usize,
    cancelled: usize,
    original_bytes: u64,
    output_bytes: u64,
    issues: Vec<AppError>,
    issue_code_counts: BTreeMap<ErrorCode, usize>,
    completed_at: Option<u64>,
}

impl AgentService {
    pub fn new(roots: Vec<PathBuf>, allow_overwrite: bool) -> AppResult<Self> {
        Self::with_clock(roots, allow_overwrite, Arc::new(SystemClock::default()))
    }

    pub fn with_clock(
        roots: Vec<PathBuf>,
        allow_overwrite: bool,
        clock: Arc<dyn Clock>,
    ) -> AppResult<Self> {
        Ok(Self {
            inner: Arc::new(AgentInner {
                policy: AccessPolicy::new(roots, allow_overwrite)?,
                engine: BatchEngine::default(),
                plans: Mutex::new(VecDeque::new()),
                jobs: Mutex::new(HashMap::new()),
                requests: Mutex::new(VecDeque::new()),
                clock,
            }),
        })
    }

    pub fn capabilities(&self) -> Envelope<AgentCapabilities> {
        Envelope::success(AgentCapabilities {
            agent_protocol_version: AGENT_PROTOCOL_VERSION,
            app_version: env!("CARGO_PKG_VERSION").into(),
            core: image_slim_core::capabilities(),
            metadata_policies: vec![
                image_slim_core::model::MetadataPolicy::Essential,
                image_slim_core::model::MetadataPolicy::Supported,
            ],
            allowed_roots: self.inner.policy.roots(),
            allow_overwrite: self.inner.policy.allow_overwrite(),
        })
    }

    pub fn plan_cli(&self, request: PlanRequest) -> Envelope<PlanResult> {
        self.plan(request, false)
    }

    pub fn plan_mcp(&self, request: PlanRequest) -> Envelope<PlanResult> {
        self.plan(request, true)
    }

    pub fn compress(&self, request: CompressRequest) -> Envelope<JobStatus> {
        let payload = match serde_json::to_value(&request) {
            Ok(payload) => payload,
            Err(error) => return Envelope::failure(AppError::internal(error)),
        };
        if let Err(error) = validate_request_id(&request.request_id) {
            return Envelope::failure(error);
        }
        if let Some(cached) = self.cached_compress(&request.request_id, &payload) {
            return cached;
        }
        let response = self.compress_uncached(&request);
        let cached = match response.result.as_ref() {
            Some(result) => CachedValue::Job(result.job_id.clone()),
            None => CachedValue::Json(
                serde_json::to_value(&response).expect("error envelope must serialize"),
            ),
        };
        self.cache_request(&request.request_id, "compress", payload, cached);
        response
    }

    pub fn compress_and_wait(&self, mut request: CompressRequest) -> Envelope<JobStatus> {
        request.wait_ms = MAX_WAIT_MS;
        let mut response = self.compress(request);
        loop {
            let Some(result) = response.result.as_ref() else {
                return response;
            };
            if !matches!(result.state, JobState::Running | JobState::Cancelling) {
                return response;
            }
            let job_id = result.job_id.clone();
            if let Some(record) = self.job(&job_id) {
                record.wait(250);
            }
            response = self.status(StatusRequest {
                job_id,
                issue_cursor: 0,
                issue_limit: crate::protocol::DEFAULT_ISSUE_LIMIT,
                include_technical_detail: false,
            });
        }
    }

    pub fn status(&self, request: StatusRequest) -> Envelope<JobStatus> {
        if let Err(error) = validate_issue_page(request.issue_cursor, request.issue_limit) {
            return Envelope::failure(error);
        }
        self.prune_jobs();
        let Some(record) = self.job(&request.job_id) else {
            return Envelope::failure(AppError::new(ErrorCode::JobNotFound));
        };
        self.budget_status(record.status(
            request.issue_cursor,
            request.issue_limit,
            request.include_technical_detail,
        ))
    }

    pub fn cancel(&self, request: CancelRequest) -> Envelope<CancelResult> {
        let Some(record) = self.job(&request.job_id) else {
            return Envelope::failure(AppError::new(ErrorCode::JobNotFound));
        };
        let accepted = self.inner.engine.cancel(&request.job_id);
        let mut data = record.data.lock().expect("job record poisoned");
        if accepted && data.state == JobState::Running {
            data.state = JobState::Cancelling;
        }
        Envelope::success(CancelResult {
            job_id: request.job_id,
            accepted: accepted || data.state == JobState::Cancelling,
            state: data.state,
        })
    }

    pub fn cancel_active(&self) -> bool {
        let jobs = self.inner.jobs.lock().expect("job store poisoned");
        let active = jobs.values().find_map(|record| {
            let data = record.data.lock().expect("job record poisoned");
            matches!(data.state, JobState::Running | JobState::Cancelling)
                .then(|| data.job_id.clone())
        });
        drop(jobs);
        active.is_some_and(|job_id| self.inner.engine.cancel(&job_id))
    }

    fn plan(&self, request: PlanRequest, persist: bool) -> Envelope<PlanResult> {
        let payload = match serde_json::to_value(&request) {
            Ok(payload) => payload,
            Err(error) => return Envelope::failure(AppError::internal(error)),
        };
        if let Err(error) = validate_request_id(&request.request_id) {
            return Envelope::failure(error);
        }
        if let Some(mut cached) =
            self.cached_json::<PlanResult>(&request.request_id, "plan", &payload)
        {
            if persist
                && let Some(result) = cached.result.as_mut()
                && let Some(plan_id) = result.plan_id.as_deref()
                && self.touch_plan(plan_id)
            {
                result.expires_at_ms = Some(self.inner.clock.unix_ms() + PLAN_TTL_MS);
            }
            return cached;
        }
        let response = match self.scan(&request, persist) {
            Ok(outcome) => {
                if persist {
                    let plan_id = outcome
                        .result
                        .plan_id
                        .clone()
                        .expect("persisted plans have ids");
                    self.store_plan(StoredPlan {
                        id: plan_id,
                        expires_at: self.inner.clock.monotonic_ms() + PLAN_TTL_MS,
                        items: outcome.items,
                        issues: outcome.issues,
                    });
                }
                Envelope::success(outcome.result)
            }
            Err(error) => Envelope::failure(error),
        };
        self.cache_request(
            &request.request_id,
            "plan",
            payload,
            CachedValue::Json(serde_json::to_value(&response).expect("plan must serialize")),
        );
        response
    }

    fn scan(&self, request: &PlanRequest, persist: bool) -> AppResult<ScanOutcome> {
        validate_paths(&request.paths)?;
        validate_issue_page(0, request.issue_limit)?;
        self.inner.policy.ensure_paths(&request.paths)?;
        scanner::validate_output_subfolder(&request.output_subfolder)?;

        let mut items = Vec::new();
        let mut issues = Vec::new();
        let mut visited = 0usize;
        let mut limit_reached = false;
        scanner::scan_stream(
            ScanRequest {
                scan_id: Uuid::new_v4().to_string(),
                paths: request.paths.clone(),
                output_subfolder: request.output_subfolder.clone(),
                existing_ids: Vec::new(),
                remaining_capacity: limits::MAX_QUEUE_ITEMS,
            },
            Arc::new(AtomicBool::new(false)),
            |event| match event {
                ScanEvent::Items { items: scanned, .. } => items.extend(scanned),
                ScanEvent::Issues {
                    issues: scanned, ..
                } => issues.extend(scanned),
                ScanEvent::Progress { visited: count, .. } => visited = count,
                ScanEvent::Finished {
                    limit_reached: reached,
                    ..
                } => limit_reached = reached,
            },
        )?;

        let mut planned = Vec::with_capacity(items.len());
        for item in items {
            if let Err(error) = self.inner.policy.ensure_item(&item) {
                issues.push(error);
                continue;
            }
            match output::file_content_hash(std::path::Path::new(&item.source_path)) {
                Ok(source_hash) => planned.push(PlannedItem { item, source_hash }),
                Err(error) => issues.push(AppError::operation(
                    ErrorCode::IoFailed,
                    error,
                    &item.source_path,
                )),
            }
        }

        let plan_id = persist.then(|| Uuid::new_v4().to_string());
        let expires_at_ms = persist.then(|| self.inner.clock.unix_ms() + PLAN_TTL_MS);
        let all_issues = issues;
        let mut result = plan_result(
            plan_id,
            expires_at_ms,
            visited,
            &planned,
            &all_issues,
            limit_reached,
            request.issue_limit,
            request.include_technical_detail,
        );
        budget_plan(&mut result);
        Ok(ScanOutcome {
            result,
            items: planned,
            issues: all_issues,
        })
    }

    fn compress_uncached(&self, request: &CompressRequest) -> Envelope<JobStatus> {
        if request.wait_ms > MAX_WAIT_MS {
            return Envelope::failure(invalid_request("wait_ms exceeds 5000"));
        }
        if let Err(error) = self.inner.policy.ensure_output_mode(request.output_mode) {
            return Envelope::failure(error);
        }
        if request.output_mode == image_slim_core::model::OutputMode::Subfolder
            && let Err(error) = scanner::validate_output_subfolder(&request.output_subfolder)
        {
            return Envelope::failure(error);
        }

        let source = match (&request.plan_id, &request.paths) {
            (Some(plan_id), None) => self.load_plan(plan_id),
            (None, Some(paths)) => self.scan(
                &PlanRequest {
                    request_id: request.request_id.clone(),
                    paths: paths.clone(),
                    output_subfolder: request.output_subfolder.clone(),
                    issue_limit: crate::protocol::DEFAULT_ISSUE_LIMIT,
                    include_technical_detail: false,
                },
                false,
            ),
            _ => Err(invalid_request(
                "exactly one of plan_id and paths must be provided",
            )),
        };
        let outcome = match source {
            Ok(source) => source,
            Err(error) => return Envelope::failure(error),
        };

        let record = Arc::new(JobRecord::new(
            outcome.items.len(),
            outcome.issues,
            self.inner.clock.clone(),
        ));
        if outcome.items.is_empty() {
            let job_id = Uuid::new_v4().to_string();
            record.complete_without_work(job_id.clone());
            self.insert_job(job_id.clone(), record.clone());
            return self.budget_status(record.status(0, 10, false));
        }

        let expected_hashes = outcome
            .items
            .iter()
            .map(|planned| (planned.item.id.clone(), planned.source_hash.clone()))
            .collect();
        let batch_request = BatchRequest {
            items: outcome
                .items
                .into_iter()
                .map(|planned| planned.item)
                .collect(),
            preset: request.preset,
            output_mode: request.output_mode,
            output_subfolder: request.output_subfolder.clone(),
            metadata_policy: request.metadata_policy,
            allow_conflicts: request.allow_conflicts,
        };
        let started = match self.inner.engine.start_with_source_hashes(
            record.clone(),
            batch_request,
            expected_hashes,
        ) {
            Ok(started) => started,
            Err(error) => return Envelope::failure(error),
        };
        if started.status == BatchStartStatus::Conflicts {
            return Envelope::failure(
                AppError::new(ErrorCode::OutputConflict)
                    .param("conflict_count", started.conflict_count)
                    .retryable(true),
            );
        }
        let job_id = started.batch_id.expect("started batch must have id");
        record.set_job_id(job_id.clone());
        self.insert_job(job_id, record.clone());
        record.wait(request.wait_ms);
        self.budget_status(record.status(0, 10, false))
    }

    fn load_plan(&self, plan_id: &str) -> AppResult<ScanOutcome> {
        let now = self.inner.clock.monotonic_ms();
        let mut plans = self.inner.plans.lock().expect("plan store poisoned");
        plans.retain(|plan| plan.expires_at > now);
        let Some(index) = plans.iter().position(|plan| plan.id == plan_id) else {
            return Err(AppError::new(ErrorCode::PlanExpired));
        };
        let mut plan = plans.remove(index).expect("plan index must exist");
        plan.expires_at = now + PLAN_TTL_MS;
        let items = plan.items.clone();
        let issues = plan.issues.clone();
        plans.push_front(plan);
        Ok(ScanOutcome {
            result: plan_result(None, None, 0, &items, &issues, false, 10, false),
            items,
            issues,
        })
    }

    fn store_plan(&self, plan: StoredPlan) {
        let now = self.inner.clock.monotonic_ms();
        let mut plans = self.inner.plans.lock().expect("plan store poisoned");
        plans.retain(|item| item.expires_at > now && item.id != plan.id);
        plans.push_front(plan);
        plans.truncate(MAX_PLANS);
    }

    fn touch_plan(&self, plan_id: &str) -> bool {
        let now = self.inner.clock.monotonic_ms();
        let mut plans = self.inner.plans.lock().expect("plan store poisoned");
        plans.retain(|plan| plan.expires_at > now);
        let Some(index) = plans.iter().position(|plan| plan.id == plan_id) else {
            return false;
        };
        let mut plan = plans.remove(index).expect("plan index must exist");
        plan.expires_at = now + PLAN_TTL_MS;
        plans.push_front(plan);
        true
    }

    fn insert_job(&self, job_id: String, record: Arc<JobRecord>) {
        self.prune_jobs();
        self.inner
            .jobs
            .lock()
            .expect("job store poisoned")
            .insert(job_id, record);
    }

    fn prune_jobs(&self) {
        let now = self.inner.clock.monotonic_ms();
        let mut jobs = self.inner.jobs.lock().expect("job store poisoned");
        jobs.retain(|_, record| {
            record
                .data
                .lock()
                .expect("job record poisoned")
                .completed_at
                .is_none_or(|completed| completed + RESULT_TTL_MS > now)
        });
        let mut completed = jobs
            .iter()
            .filter_map(|(id, record)| {
                record
                    .data
                    .lock()
                    .expect("job record poisoned")
                    .completed_at
                    .map(|at| (id.clone(), at))
            })
            .collect::<Vec<_>>();
        completed.sort_by_key(|(_, at)| *at);
        let remove_count = completed.len().saturating_sub(MAX_COMPLETED_JOBS);
        for (id, _) in completed.into_iter().take(remove_count) {
            jobs.remove(&id);
        }
    }

    fn job(&self, job_id: &str) -> Option<Arc<JobRecord>> {
        self.inner
            .jobs
            .lock()
            .expect("job store poisoned")
            .get(job_id)
            .cloned()
    }

    fn cached_json<T: DeserializeOwned>(
        &self,
        request_id: &str,
        operation: &'static str,
        payload: &Value,
    ) -> Option<Envelope<T>> {
        match self.cached_value(request_id, operation, payload)? {
            Ok(CachedValue::Json(value)) => serde_json::from_value(value).ok(),
            Ok(CachedValue::Job(_)) => Some(Envelope::failure(invalid_request(
                "request_id belongs to another result type",
            ))),
            Err(error) => Some(Envelope::failure(error)),
        }
    }

    fn cached_compress(&self, request_id: &str, payload: &Value) -> Option<Envelope<JobStatus>> {
        match self.cached_value(request_id, "compress", payload)? {
            Ok(CachedValue::Job(job_id)) => Some(self.status(StatusRequest {
                job_id,
                issue_cursor: 0,
                issue_limit: 10,
                include_technical_detail: false,
            })),
            Ok(CachedValue::Json(value)) => serde_json::from_value(value).ok(),
            Err(error) => Some(Envelope::failure(error)),
        }
    }

    fn cached_value(
        &self,
        request_id: &str,
        operation: &'static str,
        payload: &Value,
    ) -> Option<Result<CachedValue, AppError>> {
        let now = self.inner.clock.monotonic_ms();
        let mut requests = self.inner.requests.lock().expect("request cache poisoned");
        requests.retain(|entry| entry.expires_at > now);
        let index = requests
            .iter()
            .position(|entry| entry.request_id == request_id)?;
        let entry = requests.remove(index).expect("request index must exist");
        if entry.operation != operation || entry.payload != *payload {
            requests.push_front(entry);
            return Some(Err(invalid_request(
                "request_id was already used with different input",
            )));
        }
        let value = match &entry.value {
            CachedValue::Json(value) => CachedValue::Json(value.clone()),
            CachedValue::Job(job_id) => CachedValue::Job(job_id.clone()),
        };
        requests.push_front(entry);
        Some(Ok(value))
    }

    fn cache_request(
        &self,
        request_id: &str,
        operation: &'static str,
        payload: Value,
        value: CachedValue,
    ) {
        let mut requests = self.inner.requests.lock().expect("request cache poisoned");
        requests.retain(|entry| entry.request_id != request_id);
        requests.push_front(CachedRequest {
            request_id: request_id.into(),
            operation,
            payload,
            expires_at: self.inner.clock.monotonic_ms() + RESULT_TTL_MS,
            value,
        });
        requests.truncate(MAX_REQUESTS);
    }

    fn budget_status(&self, mut status: JobStatus) -> Envelope<JobStatus> {
        while status.issues.len() > 1
            && serde_json::to_vec(&Envelope::success(status.clone()))
                .is_ok_and(|bytes| bytes.len() > MAX_RESPONSE_BYTES)
        {
            status.issues.pop();
            status.next_issue_cursor = Some(
                status
                    .next_issue_cursor
                    .unwrap_or(status.issue_count)
                    .saturating_sub(1),
            );
        }
        Envelope::success(status)
    }
}

impl JobRecord {
    fn new(total: usize, issues: Vec<AppError>, clock: Arc<dyn Clock>) -> Self {
        let issue_code_counts = count_issues(&issues);
        Self {
            data: Mutex::new(JobData {
                job_id: String::new(),
                state: JobState::Running,
                total,
                completed: 0,
                unchanged: 0,
                failed: 0,
                cancelled: 0,
                original_bytes: 0,
                output_bytes: 0,
                issues,
                issue_code_counts,
                completed_at: None,
            }),
            changed: Condvar::new(),
            clock,
        }
    }

    fn set_job_id(&self, job_id: String) {
        self.data.lock().expect("job record poisoned").job_id = job_id;
    }

    fn complete_without_work(&self, job_id: String) {
        let mut data = self.data.lock().expect("job record poisoned");
        data.job_id = job_id;
        data.state = JobState::Completed;
        data.completed_at = Some(self.clock.monotonic_ms());
        self.changed.notify_all();
    }

    fn wait(&self, wait_ms: u64) {
        if wait_ms == 0 {
            return;
        }
        let data = self.data.lock().expect("job record poisoned");
        if matches!(data.state, JobState::Running | JobState::Cancelling) {
            let _ = self
                .changed
                .wait_timeout(data, Duration::from_millis(wait_ms))
                .expect("job record poisoned while waiting");
        }
    }

    fn status(&self, cursor: usize, limit: usize, include_technical_detail: bool) -> JobStatus {
        let data = self.data.lock().expect("job record poisoned");
        let end = cursor.saturating_add(limit).min(data.issues.len());
        let issues = if cursor < data.issues.len() {
            data.issues[cursor..end]
                .iter()
                .map(|issue| project_issue(issue, include_technical_detail))
                .collect()
        } else {
            Vec::new()
        };
        JobStatus {
            job_id: data.job_id.clone(),
            state: data.state,
            total: data.total,
            completed: data.completed,
            unchanged: data.unchanged,
            failed: data.failed,
            cancelled: data.cancelled,
            original_bytes: data.original_bytes,
            output_bytes: data.output_bytes,
            saved_bytes: data.original_bytes.saturating_sub(data.output_bytes),
            issue_count: data.issues.len(),
            issue_code_counts: data.issue_code_counts.clone(),
            issues,
            next_issue_cursor: (end < data.issues.len()).then_some(end),
        }
    }
}

impl EventSink for JobRecord {
    fn scan_event(&self, _event: ScanEvent) {}

    fn item_progress(&self, progress: ItemProgress) {
        if progress.status == TaskStatus::Processing {
            return;
        }
        let mut data = self.data.lock().expect("job record poisoned");
        if data.job_id.is_empty() {
            data.job_id = progress.batch_id;
        }
        match progress.status {
            TaskStatus::Completed => data.completed += 1,
            TaskStatus::Unchanged => data.unchanged += 1,
            TaskStatus::Failed => {
                data.failed += 1;
                if let Some(error) = progress.error {
                    *data.issue_code_counts.entry(error.code).or_default() += 1;
                    data.issues.push(error);
                }
            }
            TaskStatus::Cancelled => data.cancelled += 1,
            TaskStatus::Ready | TaskStatus::Processing => {}
        }
        if let Some(output_size) = progress.output_size {
            data.output_bytes = data.output_bytes.saturating_add(output_size);
            data.original_bytes = data
                .original_bytes
                .saturating_add(output_size.saturating_add(progress.saved_bytes));
        }
        self.changed.notify_all();
    }

    fn batch_summary(&self, summary: BatchSummary) {
        let mut data = self.data.lock().expect("job record poisoned");
        data.job_id = summary.batch_id;
        data.completed = summary.completed;
        data.unchanged = summary.unchanged;
        data.failed = summary.failed;
        data.cancelled = summary.cancelled;
        data.original_bytes = summary.original_bytes;
        data.output_bytes = summary.output_bytes;
        data.state = if summary.cancelled > 0 || data.state == JobState::Cancelling {
            JobState::Cancelled
        } else {
            JobState::Completed
        };
        data.completed_at = Some(self.clock.monotonic_ms());
        self.changed.notify_all();
    }
}

fn validate_request_id(request_id: &str) -> AppResult<()> {
    Uuid::parse_str(request_id)
        .map(|_| ())
        .map_err(|_| invalid_request("request_id must be a UUID"))
}

fn validate_paths(paths: &[String]) -> AppResult<()> {
    if paths.is_empty() || paths.len() > MAX_PATHS {
        Err(invalid_request(
            "paths must contain between 1 and 1000 entries",
        ))
    } else {
        Ok(())
    }
}

fn validate_issue_page(_cursor: usize, limit: usize) -> AppResult<()> {
    if limit == 0 || limit > MAX_ISSUE_LIMIT {
        Err(invalid_request("issue cursor or limit is out of range"))
    } else {
        Ok(())
    }
}

fn invalid_request(detail: &str) -> AppError {
    AppError::new(ErrorCode::InvalidRequest).detail(detail)
}

#[allow(clippy::too_many_arguments)]
fn plan_result(
    plan_id: Option<String>,
    expires_at_ms: Option<u64>,
    visited: usize,
    planned: &[PlannedItem],
    issues: &[AppError],
    limit_reached: bool,
    issue_limit: usize,
    include_technical_detail: bool,
) -> PlanResult {
    let mut format_counts = BTreeMap::new();
    let mut input_bytes = 0u64;
    for planned in planned {
        *format_counts.entry(planned.item.format).or_default() += 1;
        input_bytes = input_bytes.saturating_add(planned.item.original_size);
    }
    let end = issue_limit.min(issues.len());
    PlanResult {
        plan_id,
        expires_at_ms,
        visited,
        accepted: planned.len(),
        input_bytes,
        format_counts,
        issue_count: issues.len(),
        issue_code_counts: count_issues(issues),
        issues: issues[..end]
            .iter()
            .map(|issue| project_issue(issue, include_technical_detail))
            .collect(),
        next_issue_cursor: (end < issues.len()).then_some(end),
        limit_reached,
    }
}

fn count_issues(issues: &[AppError]) -> BTreeMap<ErrorCode, usize> {
    let mut counts = BTreeMap::new();
    for issue in issues {
        *counts.entry(issue.code).or_default() += 1;
    }
    counts
}

fn project_issue(issue: &AppError, include_technical_detail: bool) -> AppError {
    let mut projected = issue.clone();
    projected.detail = if include_technical_detail {
        projected
            .detail
            .as_deref()
            .map(|detail| truncate_utf8(detail, MAX_DETAIL_BYTES))
    } else {
        None
    };
    if let Some(path) = projected.path.as_deref()
        && path.len() > MAX_PATH_BYTES
    {
        projected.path = Some(truncate_utf8(path, MAX_PATH_BYTES));
        projected
            .params
            .insert("path_truncated".into(), "true".into());
    }
    projected
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.into();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn budget_plan(result: &mut PlanResult) {
    while result.issues.len() > 1
        && serde_json::to_vec(&Envelope::success(result.clone()))
            .is_ok_and(|bytes| bytes.len() > MAX_RESPONSE_BYTES)
    {
        result.issues.pop();
        result.next_issue_cursor = Some(
            result
                .next_issue_cursor
                .unwrap_or(result.issue_count)
                .saturating_sub(1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image_slim_core::model::ImageFormat;

    #[test]
    fn ten_thousand_item_plan_returns_only_aggregates() {
        let planned = (0..limits::MAX_QUEUE_ITEMS)
            .map(|index| PlannedItem {
                item: InputItem {
                    id: format!("item-{index}"),
                    source_path: format!(r"D:\Pictures\private-{index}.png"),
                    input_root: r"D:\Pictures".into(),
                    relative_path: format!("private-{index}.png"),
                    name: format!("private-{index}.png"),
                    format: ImageFormat::Png,
                    width: 1,
                    height: 1,
                    original_size: 100,
                    modified_ms: 1,
                },
                source_hash: "hash".into(),
            })
            .collect::<Vec<_>>();
        let result = plan_result(
            Some("plan".into()),
            Some(1),
            limits::MAX_QUEUE_ITEMS,
            &planned,
            &[],
            true,
            10,
            false,
        );
        let serialized = serde_json::to_vec(&Envelope::success(result)).unwrap();
        assert!(serialized.len() < 8 * 1024);
        assert!(!String::from_utf8(serialized).unwrap().contains("private-"));
    }

    #[test]
    fn issue_pages_are_stable_and_bounded() {
        let issues = (0..128)
            .map(|index| {
                AppError::new(ErrorCode::InvalidImage)
                    .path(format!(r"D:\Pictures\bad-{index}.png"))
                    .detail("technical detail".repeat(500))
            })
            .collect::<Vec<_>>();
        let record = JobRecord::new(0, issues, Arc::new(SystemClock::default()));
        record.complete_without_work("job".into());
        let service = AgentService::new(Vec::new(), false).unwrap();
        let first = service.budget_status(record.status(0, 50, true));
        let first = first.result.unwrap();
        let cursor = first.next_issue_cursor.unwrap();
        let second = service.budget_status(record.status(cursor, 50, true));
        let second = second.result.unwrap();
        assert_eq!(
            first.issues[0].path.as_deref(),
            Some(r"D:\Pictures\bad-0.png")
        );
        assert_eq!(
            second.issues[0].path.as_deref(),
            Some(format!(r"D:\Pictures\bad-{cursor}.png").as_str())
        );
        assert!(serde_json::to_vec(&Envelope::success(first)).unwrap().len() <= MAX_RESPONSE_BYTES);
    }
}
