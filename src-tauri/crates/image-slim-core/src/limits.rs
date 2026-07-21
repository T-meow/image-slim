use crate::error::{AppError, AppResult, ErrorCode};
use crate::model::{
    AppCapabilities, CompressionPreset, FormatCapability, ImageFormat, InputItem, InputLimits,
};
use std::fs;
use std::path::Path;

pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 100_000_000;
pub const MAX_DIMENSION: u32 = 65_535;
pub const MAX_QUEUE_ITEMS: usize = 10_000;
pub const PREVIEW_MAX_EDGE: u32 = 2_048;
pub const PREVIEW_CACHE_MAX_BYTES: u64 = 1024 * 1024 * 1024;
pub const PREVIEW_CACHE_MAX_ITEMS: usize = 4;

pub fn capabilities() -> AppCapabilities {
    AppCapabilities {
        formats: vec![
            FormatCapability {
                format: ImageFormat::Png,
                extensions: vec!["png".into()],
            },
            FormatCapability {
                format: ImageFormat::Jpeg,
                extensions: vec!["jpg".into(), "jpeg".into()],
            },
            FormatCapability {
                format: ImageFormat::Webp,
                extensions: vec!["webp".into()],
            },
        ],
        presets: vec![
            CompressionPreset::Lossless,
            CompressionPreset::Balanced,
            CompressionPreset::Strong,
        ],
        limits: InputLimits {
            max_file_bytes: MAX_FILE_BYTES,
            max_pixels: MAX_PIXELS,
            max_dimension: MAX_DIMENSION,
            max_queue_items: MAX_QUEUE_ITEMS,
        },
    }
}

pub fn validate_file_size(path: &Path, size: u64) -> AppResult<()> {
    if size > MAX_FILE_BYTES {
        return Err(AppError::new(ErrorCode::FileTooLarge)
            .path(path)
            .param("actual", size)
            .param("limit", MAX_FILE_BYTES));
    }
    Ok(())
}

pub fn validate_dimensions(path: &Path, width: u32, height: u32) -> AppResult<()> {
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(AppError::new(ErrorCode::DimensionLimitExceeded)
            .path(path)
            .param("width", width)
            .param("height", height)
            .param("limit", MAX_DIMENSION));
    }
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if pixels > MAX_PIXELS {
        return Err(AppError::new(ErrorCode::PixelLimitExceeded)
            .path(path)
            .param("actual", pixels)
            .param("limit", MAX_PIXELS));
    }
    Ok(())
}

pub fn validate_queue_size(count: usize) -> AppResult<()> {
    if count > MAX_QUEUE_ITEMS {
        return Err(AppError::new(ErrorCode::QueueLimitReached)
            .param("actual", count)
            .param("limit", MAX_QUEUE_ITEMS));
    }
    Ok(())
}

pub fn validate_item(item: &InputItem) -> AppResult<()> {
    let path = Path::new(&item.source_path);
    let metadata = fs::metadata(path).map_err(|error| AppError::io(error, path))?;
    validate_file_size(path, metadata.len())?;
    validate_dimensions(path, item.width, item.height)
}

pub fn estimated_peak_bytes(item: &InputItem) -> u64 {
    let pixels = u64::from(item.width).saturating_mul(u64::from(item.height));
    let decoded_bytes_per_pixel = match item.format {
        ImageFormat::Png => 8,
        ImageFormat::Jpeg | ImageFormat::Webp => 4,
    };
    pixels
        .saturating_mul(decoded_bytes_per_pixel)
        .saturating_mul(4)
        .saturating_add(item.original_size.saturating_mul(2))
        .saturating_add(128 * 1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_rejects_dimension_boundaries() {
        assert!(validate_dimensions(Path::new("a.png"), 10_000, 10_000).is_ok());
        assert!(validate_dimensions(Path::new("a.png"), 10_001, 10_000).is_err());
        assert!(validate_dimensions(Path::new("a.png"), MAX_DIMENSION, 1).is_ok());
        assert!(validate_dimensions(Path::new("a.png"), MAX_DIMENSION + 1, 1).is_err());
    }

    #[test]
    fn accepts_and_rejects_file_size_boundaries() {
        assert!(validate_file_size(Path::new("a.png"), MAX_FILE_BYTES).is_ok());
        assert!(validate_file_size(Path::new("a.png"), MAX_FILE_BYTES + 1).is_err());
    }

    #[test]
    fn accepts_and_rejects_queue_boundaries() {
        assert!(validate_queue_size(MAX_QUEUE_ITEMS).is_ok());
        assert!(validate_queue_size(MAX_QUEUE_ITEMS + 1).is_err());
    }
}
