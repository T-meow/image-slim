use crate::error::{AppError, AppResult, ErrorCode};
use crate::limits;
use crate::model::InputItem;
use std::sync::{Arc, Condvar, Mutex};

const MAX_ACTIVE_JOBS: usize = 2;
const GIB: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Default)]
pub struct WorkScheduler {
    inner: Arc<SchedulerInner>,
}

#[derive(Default)]
struct SchedulerInner {
    state: Mutex<SchedulerState>,
    changed: Condvar,
    #[cfg(test)]
    test_budget: Mutex<Option<u64>>,
}

#[derive(Default)]
struct SchedulerState {
    batch_active: bool,
    active_jobs: usize,
    active_previews: usize,
    reserved_bytes: u64,
}

pub struct BatchGuard {
    inner: Arc<SchedulerInner>,
}

pub struct WorkPermit {
    inner: Arc<SchedulerInner>,
    reserved_bytes: u64,
    preview: bool,
}

impl WorkScheduler {
    pub fn begin_batch(&self) -> AppResult<BatchGuard> {
        let mut state = self.inner.state.lock().expect("scheduler poisoned");
        if state.batch_active {
            return Err(AppError::new(ErrorCode::BatchRunning).retryable(true));
        }
        state.batch_active = true;
        while state.active_previews > 0 {
            state = self.inner.changed.wait(state).expect("scheduler poisoned");
        }
        Ok(BatchGuard {
            inner: self.inner.clone(),
        })
    }

    pub fn ensure_preview_allowed(&self) -> AppResult<()> {
        let state = self.inner.state.lock().expect("scheduler poisoned");
        if state.batch_active {
            return Err(AppError::new(ErrorCode::PreviewPaused).retryable(true));
        }
        Ok(())
    }

    pub fn acquire_batch(&self, item: &InputItem) -> AppResult<WorkPermit> {
        self.acquire(item, false)
    }

    pub fn acquire_preview(&self, item: &InputItem) -> AppResult<WorkPermit> {
        self.acquire(item, true)
    }

    fn acquire(&self, item: &InputItem, preview: bool) -> AppResult<WorkPermit> {
        let estimate = limits::estimated_peak_bytes(item);
        let budget = self.memory_budget()?;
        if estimate > budget {
            return Err(AppError::new(ErrorCode::InsufficientMemory)
                .path(&item.source_path)
                .param("required", estimate)
                .param("available", budget)
                .retryable(true));
        }

        let mut state = self.inner.state.lock().expect("scheduler poisoned");
        if preview && state.batch_active {
            return Err(AppError::new(ErrorCode::PreviewPaused).retryable(true));
        }
        while state.active_jobs >= MAX_ACTIVE_JOBS
            || state.reserved_bytes.saturating_add(estimate) > budget
        {
            state = self.inner.changed.wait(state).expect("scheduler poisoned");
            if preview && state.batch_active {
                return Err(AppError::new(ErrorCode::PreviewPaused).retryable(true));
            }
        }
        state.active_jobs += 1;
        if preview {
            state.active_previews += 1;
        }
        state.reserved_bytes = state.reserved_bytes.saturating_add(estimate);
        Ok(WorkPermit {
            inner: self.inner.clone(),
            reserved_bytes: estimate,
            preview,
        })
    }

    fn memory_budget(&self) -> AppResult<u64> {
        #[cfg(test)]
        if let Some(budget) = *self.inner.test_budget.lock().expect("scheduler poisoned") {
            return Ok(budget);
        }

        let (total, available) = physical_memory()?;
        let reserve = GIB.max(total / 5);
        Ok(available.saturating_sub(reserve))
    }

    #[cfg(test)]
    pub(crate) fn with_budget(budget: u64) -> Self {
        let scheduler = Self::default();
        *scheduler
            .inner
            .test_budget
            .lock()
            .expect("scheduler poisoned") = Some(budget);
        scheduler
    }
}

