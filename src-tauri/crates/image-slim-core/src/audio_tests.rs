use crate::EventSink;
use crate::audio;
use crate::batch::{self, BatchEngine, JobRegistry};
use crate::model::{
    BatchRequest, BatchStartStatus, BatchSummary, CompressionPreset, ImageFormat, InputItem,
    ItemProgress, MetadataPolicy, OutputMode, ScanEvent, ScanRequest, TaskStatus,
};
use crate::preview::PreviewCache;
use crate::scanner;
use crate::scheduler::WorkScheduler;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, atomic::AtomicBool, mpsc};
use std::time::Duration;
use tempfile::tempdir;

const WAV: &[u8] = include_bytes!("../tests/fixtures/tone.wav");
const MP3: &[u8] = include_bytes!("../tests/fixtures/tone.mp3");
const OGG: &[u8] = include_bytes!("../tests/fixtures/tone.ogg");

fn scan(path: &Path) -> Vec<InputItem> {
    let mut items = Vec::new();
    scanner::scan_stream(
        ScanRequest {
            scan_id: "audio-test".into(),
            paths: vec![path.to_string_lossy().into_owned()],
            output_subfolder: "compressed".into(),
            existing_ids: vec![],
            remaining_capacity: 100,
        },
        Arc::new(AtomicBool::new(false)),
        |event| match event {
            ScanEvent::Items {
                items: accepted, ..
            } => items.extend(accepted),
            ScanEvent::Issues { issues, .. } => panic!("scan issues: {issues:?}"),
            _ => {}
        },
    )
    .unwrap();
    items
}

fn request(items: Vec<InputItem>, mode: OutputMode, allow_conflicts: bool) -> BatchRequest {
    BatchRequest {
        items,
        preset: CompressionPreset::Lossless,
        audio_bitrate_kbps: Some(128),
        output_mode: mode,
        output_subfolder: "compressed".into(),
        metadata_policy: MetadataPolicy::Supported,
        allow_conflicts,
    }
}

struct Sink {
    items: Mutex<Vec<ItemProgress>>,
    summary: mpsc::Sender<BatchSummary>,
}
impl EventSink for Sink {
    fn scan_event(&self, _: ScanEvent) {}
    fn item_progress(&self, event: ItemProgress) {
        self.items.lock().unwrap().push(event);
    }
    fn batch_summary(&self, event: BatchSummary) {
        self.summary.send(event).unwrap();
    }
}

fn run(request: BatchRequest) -> (BatchSummary, Vec<ItemProgress>) {
    let (tx, rx) = mpsc::channel();
    let sink = Arc::new(Sink {
        items: Mutex::new(vec![]),
        summary: tx,
    });
    let result = batch::start(
        sink.clone(),
        JobRegistry::default(),
        WorkScheduler::with_budget(2 * 1024 * 1024 * 1024),
        PreviewCache::default(),
        request,
    )
    .unwrap();
    assert_eq!(result.status, BatchStartStatus::Started);
    let summary = rx.recv_timeout(Duration::from_secs(15)).unwrap();
    let events = sink.items.lock().unwrap().clone();
    (summary, events)
}

#[test]
fn real_audio_formats_decode_and_produce_valid_smaller_mp3_or_no_gain() {
    let fixtures: &[(&str, ImageFormat, &[u8])] = &[
        ("tone.wav", ImageFormat::Wav, WAV),
        (
            "mono.wav",
            ImageFormat::Wav,
            include_bytes!("../tests/fixtures/mono.wav"),
        ),
        ("tone.mp3", ImageFormat::Mp3, MP3),
        (
            "tone.flac",
            ImageFormat::Flac,
            include_bytes!("../tests/fixtures/tone.flac"),
        ),
        (
            "tone.m4a",
            ImageFormat::M4a,
            include_bytes!("../tests/fixtures/tone.m4a"),
        ),
        (
            "alac.m4a",
            ImageFormat::M4a,
            include_bytes!("../tests/fixtures/alac.m4a"),
        ),
        ("tone.ogg", ImageFormat::Ogg, OGG),
    ];
    let dir = tempdir().unwrap();
    for (name, format, bytes) in fixtures {
        let path = dir.path().join(name);
        fs::write(&path, bytes).unwrap();
        let item = scan(&path).remove(0);
        assert_eq!(item.format, *format);
        assert_eq!(item.audio.as_ref().unwrap().sample_rate, 44_100);
        for bitrate in audio::BITRATES {
            let result =
                audio::compress(Arc::from(*bytes), *format, bitrate, &AtomicBool::new(false))
                    .unwrap_or_else(|error| panic!("{name} at {bitrate}: {error:#}"));
            if let Some(output) = result {
                assert!(output.len() < bytes.len());
                assert_eq!(scanner::format_from_magic(&output), Some(ImageFormat::Mp3));
                let result_path = dir.path().join("result.mp3");
                fs::write(&result_path, output).unwrap();
                assert_eq!(
                    audio::inspect(&result_path, ImageFormat::Mp3)
                        .unwrap()
                        .channels,
                    item.audio.as_ref().unwrap().channels
                );
            } else {
                assert!(matches!(format, ImageFormat::Ogg | ImageFormat::M4a));
            }
        }
    }
}

