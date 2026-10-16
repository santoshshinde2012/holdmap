//! PNG validation shared by native snapshot reads and the explicit save boundary.

pub(crate) const MAX_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_WIDTH: u32 = 3840;
pub(crate) const MAX_HEIGHT: u32 = 2160;

pub(crate) fn validate(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() < 45
        || bytes.len() > MAX_BYTES
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes[8..12] != 13u32.to_be_bytes()
        || &bytes[12..16] != b"IHDR"
    {
        return Err("The snapshot must be a complete PNG smaller than 32 MiB.".into());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("bounded PNG width"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("bounded PNG height"));
    // Inspect dimensions before asking the decoder to allocate pixels.
    if width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err("The snapshot exceeds the 3840 by 2160 pixel limit.".into());
    }
    let image = tauri::image::Image::from_bytes(bytes)
        .map_err(|_| "The snapshot PNG is incomplete or invalid.".to_owned())?;
    if image.width() != width || image.height() != height {
        return Err("The snapshot PNG dimensions are inconsistent.".into());
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    const PNG: &[u8] = include_bytes!("../icons/32x32.png");

    #[test]
    fn validates_real_png_and_rejects_truncation_corruption_and_dimension_bombs() {
        assert_eq!(validate(PNG).unwrap(), (32, 32));
        for bytes in [&PNG[..24], &PNG[..45], b"GIF89a".as_slice()] {
            assert!(validate(bytes).is_err());
        }
        let mut oversized = PNG.to_vec();
        oversized[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(validate(&oversized).unwrap_err().contains("pixel limit"));
        let mut corrupt = PNG.to_vec();
        corrupt[35] ^= 0xff;
        assert!(validate(&corrupt).is_err());
    }
}
