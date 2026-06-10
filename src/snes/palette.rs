//! SNES CGRAM color handling (15-bit BGR555).
//!
//! Bit layout: 0BBBBBGG GGGRRRRR. Bit 15 is unused and normally 0 in CGRAM
//! data stored in ROM (a useful heuristic when scanning for palettes).
//!
//! Confidence: confirmed (standard SNES PPU format, not game-specific).

/// Convert a BGR555 color to RGB8 using the common (v << 3) | (v >> 2) expansion.
pub fn bgr555_to_rgb8(color: u16) -> [u8; 3] {
    let expand = |v: u16| -> u8 { ((v << 3) | (v >> 2)) as u8 };
    [
        expand(color & 0x1F),
        expand((color >> 5) & 0x1F),
        expand((color >> 10) & 0x1F),
    ]
}

/// Convert to RGBA8 with full alpha (color index 0 transparency is the
/// renderer's decision, not encoded here).
pub fn bgr555_to_rgba8(color: u16) -> [u8; 4] {
    let [r, g, b] = bgr555_to_rgb8(color);
    [r, g, b, 0xFF]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_colors_convert_exactly() {
        assert_eq!(bgr555_to_rgb8(0x0000), [0, 0, 0]); // black
        assert_eq!(bgr555_to_rgb8(0x7FFF), [255, 255, 255]); // white
        assert_eq!(bgr555_to_rgb8(0x001F), [255, 0, 0]); // pure red
        assert_eq!(bgr555_to_rgb8(0x03E0), [0, 255, 0]); // pure green
        assert_eq!(bgr555_to_rgb8(0x7C00), [0, 0, 255]); // pure blue
    }

    #[test]
    fn five_to_eight_bit_scaling_is_monotonic_and_full_range() {
        let mut prev = -1i32;
        for v in 0u16..32 {
            let [r, _, _] = bgr555_to_rgb8(v);
            assert!(r as i32 > prev);
            prev = r as i32;
        }
        assert_eq!(prev, 255);
    }
}
