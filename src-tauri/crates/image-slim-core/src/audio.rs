use crate::model::{AudioInfo, ImageFormat};
use anyhow::{Context, Result, anyhow, bail, ensure};
use mp3lame_encoder::{Bitrate, Builder, FlushGap, InterleavedPcm, MonoPcm, Quality};
use std::fs::File;
use std::io::{Cursor, ErrorKind};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{
    CODEC_TYPE_AAC, CODEC_TYPE_ALAC, CODEC_TYPE_FLAC, CODEC_TYPE_MP3, CODEC_TYPE_NULL,
    CODEC_TYPE_VORBIS, Decoder, DecoderOptions,
};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub const BITRATES: [u32; 3] = [192, 128, 64];
pub const MAX_DURATION_SECONDS: u64 = 6 * 60 * 60;

struct AudioReader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    info: AudioInfo,
    expected_frames: Option<u64>,
    exact_length: bool,
    first_samples: Vec<f32>,
}

fn open(source: Box<dyn MediaSource>, kind: ImageFormat) -> Result<AudioReader> {
    ensure!(kind.is_audio(), "Not an audio format");
    let stream = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    hint.with_extension(kind.extension());
    let mut probed = symphonia::default::get_probe().format(
        &hint,
        stream,
        &FormatOptions {
            enable_gapless: true,
            ..Default::default()
        },
        &MetadataOptions::default(),
    )?;
    let tracks: Vec<_> = probed
        .format
        .tracks()
        .iter()
        .filter(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .collect();
    ensure!(tracks.len() == 1, "Exactly one audio track is required");
    let track = tracks[0];
    let params = track.codec_params.clone();
    let track_id = track.id;
    let matches_kind = match kind {
        ImageFormat::Mp3 => params.codec == CODEC_TYPE_MP3,
        ImageFormat::Flac => params.codec == CODEC_TYPE_FLAC,
        ImageFormat::M4a => matches!(params.codec, CODEC_TYPE_AAC | CODEC_TYPE_ALAC),
        ImageFormat::Ogg => params.codec == CODEC_TYPE_VORBIS,
        ImageFormat::Wav => true,
        _ => false,
    };
    ensure!(
        matches_kind,
        "The audio codec does not match the supported container"
    );
    let mut decoder =
        symphonia::default::get_codecs().make(&params, &DecoderOptions { verify: true })?;
    // Containers such as M4A may expose channel information only after decoding.
    // Retain this first packet so probing never removes samples from the output.
    let (spec, first_samples) = loop {
        let packet = probed.format.next_packet()?;
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet)?;
        if decoded.frames() == 0 {
            continue;
        }
        ensure!(decoded.capacity() <= 1_048_576, "Audio packet is too large");
        let spec = *decoded.spec();
        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        samples.copy_interleaved_ref(decoded);
        break (spec, samples.samples().to_vec());
    };
    let sample_rate = spec.rate;
    let channels = spec.channels.count() as u32;
    ensure!(
        (8_000..=192_000).contains(&sample_rate),
        "Supported audio sample rate: 8–192 kHz"
    );
    ensure!(
        (1..=2).contains(&channels),
        "Only mono and stereo audio are supported"
    );
    let expected_frames = params.n_frames.map(|frames| {
        params.time_base.map_or(frames, |base| {
            let time = base.calc_time(frames);
            time.seconds
                .saturating_mul(u64::from(sample_rate))
                .saturating_add((time.frac * f64::from(sample_rate)).round() as u64)
        })
    });
    let duration_ms =
        expected_frames.map(|frames| frames.saturating_mul(1000) / u64::from(sample_rate));
    ensure!(
        duration_ms.is_none_or(|ms| ms <= MAX_DURATION_SECONDS * 1000),
        "Audio exceeds the six-hour limit"
    );
    Ok(AudioReader {
        format: probed.format,
        decoder,
        track_id,
        info: AudioInfo {
            sample_rate,
            channels,
            duration_ms,
        },
        expected_frames,
        exact_length: matches!(kind, ImageFormat::Wav | ImageFormat::Flac),
        first_samples,
    })
}

pub fn inspect(path: &Path, kind: ImageFormat) -> Result<AudioInfo> {
    Ok(open(Box::new(File::open(path)?), kind)?.info)
}

