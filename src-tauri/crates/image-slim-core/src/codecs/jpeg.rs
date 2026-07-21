use super::jpeg_lossless;
use crate::metadata;
use crate::model::{CompressionPreset, ImageFormat, MetadataPolicy};
use anyhow::{Result, anyhow};
use image::{ColorType, ImageFormat as DecoderFormat};
use mozjpeg::{ColorSpace, Compress};
use std::panic::{AssertUnwindSafe, catch_unwind};

pub fn compress(
    source: &[u8],
    preset: CompressionPreset,
    metadata_policy: MetadataPolicy,
) -> Result<Vec<u8>> {
    let encoded = match preset {
        CompressionPreset::Lossless => {
            jpeg_lossless::optimize(source, metadata_policy == MetadataPolicy::Essential)?
        }
        CompressionPreset::Balanced => encode_lossy(source, 82.0)?,
        CompressionPreset::Strong => encode_lossy(source, 68.0)?,
    };
    if preset == CompressionPreset::Lossless && metadata_policy == MetadataPolicy::Supported {
        return Ok(encoded);
    }
    metadata::apply(source, &encoded, ImageFormat::Jpeg, metadata_policy)
}

fn encode_lossy(source: &[u8], quality: f32) -> Result<Vec<u8>> {
    let decoded = image::load_from_memory_with_format(source, DecoderFormat::Jpeg)?;
    let width = decoded.width() as usize;
    let height = decoded.height() as usize;
    let grayscale = matches!(decoded.color(), ColorType::L8 | ColorType::L16);

    let encoded = catch_unwind(AssertUnwindSafe(|| -> std::io::Result<Vec<u8>> {
        let mut compressor = Compress::new(if grayscale {
            ColorSpace::JCS_GRAYSCALE
        } else {
            ColorSpace::JCS_RGB
        });
        compressor.set_size(width, height);
        compressor.set_quality(quality);
        compressor.set_progressive_mode();
        compressor.set_optimize_scans(true);
        compressor.set_optimize_coding(true);
        compressor.set_use_scans_in_trellis(true);
        if !grayscale {
            compressor.set_chroma_sampling_pixel_sizes((2, 2), (2, 2));
        }
        let pixels = if grayscale {
            decoded.to_luma8().into_raw()
        } else {
            decoded.to_rgb8().into_raw()
        };
        let mut started = compressor.start_compress(Vec::new())?;
        started.write_scanlines(&pixels)?;
        started.finish()
    }))
    .map_err(|_| anyhow!("MozJPEG aborted while encoding this file"))??;
    Ok(encoded)
}
