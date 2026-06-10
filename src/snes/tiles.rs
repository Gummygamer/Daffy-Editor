//! SNES 4bpp planar tile decoding/encoding.
//!
//! A 4bpp tile is 32 bytes: rows 0-7 store bitplanes 0/1 interleaved in
//! bytes 0..16 (2 bytes per row), bitplanes 2/3 interleaved in bytes 16..32.
//! Pixel x of a row takes bit (7-x) from each plane.
//!
//! Confidence: confirmed (standard SNES PPU format, not game-specific).

pub const TILE_4BPP_BYTES: usize = 32;
pub const TILE_SIZE: usize = 8;

/// Decode one 4bpp tile into an 8x8 matrix of palette indices (0-15).
pub fn decode_4bpp_tile(bytes: &[u8; TILE_4BPP_BYTES]) -> [[u8; TILE_SIZE]; TILE_SIZE] {
    let mut out = [[0u8; TILE_SIZE]; TILE_SIZE];
    for (y, row) in out.iter_mut().enumerate() {
        let p0 = bytes[y * 2];
        let p1 = bytes[y * 2 + 1];
        let p2 = bytes[16 + y * 2];
        let p3 = bytes[16 + y * 2 + 1];
        for (x, px) in row.iter_mut().enumerate() {
            let bit = 7 - x;
            *px = ((p0 >> bit) & 1)
                | (((p1 >> bit) & 1) << 1)
                | (((p2 >> bit) & 1) << 2)
                | (((p3 >> bit) & 1) << 3);
        }
    }
    out
}

/// Encode an 8x8 matrix of palette indices (0-15) into a 4bpp tile.
pub fn encode_4bpp_tile(pixels: &[[u8; TILE_SIZE]; TILE_SIZE]) -> [u8; TILE_4BPP_BYTES] {
    let mut out = [0u8; TILE_4BPP_BYTES];
    for (y, row) in pixels.iter().enumerate() {
        for (x, &px) in row.iter().enumerate() {
            let bit = 7 - x;
            out[y * 2] |= (px & 1) << bit;
            out[y * 2 + 1] |= ((px >> 1) & 1) << bit;
            out[16 + y * 2] |= ((px >> 2) & 1) << bit;
            out[16 + y * 2 + 1] |= ((px >> 3) & 1) << bit;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_zero_tile_decodes_to_zero_pixels() {
        let px = decode_4bpp_tile(&[0u8; 32]);
        assert!(px.iter().flatten().all(|&p| p == 0));
    }

    #[test]
    fn hand_built_tile_decodes_exactly() {
        // Row 0: plane0 = 0b1000_0001, plane1 = 0b0000_0001,
        //        plane2 = 0b0000_0001, plane3 = 0b1000_0001
        // => pixel 0 = 0b1001 = 9, pixels 1-6 = 0, pixel 7 = 0b1111 = 15
        let mut bytes = [0u8; 32];
        bytes[0] = 0b1000_0001;
        bytes[1] = 0b0000_0001;
        bytes[16] = 0b0000_0001;
        bytes[17] = 0b1000_0001;
        let px = decode_4bpp_tile(&bytes);
        assert_eq!(px[0][0], 9);
        assert_eq!(px[0][7], 15);
        assert!(px[0][1..7].iter().all(|&p| p == 0));
        assert!(px[1..].iter().flatten().all(|&p| p == 0));
    }

    #[test]
    fn encode_decode_round_trip() {
        let mut pixels = [[0u8; 8]; 8];
        for y in 0..8 {
            for x in 0..8 {
                pixels[y][x] = ((y * 8 + x) % 16) as u8;
            }
        }
        assert_eq!(decode_4bpp_tile(&encode_4bpp_tile(&pixels)), pixels);
    }
}