impl Drop for BatchGuard {
    fn drop(&mut self) {
        let mut state = self.inner.state.lock().expect("scheduler poisoned");
        state.batch_active = false;
        self.inner.changed.notify_all();
    }
}

impl Drop for WorkPermit {
    fn drop(&mut self) {
        let mut state = self.inner.state.lock().expect("scheduler poisoned");
        state.active_jobs = state.active_jobs.saturating_sub(1);
        if self.preview {
            state.active_previews = state.active_previews.saturating_sub(1);
        }
        state.reserved_bytes = state.reserved_bytes.saturating_sub(self.reserved_bytes);
        self.inner.changed.notify_all();
    }
}

#[cfg(windows)]
fn physical_memory() -> AppResult<(u64, u64)> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status: MEMORYSTATUSEX = unsafe { zeroed() };
    status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return Err(AppError::internal(std::io::Error::last_os_error()));
    }
    Ok((status.ullTotalPhys, status.ullAvailPhys))
}

#[cfg(not(windows))]
fn physical_memory() -> AppResult<(u64, u64)> {
    Ok((8 * GIB, 6 * GIB))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ImageFormat;

    fn item(width: u32, height: u32) -> InputItem {
        InputItem {
            id: "test".into(),
            source_path: "test.png".into(),
            input_root: ".".into(),
            relative_path: "test.png".into(),
            name: "test.png".into(),
            format: ImageFormat::Png,
            width,
            height,
            original_size: 1024,
            modified_ms: 0,
        }
    }

    #[test]
    fn rejects_work_that_exceeds_the_budget() {
        let scheduler = WorkScheduler::with_budget(256 * 1024 * 1024);
        let error = match scheduler.acquire_batch(&item(10_000, 10_000)) {
            Ok(_) => panic!("work unexpectedly fit in the budget"),
            Err(error) => error,
        };
        assert_eq!(error.code, ErrorCode::InsufficientMemory);
    }

    #[test]
    fn pauses_previews_while_a_batch_is_active() {
        let scheduler = WorkScheduler::with_budget(8 * GIB);
        let guard = scheduler.begin_batch().unwrap();
        assert_eq!(
            scheduler.ensure_preview_allowed().unwrap_err().code,
            ErrorCode::PreviewPaused
        );
        drop(guard);
        assert!(scheduler.ensure_preview_allowed().is_ok());
    }

    #[test]
    fn batch_waits_for_an_active_preview() {
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        let scheduler = WorkScheduler::with_budget(8 * GIB);
        let preview = scheduler.acquire_preview(&item(100, 100)).unwrap();
        let waiting_scheduler = scheduler.clone();
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let guard = waiting_scheduler.begin_batch().unwrap();
            sender.send(()).unwrap();
            drop(guard);
        });

        let deadline = Instant::now() + Duration::from_secs(1);
        while scheduler.ensure_preview_allowed().is_ok() {
            assert!(
                Instant::now() < deadline,
                "batch did not enter its wait state"
            );
            std::thread::yield_now();
        }
        assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
        let error = match scheduler.acquire_preview(&item(100, 100)) {
            Ok(_) => panic!("preview started while a batch was waiting"),
            Err(error) => error,
        };
        assert_eq!(error.code, ErrorCode::PreviewPaused);
        drop(preview);
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        thread.join().unwrap();
    }

    #[test]
    fn memory_budget_makes_large_jobs_exclusive() {
        use std::sync::mpsc;
        use std::time::Duration;

        let sample = item(1_000, 1_000);
        let estimate = limits::estimated_peak_bytes(&sample);
        let scheduler = WorkScheduler::with_budget(estimate + estimate / 2);
        let first = scheduler.acquire_batch(&sample).unwrap();
        let waiting_scheduler = scheduler.clone();
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let _second = waiting_scheduler.acquire_batch(&sample).unwrap();
            sender.send(()).unwrap();
        });

        assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        thread.join().unwrap();
    }
}
