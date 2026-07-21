use crate::model::{ImageFormat, MetadataPolicy};
use anyhow::{Result, anyhow};
use img_parts::jpeg::Jpeg;
use img_parts::png::Png;
use img_parts::{Bytes, ImageEXIF, ImageICC};

pub fn apply(
    original: &[u8],
    encoded: &[u8],
    format: ImageFormat,
    policy: MetadataPolicy,
) -> Result<Vec<u8>> {
    match format {
        ImageFormat::Png => apply_png(original, encoded, policy),
        ImageFormat::Jpeg => apply_jpeg(original, encoded, policy),
        ImageFormat::Webp => apply_webp(original, encoded, policy),
    }
}

pub fn clean_webp_container(original: &[u8], policy: MetadataPolicy) -> Result<Vec<u8>> {
    if policy == MetadataPolicy::Supported {
        return Ok(original.to_vec());
    }

    let icc = webpx::get_icc_profile(original).map_err(|error| anyhow!(error.to_string()))?;
    let exif = webpx::get_exif(original)
        .map_err(|error| anyhow!(error.to_string()))?
        .and_then(|value| essential_exif(Some(Bytes::from(value))));
    let mut cleaned = webpx::remove_xmp(original)
        .and_then(|value| webpx::remove_exif(&value))
        .and_then(|value| webpx::remove_icc(&value))
        .map_err(|error| anyhow!(error.to_string()))?;
    if let Some(profile) = icc {
        cleaned =
            webpx::embed_icc(&cleaned, &profile).map_err(|error| anyhow!(error.to_string()))?;
    }
    if let Some(exif) = exif {
        cleaned = webpx::embed_exif(&cleaned, &exif).map_err(|error| anyhow!(error.to_string()))?;
    }
    Ok(cleaned)
}

fn apply_jpeg(original: &[u8], encoded: &[u8], policy: MetadataPolicy) -> Result<Vec<u8>> {
    let source = Jpeg::from_bytes(Bytes::copy_from_slice(original))?;
    let mut target = Jpeg::from_bytes(Bytes::copy_from_slice(encoded))?;
    target.set_icc_profile(source.icc_profile());
    target.set_exif(match policy {
        MetadataPolicy::Supported => source.exif(),
        MetadataPolicy::Essential => essential_exif(source.exif()),
    });
    Ok(target.encoder().bytes().to_vec())
}

fn apply_png(original: &[u8], encoded: &[u8], policy: MetadataPolicy) -> Result<Vec<u8>> {
    let source = Png::from_bytes(Bytes::copy_from_slice(original))?;
    let mut target = Png::from_bytes(Bytes::copy_from_slice(encoded))?;

    let preserved_types: &[[u8; 4]] = match policy {
        MetadataPolicy::Essential => &[gama(), chrm(), srgb(), iccp()],
        MetadataPolicy::Supported => &[
            gama(),
            chrm(),
            srgb(),
            iccp(),
            phys(),
            text(),
            ztxt(),
            itxt(),
            time(),
        ],
    };
    let preserved = source
        .chunks()
        .iter()
        .filter(|chunk| preserved_types.contains(&chunk.kind()))
        .cloned()
        .collect::<Vec<_>>();
    target
        .chunks_mut()
        .retain(|chunk| !preserved_types.contains(&chunk.kind()));
    let insert_at = target
        .chunks()
        .iter()
        .position(|chunk| matches!(&chunk.kind(), b"PLTE" | b"IDAT"))
        .unwrap_or(1);
    target.chunks_mut().splice(insert_at..insert_at, preserved);

    target.set_exif(match policy {
        MetadataPolicy::Supported => source.exif(),
        MetadataPolicy::Essential => essential_exif(source.exif()),
    });
    Ok(target.encoder().bytes().to_vec())
}

fn apply_webp(original: &[u8], encoded: &[u8], policy: MetadataPolicy) -> Result<Vec<u8>> {
    let mut target = encoded.to_vec();
    if let Some(icc) =
        webpx::get_icc_profile(original).map_err(|error| anyhow!(error.to_string()))?
    {
        target = webpx::embed_icc(&target, &icc).map_err(|error| anyhow!(error.to_string()))?;
    }

    let source_exif = webpx::get_exif(original)
        .map_err(|error| anyhow!(error.to_string()))?
        .map(Bytes::from);
    let exif = match policy {
        MetadataPolicy::Supported => source_exif,
        MetadataPolicy::Essential => essential_exif(source_exif),
    };
    if let Some(exif) = exif {
        target = webpx::embed_exif(&target, &exif).map_err(|error| anyhow!(error.to_string()))?;
    }

    if policy == MetadataPolicy::Supported
        && let Some(xmp) = webpx::get_xmp(original).map_err(|error| anyhow!(error.to_string()))?
    {
        target = webpx::embed_xmp(&target, &xmp).map_err(|error| anyhow!(error.to_string()))?;
    }
    Ok(target)
}