#[test]
fn damaged_audio_and_cancelled_operations_never_produce_candidates() {
    let truncated: Arc<[u8]> = Arc::from(&WAV[..WAV.len() / 2]);
    assert!(audio::compress(truncated, ImageFormat::Wav, 128, &AtomicBool::new(false)).is_err());
    assert!(
        audio::compress(
            Arc::from(WAV),
            ImageFormat::Wav,
            128,
            &AtomicBool::new(true)
        )
        .is_err()
    );
    assert!(
        audio::compress(
            Arc::from(WAV),
            ImageFormat::Wav,
            123,
            &AtomicBool::new(false)
        )
        .is_err()
    );
    let dir = tempdir().unwrap();
    let path = dir.path().join("wrong.flac");
    fs::write(&path, WAV).unwrap();
    assert!(audio::inspect(&path, ImageFormat::Flac).is_err());
}

#[test]
fn audio_preserves_the_source_even_when_image_overwrite_is_selected() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("原音频.wav");
    fs::write(&source, WAV).unwrap();
    let (summary, events) = run(request(scan(&source), OutputMode::Overwrite, false));
    assert_eq!(summary.failed, 0, "{events:?}");
    assert_eq!(summary.completed, 1);
    assert_eq!(fs::read(&source).unwrap(), WAV);
    let target = dir.path().join("compressed/原音频.wav.mp3");
    assert_eq!(
        scanner::format_from_magic(&fs::read(target).unwrap()),
        Some(ImageFormat::Mp3)
    );
}

#[test]
fn audio_conflicts_require_confirmation_even_in_image_overwrite_mode() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("tone.wav");
    fs::write(&source, WAV).unwrap();
    let target = dir.path().join("compressed/tone.wav.mp3");
    fs::create_dir(target.parent().unwrap()).unwrap();
    fs::write(&target, b"existing output").unwrap();
    let (tx, _) = mpsc::channel();
    let sink = Arc::new(Sink {
        items: Mutex::new(vec![]),
        summary: tx,
    });
    let result = BatchEngine::default()
        .start(sink, request(scan(&source), OutputMode::Overwrite, false))
        .unwrap();
    assert_eq!(result.status, BatchStartStatus::Conflicts);
    assert_eq!(fs::read(&target).unwrap(), b"existing output");
    let (summary, events) = run(request(scan(&source), OutputMode::Overwrite, true));
    assert_eq!(summary.completed, 1, "{events:?}");
    assert_eq!(fs::read(source).unwrap(), WAV);
    assert_eq!(
        scanner::format_from_magic(&fs::read(target).unwrap()),
        Some(ImageFormat::Mp3)
    );
}

#[test]
fn no_gain_conversion_keeps_original_bytes_and_does_not_create_a_fake_mp3() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("tone.ogg");
    fs::write(&source, OGG).unwrap();
    let (summary, events) = run(request(scan(&source), OutputMode::Subfolder, false));
    assert_eq!(summary.unchanged, 1, "{events:?}");
    assert_eq!(summary.failed, 0);
    assert_eq!(events.last().unwrap().status, TaskStatus::Unchanged);
    assert_eq!(
        events.last().unwrap().output_path.as_deref(),
        Some(scanner::normalize_display_path(&source).as_str())
    );
    assert_eq!(fs::read(source).unwrap(), OGG);
    assert!(!dir.path().join("compressed/tone.ogg.mp3").exists());
}

#[test]
fn duplicate_audio_targets_are_rejected_before_any_file_is_written() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("tone.wav"), WAV).unwrap();
    fs::write(dir.path().join("tone.wav.mp3"), MP3).unwrap();
    let (tx, _) = mpsc::channel();
    let sink = Arc::new(Sink {
        items: Mutex::new(vec![]),
        summary: tx,
    });
    let result =
        BatchEngine::default().start(sink, request(scan(dir.path()), OutputMode::Subfolder, true));
    assert_eq!(
        result.unwrap_err().code,
        crate::error::ErrorCode::OutputConflict
    );
    assert!(!dir.path().join("compressed").exists());
}

#[test]
fn audio_output_cannot_replace_another_source_in_the_same_batch() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("tone.wav");
    let other_source = dir.path().join("compressed/tone.wav.mp3");
    fs::write(&source, WAV).unwrap();
    fs::create_dir(other_source.parent().unwrap()).unwrap();
    fs::write(&other_source, MP3).unwrap();
    let mut items = scan(&source);
    items.extend(scan(&other_source));
    let (tx, _) = mpsc::channel();
    let sink = Arc::new(Sink {
        items: Mutex::new(vec![]),
        summary: tx,
    });
    let result = BatchEngine::default().start(sink, request(items, OutputMode::Subfolder, true));
    assert_eq!(
        result.unwrap_err().code,
        crate::error::ErrorCode::OutputConflict
    );
    assert_eq!(fs::read(other_source).unwrap(), MP3);
}
