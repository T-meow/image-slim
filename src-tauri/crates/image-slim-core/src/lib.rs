pub mod access;
pub mod batch;
mod codecs;
pub mod error;
pub mod limits;
mod metadata;
pub mod model;
pub mod output;
pub mod preview;
pub mod scanner;
pub mod scheduler;

use crate::model::{BatchSummary, ItemProgress, ScanEvent};

pub trait EventSink: Send + Sync {
    fn scan_event(&self, event: ScanEvent);
    fn item_progress(&self, progress: ItemProgress);
    fn batch_summary(&self, summary: BatchSummary);
}

pub use limits::capabilities;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BatchSummary, ItemProgress, TaskStatus};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        scan: Mutex<Vec<ScanEvent>>,
        items: Mutex<Vec<ItemProgress>>,
        summaries: Mutex<Vec<BatchSummary>>,
    }

    impl EventSink for RecordingSink {
        fn scan_event(&self, event: ScanEvent) {
            self.scan.lock().unwrap().push(event);
        }

        fn item_progress(&self, progress: ItemProgress) {
            self.items.lock().unwrap().push(progress);
        }

        fn batch_summary(&self, summary: BatchSummary) {
            self.summaries.lock().unwrap().push(summary);
        }
    }

    #[test]
    fn event_sink_preserves_the_three_gui_payload_shapes() {
        let sink = RecordingSink::default();
        sink.scan_event(ScanEvent::Finished {
            scan_id: "scan".into(),
            accepted: 1,
            issue_count: 0,
            cancelled: false,
            limit_reached: false,
        });
        sink.item_progress(ItemProgress {
            batch_id: "batch".into(),
            item_id: "item".into(),
            status: TaskStatus::Completed,
            output_path: Some(r"D:\output.png".into()),
            output_size: Some(10),
            saved_bytes: 5,
            error: None,
        });
        sink.batch_summary(BatchSummary {
            batch_id: "batch".into(),
            completed: 1,
            unchanged: 0,
            failed: 0,
            cancelled: 0,
            original_bytes: 15,
            output_bytes: 10,
        });

        let scan = serde_json::to_value(&sink.scan.lock().unwrap()[0]).unwrap();
        let item = serde_json::to_value(&sink.items.lock().unwrap()[0]).unwrap();
        let summary = serde_json::to_value(&sink.summaries.lock().unwrap()[0]).unwrap();
        assert_eq!(scan["type"], "finished");
        assert_eq!(item["status"], "completed");
        assert_eq!(summary["completed"], 1);
    }
}