fn essential_exif(exif: Option<Bytes>) -> Option<Bytes> {
    let orientation = read_orientation(exif.as_deref()?)?;
    Some(Bytes::from(minimal_orientation_exif(orientation)))
}

fn read_orientation(exif: &[u8]) -> Option<u16> {
    let data = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
    if data.len() < 8 {
        return None;
    }
    let little = match &data[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read_u16 = |bytes: &[u8]| -> Option<u16> {
        let value: [u8; 2] = bytes.get(..2)?.try_into().ok()?;
        Some(if little {
            u16::from_le_bytes(value)
        } else {
            u16::from_be_bytes(value)
        })
    };
    let read_u32 = |bytes: &[u8]| -> Option<u32> {
        let value: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
        Some(if little {
            u32::from_le_bytes(value)
        } else {
            u32::from_be_bytes(value)
        })
    };
    if read_u16(&data[2..])? != 42 {
        return None;
    }
    let ifd_offset = usize::try_from(read_u32(&data[4..])?).ok()?;
    let entry_count = usize::from(read_u16(data.get(ifd_offset..)?)?);
    for index in 0..entry_count {
        let offset = ifd_offset.checked_add(2 + index * 12)?;
        let entry = data.get(offset..offset + 12)?;
        if read_u16(entry)? == 0x0112 && read_u16(&entry[2..])? == 3 && read_u32(&entry[4..])? == 1
        {
            let value = read_u16(&entry[8..])?;
            return (1..=8).contains(&value).then_some(value);
        }
    }
    None
}

fn minimal_orientation_exif(orientation: u16) -> Vec<u8> {
    let mut exif = Vec::with_capacity(26);
    exif.extend_from_slice(b"MM");
    exif.extend_from_slice(&42u16.to_be_bytes());
    exif.extend_from_slice(&8u32.to_be_bytes());
    exif.extend_from_slice(&1u16.to_be_bytes());
    exif.extend_from_slice(&0x0112u16.to_be_bytes());
    exif.extend_from_slice(&3u16.to_be_bytes());
    exif.extend_from_slice(&1u32.to_be_bytes());
    exif.extend_from_slice(&orientation.to_be_bytes());
    exif.extend_from_slice(&0u16.to_be_bytes());
    exif.extend_from_slice(&0u32.to_be_bytes());
    exif
}

const fn gama() -> [u8; 4] {
    *b"gAMA"
}
const fn chrm() -> [u8; 4] {
    *b"cHRM"
}
const fn srgb() -> [u8; 4] {
    *b"sRGB"
}
const fn iccp() -> [u8; 4] {
    *b"iCCP"
}
const fn phys() -> [u8; 4] {
    *b"pHYs"
}
const fn text() -> [u8; 4] {
    *b"tEXt"
}
const fn ztxt() -> [u8; 4] {
    *b"zTXt"
}
const fn itxt() -> [u8; 4] {
    *b"iTXt"
}
const fn time() -> [u8; 4] {
    *b"tIME"
}

pub fn verify_supported_container(data: &[u8], expected: ImageFormat) -> Result<()> {
    let actual = crate::scanner::format_from_magic(data)
        .ok_or_else(|| anyhow!("Encoder returned an unsupported image container"))?;
    if actual != expected {
        return Err(anyhow!("Encoder returned a different image format"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use webpx::{Encoder, Unstoppable};

    #[test]
    fn reads_and_rebuilds_orientation() {
        let bytes = minimal_orientation_exif(6);
        assert_eq!(read_orientation(&bytes), Some(6));
        assert_eq!(bytes.len(), 26);
    }

    #[test]
    fn ignores_invalid_orientation() {
        assert_eq!(read_orientation(b"not-exif"), None);
    }

    #[test]
    fn webp_essential_policy_keeps_display_metadata_and_removes_xmp() {
        let base = Encoder::new_rgba(&[10, 20, 30, 128], 1, 1)
            .lossless(true)
            .exact(true)
            .encode(Unstoppable)
            .unwrap();
        let with_icc = webpx::embed_icc(&base, b"test-icc").unwrap();
        let with_exif = webpx::embed_exif(&with_icc, &minimal_orientation_exif(6)).unwrap();
        let source = webpx::embed_xmp(&with_exif, b"camera notes").unwrap();

        let cleaned = clean_webp_container(&source, MetadataPolicy::Essential).unwrap();
        webpx::decode_rgba(&cleaned).unwrap();
        assert_eq!(
            webpx::get_icc_profile(&cleaned).unwrap().unwrap(),
            b"test-icc"
        );
        assert_eq!(
            read_orientation(&webpx::get_exif(&cleaned).unwrap().unwrap()),
            Some(6)
        );
        assert!(webpx::get_xmp(&cleaned).unwrap().is_none());
    }
}
