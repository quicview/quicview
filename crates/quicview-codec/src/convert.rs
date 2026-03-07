use crate::error::CodecError;

/// Convert a BGRA buffer to RGBA in place.
pub fn bgra_to_rgba(buf: &mut [u8]) -> Result<(), CodecError> {
    if !buf.len().is_multiple_of(4) {
        return Err(CodecError::BufferMismatch {
            expected: (buf.len() / 4) * 4,
            actual: buf.len(),
        });
    }
    for pixel in buf.chunks_exact_mut(4) {
        pixel.swap(0, 2); // B ↔ R
    }
    Ok(())
}

/// Convert an RGBA buffer to RGB (dropping the alpha channel).
pub fn rgba_to_rgb(rgba: &[u8]) -> Result<Vec<u8>, CodecError> {
    if !rgba.len().is_multiple_of(4) {
        return Err(CodecError::BufferMismatch {
            expected: (rgba.len() / 4) * 4,
            actual: rgba.len(),
        });
    }
    let pixel_count = rgba.len() / 4;
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    for pixel in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
    }
    Ok(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_to_rgba_swaps_channels() {
        let mut buf = vec![10, 20, 30, 255]; // B=10, G=20, R=30, A=255
        bgra_to_rgba(&mut buf).unwrap();
        assert_eq!(buf, [30, 20, 10, 255]); // R=30, G=20, B=10, A=255
    }

    #[test]
    fn rgba_to_rgb_drops_alpha() {
        let rgba = vec![10, 20, 30, 255, 40, 50, 60, 128];
        let rgb = rgba_to_rgb(&rgba).unwrap();
        assert_eq!(rgb, vec![10, 20, 30, 40, 50, 60]);
    }

    #[test]
    fn misaligned_buffer_errors() {
        let mut buf = vec![1, 2, 3]; // not a multiple of 4
        assert!(bgra_to_rgba(&mut buf).is_err());
        assert!(rgba_to_rgb(&buf).is_err());
    }
}
