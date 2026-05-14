// Millennium Clipboard — image thumbnails (Fase 9, v0.9.0)
//
// Before a file gets accepted, the sender may include a tiny base64
// JPEG thumbnail (~64×64, q60) in the prepare-upload payload so the
// receiver can preview images in the accept/reject modal. Skipped for
// non-images and oversize files.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use image::imageops::FilterType;
use image::ImageFormat;
use std::io::Cursor;
use std::path::Path;

/// Max file size we'll bother thumbnailing (don't decode a 1GB tiff).
const MAX_THUMBNAIL_INPUT_BYTES: u64 = 50 * 1024 * 1024;

/// Final dimensions of the longest edge after fit-in-box resize.
const THUMB_LONGEST_EDGE: u32 = 96;

/// JPEG quality factor used to keep the encoded base64 string under ~10KB.
const THUMB_JPEG_QUALITY: u8 = 65;

/// Generate a base64-encoded JPEG thumbnail data URL for `path`, or
/// `Ok(None)` if the file is not a supported image, is too big, or any
/// decode step fails (we want this to be best-effort; a missing
/// thumbnail must not break the transfer).
pub fn generate_for(path: &Path, file_size: u64) -> Result<Option<String>> {
    if file_size == 0 || file_size > MAX_THUMBNAIL_INPUT_BYTES {
        return Ok(None);
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let supported = matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp"
    );
    if !supported {
        return Ok(None);
    }

    let bytes = std::fs::read(path)
        .with_context(|| format!("read image {}", path.display()))?;
    let img = match image::load_from_memory(&bytes) {
        Ok(i) => i,
        Err(_) => return Ok(None),
    };

    let thumb = img.thumbnail(THUMB_LONGEST_EDGE, THUMB_LONGEST_EDGE);
    // Convert to RGB8 so JPEG can encode it regardless of source alpha.
    let rgb = thumb.to_rgb8();

    let mut out: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut cursor = Cursor::new(&mut out);
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, THUMB_JPEG_QUALITY);
    let _ = encoder; // silence unused warning for future expansion
    {
        let mut enc =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, THUMB_JPEG_QUALITY);
        enc.encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )?;
    }
    drop(cursor);

    let _ = ImageFormat::Jpeg; // keep dep alive for `FilterType`
    let _ = FilterType::Triangle;

    let data_url = format!("data:image/jpeg;base64,{}", B64.encode(&out));
    Ok(Some(data_url))
}
