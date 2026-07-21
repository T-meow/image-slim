mod jpeg;
mod jpeg_lossless;
mod png_codec;
mod webp;

use crate::metadata;
use crate::model::{CompressionPreset, ImageFormat, MetadataPolicy};
use anyhow::{Result, anyhow};
use image::GenericImageView;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub fn compress(
    source: &[u8],
    format: ImageFormat,
    expected_dimensions: (u32, u32),
    preset: CompressionPreset,
    metadata_policy: MetadataPolicy,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(anyhow!("cancelled"));
    }

    let encoded = match format {
        ImageFormat::Png => {
            png_codec::compress(source, preset, metadata_policy, cancelled.clone())?
        }
        ImageFormat::Jpeg => jpeg::compress(source, preset, metadata_policy)?,
        ImageFormat::Webp => webp::compress(source, preset, metadata_policy)?,
    };
    metadata::verify_supported_container(&encoded, format)?;
    if preset == CompressionPreset::Lossless {
        validate_lossless_pixels(source, &encoded, format, expected_dimensions)?;
    } else {
        validate_encoded_dimensions(&encoded, format, expected_dimensions)?;
    }
    Ok(encoded)
}

fn validate_encoded_dimensions(
    encoded: &[u8],
    format: ImageFormat,
    expected_dimensions: (u32, u32),
) -> Result<()> {
    if decoded_dimensions(encoded, format)? != expected_dimensions {
        return Err(anyhow!("Encoded image dimensions changed"));
    }
    Ok(())
}

fn decoded_dimensions(data: &[u8], format: ImageFormat) -> Result<(u32, u32)> {
    if format == ImageFormat::Webp {
        let (_, width, height) =
            webpx::decode_rgba(data).map_err(|error| anyhow!(error.to_string()))?;
        return Ok((width, height));
    }
    Ok(image::load_from_memory(data)?.dimensions())
}

