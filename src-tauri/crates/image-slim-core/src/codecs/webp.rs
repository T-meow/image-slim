use crate::metadata;
use crate::model::{CompressionPreset, ImageFormat, MetadataPolicy};
use anyhow::{Result, anyhow};
use webpx::{Encoder, Preset, Unstoppable};

pub fn compress(
    source: &[u8],
    preset: CompressionPreset,
    metadata_policy: MetadataPolicy,
) -> Result<Vec<u8>> {
    if preset == CompressionPreset::Lossless && !contains_lossless_bitstream(source) {
        return metadata::clean_webp_container(source, metadata_policy);
    }

    let (pixels, width, height) =
        webpx::decode_rgba(source).map_err(|error| anyhow!(error.to_string()))?;
    let encoder = Encoder::new_rgba(&pixels, width, height)
        .preset(Preset::Picture)
        .method(6);
    let encoded = match preset {
        CompressionPreset::Lossless => encoder
            .lossless(true)
            .quality(100.0)
            .exact(true)
            .encode(Unstoppable),
        CompressionPreset::Balanced => encoder
            .quality(80.0)
            .alpha_quality(90)
            .sharp_yuv(true)
            .encode(Unstoppable),
        CompressionPreset::Strong => encoder
            .quality(65.0)
            .alpha_quality(75)
            .sharp_yuv(true)
            .encode(Unstoppable),
    }
    .map_err(|error| anyhow!(error.to_string()))?;
    metadata::apply(source, &encoded, ImageFormat::Webp, metadata_policy)
}

fn contains_lossless_bitstream(data: &[u8]) -> bool {
    data.windows(4).any(|window| window == b"VP8L")
}