impl AudioReader {
    fn decode_all(
        &mut self,
        cancelled: &AtomicBool,
        mut consume: impl FnMut(&[f32], usize) -> Result<()>,
    ) -> Result<u64> {
        ensure!(!cancelled.load(Ordering::Relaxed), "cancelled");
        ensure!(
            self.first_samples.iter().all(|sample| sample.is_finite()),
            "Audio contains invalid samples"
        );
        let mut frames = self.first_samples.len() as u64 / u64::from(self.info.channels);
        consume(&self.first_samples, self.info.channels as usize)?;
        loop {
            ensure!(!cancelled.load(Ordering::Relaxed), "cancelled");
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(DecodeError::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(error) => return Err(error.into()),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            let decoded = self.decoder.decode(&packet)?;
            let spec = *decoded.spec();
            ensure!(
                spec.rate == self.info.sample_rate
                    && spec.channels.count() as u32 == self.info.channels,
                "Audio parameters changed during decoding"
            );
            ensure!(decoded.capacity() <= 1_048_576, "Audio packet is too large");
            frames = frames
                .checked_add(decoded.frames() as u64)
                .context("Audio length overflow")?;
            ensure!(
                frames <= u64::from(spec.rate) * MAX_DURATION_SECONDS,
                "Audio exceeds the six-hour limit"
            );
            let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            samples.copy_interleaved_ref(decoded);
            ensure!(
                samples.samples().iter().all(|sample| sample.is_finite()),
                "Audio contains invalid samples"
            );
            consume(samples.samples(), spec.channels.count())?;
        }
        ensure!(frames > 0, "Empty audio stream");
        if let Some(expected) = self.expected_frames {
            let tolerance = if self.exact_length {
                0
            } else {
                u64::from(self.info.sample_rate / 5).max(4096)
            };
            ensure!(
                frames.abs_diff(expected) <= tolerance,
                "Audio is truncated or its declared duration is invalid"
            );
        }
        ensure!(
            self.decoder.finalize().verify_ok != Some(false),
            "Audio integrity check failed"
        );
        Ok(frames)
    }
}

/// Returns no candidate when it cannot be smaller. Input decoding still finishes
/// so a corrupt tail cannot be silently accepted as an unchanged file.
pub fn compress(
    source: Arc<[u8]>,
    kind: ImageFormat,
    bitrate: u32,
    cancelled: &AtomicBool,
) -> Result<Option<Vec<u8>>> {
    ensure!(!cancelled.load(Ordering::Relaxed), "cancelled");
    let brate = match bitrate {
        192 => Bitrate::Kbps192,
        128 => Bitrate::Kbps128,
        64 => Bitrate::Kbps64,
        _ => bail!("Unsupported MP3 bitrate"),
    };
    let mut reader = open(Box::new(Cursor::new(source.clone())), kind)?;
    let info = reader.info.clone();
    let mut encoder = Builder::new()
        .context("Could not initialize LAME")?
        .with_num_channels(info.channels as u8)?
        .with_sample_rate(info.sample_rate)?
        .with_brate(brate)?
        .with_quality(Quality::Best)?
        .with_to_write_vbr_tag(false)?
        .build()?;
    let mut candidate = Some(Vec::new());
    let source_frames = reader.decode_all(cancelled, |samples, channels| {
        if let Some(output) = &mut candidate {
            output.reserve(mp3lame_encoder::max_required_buffer_size(
                samples.len() / channels,
            ));
            if channels == 1 {
                encoder.encode_to_vec(MonoPcm(samples), output)?;
            } else {
                encoder.encode_to_vec(InterleavedPcm(samples), output)?;
            }
            if output.len() >= source.len() {
                candidate = None;
            }
        }
        Ok(())
    })?;
    let Some(mut output) = candidate else {
        return Ok(None);
    };
    output.reserve(7200);
    encoder.flush_to_vec::<FlushGap>(&mut output)?;
    if output.len() >= source.len() {
        return Ok(None);
    }
    ensure!(!cancelled.load(Ordering::Relaxed), "cancelled");
    // Decode the actual MP3 candidate and compare duration and channel count.
    let mut verification = open(Box::new(Cursor::new(output.clone())), ImageFormat::Mp3)?;
    let encoded_frames = verification.decode_all(cancelled, |_, _| Ok(()))?;
    ensure!(
        verification.info.channels == info.channels,
        "Encoded audio channel count changed"
    );
    let source_duration = source_frames as f64 / f64::from(info.sample_rate);
    let encoded_duration = encoded_frames as f64 / f64::from(verification.info.sample_rate);
    if (source_duration - encoded_duration).abs() > 0.25 {
        return Err(anyhow!("Encoded audio duration changed"));
    }
    Ok(Some(output))
}

#[cfg(test)]
#[path = "audio_tests.rs"]
mod tests;