fn validate_lossless_pixels(
    source: &[u8],
    encoded: &[u8],
    format: ImageFormat,
    expected_dimensions: (u32, u32),
) -> Result<()> {
    if format == ImageFormat::Webp {
        let (before, before_width, before_height) =
            webpx::decode_rgba(source).map_err(|error| anyhow!(error.to_string()))?;
        let (after, after_width, after_height) =
            webpx::decode_rgba(encoded).map_err(|error| anyhow!(error.to_string()))?;
        if (before_width, before_height) != expected_dimensions
            || (after_width, after_height) != expected_dimensions
            || before != after
        {
            return Err(anyhow!("Lossless verification found changed pixel values"));
        }
        return Ok(());
    }

    let before = image::load_from_memory(source)?;
    let after = image::load_from_memory(encoded)?;
    if before.dimensions() != expected_dimensions || after.dimensions() != expected_dimensions {
        return Err(anyhow!("Encoded image dimensions changed"));
    }
    let identical = match format {
        ImageFormat::Png => before.to_rgba16().into_raw() == after.to_rgba16().into_raw(),
        ImageFormat::Jpeg => before.to_rgba8().into_raw() == after.to_rgba8().into_raw(),
        ImageFormat::Webp => unreachable!("WebP is verified through libwebp above"),
    };
    if !identical {
        return Err(anyhow!("Lossless verification found changed pixel values"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::sync::atomic::AtomicBool;

    fn pixels() -> ImageBuffer<Rgba<u8>, Vec<u8>> {
        ImageBuffer::from_fn(96, 72, |x, y| {
            let alpha = if (x + y) % 11 == 0 { 0 } else { 255 };
            Rgba([
                ((x * 7 + y * 3) % 256) as u8,
                ((x * 2 + y * 9) % 256) as u8,
                ((x * 5 + y * 5) % 256) as u8,
                alpha,
            ])
        })
    }

    fn png_source() -> Vec<u8> {
        let image = pixels();
        let mut output = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut output, image.width(), image.height());
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(image.as_raw()).unwrap();
        }
        output
    }

    fn jpeg_source() -> Vec<u8> {
        let image = pixels();
        let rgb = image::DynamicImage::ImageRgba8(image).to_rgb8();
        let mut output = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 95);
        encoder
            .encode(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        output
    }

    fn webp_source() -> Vec<u8> {
        let image = pixels();
        webpx::Encoder::new_rgba(image.as_raw(), image.width(), image.height())
            .lossless(true)
            .method(4)
            .exact(true)
            .encode(webpx::Unstoppable)
            .unwrap()
    }

    fn run_all_presets(source: &[u8], format: ImageFormat) {
        let mut sizes = Vec::new();
        for preset in [
            CompressionPreset::Lossless,
            CompressionPreset::Balanced,
            CompressionPreset::Strong,
        ] {
            let result = compress(
                source,
                format,
                decoded_dimensions(source, format).unwrap(),
                preset,
                MetadataPolicy::Essential,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap_or_else(|error| panic!("{format:?} {preset:?}: {error:#}"));
            assert_eq!(crate::scanner::format_from_magic(&result), Some(format));
            assert_eq!(
                decoded_dimensions(source, format).unwrap(),
                decoded_dimensions(&result, format).unwrap()
            );
            sizes.push(result.len());
        }
        if format != ImageFormat::Webp {
            assert!(
                sizes[1] <= sizes[0],
                "{format:?}: balanced was larger than lossless"
            );
        }
        assert!(
            sizes[2] <= sizes[1],
            "{format:?}: strong was larger than balanced"
        );
    }

    #[test]
    fn png_presets_produce_valid_images() {
        run_all_presets(&png_source(), ImageFormat::Png);
    }

    #[test]
    fn jpeg_presets_produce_valid_images() {
        run_all_presets(&jpeg_source(), ImageFormat::Jpeg);
    }

    #[test]
    fn webp_presets_produce_valid_images() {
        run_all_presets(&webp_source(), ImageFormat::Webp);
    }

    #[test]
    fn lossless_png_preserves_sixteen_bit_samples() {
        let mut source = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut source, 8, 8);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Sixteen);
            let mut writer = encoder.write_header().unwrap();
            let mut samples = Vec::with_capacity(8 * 8 * 8);
            for index in 0..(8 * 8 * 4) {
                samples.extend_from_slice(&((index * 257) as u16).to_be_bytes());
            }
            writer.write_image_data(&samples).unwrap();
        }
        compress(
            &source,
            ImageFormat::Png,
            (8, 8),
            CompressionPreset::Lossless,
            MetadataPolicy::Essential,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    }

    #[test]
    fn cancelled_compression_does_not_start() {
        let error = compress(
            &png_source(),
            ImageFormat::Png,
            (96, 72),
            CompressionPreset::Balanced,
            MetadataPolicy::Essential,
            Arc::new(AtomicBool::new(true)),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "cancelled");
    }

    #[test]
    #[ignore = "manual release performance baseline"]
    fn records_twelve_and_forty_eight_megapixel_baselines() {
        use std::time::Instant;

        for (label, width, height) in [("12MP", 4_000, 3_000), ("48MP", 8_000, 6_000)] {
            let pixels = ImageBuffer::from_fn(width, height, |x, y| {
                Rgba([
                    ((x * 3 + y) % 256) as u8,
                    ((y * 2 + x / 7) % 256) as u8,
                    ((x / 3 + y / 5) % 256) as u8,
                    255,
                ])
            });
            let mut png = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut png, width, height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header().unwrap();
                writer.write_image_data(pixels.as_raw()).unwrap();
            }
            let rgb = image::DynamicImage::ImageRgba8(pixels.clone()).to_rgb8();
            let mut jpeg = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 92)
                .encode(rgb.as_raw(), width, height, image::ExtendedColorType::Rgb8)
                .unwrap();
            let webp = webpx::Encoder::new_rgba(pixels.as_raw(), width, height)
                .quality(92.0)
                .method(4)
                .encode(webpx::Unstoppable)
                .unwrap();
            drop(rgb);
            drop(pixels);

            for (format, source) in [
                (ImageFormat::Png, png.as_slice()),
                (ImageFormat::Jpeg, jpeg.as_slice()),
                (ImageFormat::Webp, webp.as_slice()),
            ] {
                for preset in [
                    CompressionPreset::Lossless,
                    CompressionPreset::Balanced,
                    CompressionPreset::Strong,
                ] {
                    let started = Instant::now();
                    let result = compress(
                        source,
                        format,
                        (width, height),
                        preset,
                        MetadataPolicy::Essential,
                        Arc::new(AtomicBool::new(false)),
                    )
                    .unwrap();
                    eprintln!(
                        "BASELINE,{label},{format:?},{preset:?},{},{},{}",
                        source.len(),
                        result.len(),
                        started.elapsed().as_millis()
                    );
                }
            }
        }
    }
}
