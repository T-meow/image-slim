use crate::metadata;
use crate::model::{CompressionPreset, ImageFormat, MetadataPolicy};
use anyhow::{Context, Result, anyhow};
use image::ImageFormat as DecoderFormat;
use imagequant::{ControlFlow, RGBA};
use oxipng::{Options, StripChunks};
use png::{BitDepth, ColorType, Encoder};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub fn compress(
    source: &[u8],
    preset: CompressionPreset,
    metadata_policy: MetadataPolicy,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    let encoded = match preset {
        CompressionPreset::Lossless => optimize_losslessly(source, metadata_policy)?,
        CompressionPreset::Balanced => {
            quantize_or_optimize(source, metadata_policy, 75, 92, 4, 0.8, cancelled)?
        }
        CompressionPreset::Strong => {
            quantize_or_optimize(source, metadata_policy, 55, 78, 1, 0.5, cancelled)?
        }
    };
    metadata::apply(source, &encoded, ImageFormat::Png, metadata_policy)
}

fn quantize_or_optimize(
    source: &[u8],
    metadata_policy: MetadataPolicy,
    minimum: u8,
    target: u8,
    speed: i32,
    dithering: f32,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    match quantize(source, minimum, target, speed, dithering, cancelled) {
        Ok(encoded) => Ok(encoded),
        Err(error) if error.to_string() == "cancelled" => Err(error),
        Err(_) => optimize_losslessly(source, metadata_policy),
    }
}

fn optimize_losslessly(source: &[u8], metadata_policy: MetadataPolicy) -> Result<Vec<u8>> {
    let mut options = Options::from_preset(4);
    options.optimize_alpha = false;
    options.strip = match metadata_policy {
        MetadataPolicy::Essential => StripChunks::Safe,
        MetadataPolicy::Supported => StripChunks::None,
    };
    options.max_decompressed_size = Some(800_000_000);
    oxipng::optimize_from_memory(source, &options).map_err(|error| anyhow!(error.to_string()))
}

fn quantize(
    source: &[u8],
    minimum: u8,
    target: u8,
    speed: i32,
    dithering: f32,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    let decoded = image::load_from_memory_with_format(source, DecoderFormat::Png)?.to_rgba8();
    let (width, height) = decoded.dimensions();
    let pixels = decoded
        .into_raw()
        .chunks_exact(4)
        .map(|pixel| RGBA {
            r: pixel[0],
            g: pixel[1],
            b: pixel[2],
            a: pixel[3],
        })
        .collect::<Vec<_>>();

    let mut attributes = imagequant::new();
    attributes.set_quality(minimum, target)?;
    attributes.set_speed(speed)?;
    let cancellation = cancelled.clone();
    attributes.set_progress_callback(move |_| {
        if cancellation.load(Ordering::Relaxed) {
            ControlFlow::Break
        } else {
            ControlFlow::Continue
        }
    });
    let mut image = attributes.new_image(pixels, width as usize, height as usize, 0.0)?;
    let mut result = attributes.quantize(&mut image)?;
    result.set_dithering_level(dithering)?;
    let (palette, indices) = result.remapped(&mut image)?;

    if cancelled.load(Ordering::Relaxed) {
        return Err(anyhow!("cancelled"));
    }

    let mut palette_rgb = Vec::with_capacity(palette.len() * 3);
    let mut transparency = Vec::with_capacity(palette.len());
    for color in palette {
        palette_rgb.extend_from_slice(&[color.r, color.g, color.b]);
        transparency.push(color.a);
    }

    let mut encoded = Vec::new();
    {
        let mut encoder = Encoder::new(&mut encoded, width, height);
        encoder.set_color(ColorType::Indexed);
        encoder.set_depth(BitDepth::Eight);
        encoder.set_palette(palette_rgb);
        if transparency.iter().any(|alpha| *alpha != 255) {
            encoder.set_trns(transparency);
        }
        let mut writer = encoder
            .write_header()
            .context("Failed to create indexed PNG header")?;
        writer
            .write_image_data(&indices)
            .context("Failed to write indexed PNG pixels")?;
    }

    let mut options = Options::from_preset(4);
    options.strip = StripChunks::All;
    options.optimize_alpha = false;
    oxipng::optimize_from_memory(&encoded, &options).map_err(|error| anyhow!(error.to_string()))
}
