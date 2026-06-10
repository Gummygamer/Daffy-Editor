//! Deterministic CPU-side rendering of tiles, tilesets and rooms into RGBA
//! buffers. The GUI uploads these as egui textures; tests assert on the bytes.

use crate::error::EditError;
use crate::model::level::{Level, Metatile, Palette};
use crate::snes::palette::bgr555_to_rgba8;
use crate::snes::tiles::{decode_4bpp_tile, TILE_4BPP_BYTES, TILE_SIZE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: usize,
    pub height: usize,
    /// width * height * 4 bytes, row-major RGBA.
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, pixels: vec![0; width * height * 4] }
    }

    pub fn put(&mut self, x: usize, y: usize, rgba: [u8; 4]) {
        if x < self.width && y < self.height {
            let i = (y * self.width + x) * 4;
            self.pixels[i..i + 4].copy_from_slice(&rgba);
        }
    }
}

/// Render raw 4bpp tile data as a tileset sheet using BGR555 `palette`
/// (first 16 entries are used; missing entries render magenta).
pub fn render_tileset_rgba(tile_data: &[u8], palette: &[u16], tiles_per_row: usize) -> RgbaImage {
    let tile_count = tile_data.len() / TILE_4BPP_BYTES;
    let tiles_per_row = tiles_per_row.max(1);
    let rows = tile_count.div_ceil(tiles_per_row).max(1);
    let mut img = RgbaImage::new(tiles_per_row * TILE_SIZE, rows * TILE_SIZE);

    for t in 0..tile_count {
        let bytes: &[u8; TILE_4BPP_BYTES] =
            tile_data[t * TILE_4BPP_BYTES..(t + 1) * TILE_4BPP_BYTES].try_into().expect("32 bytes");
        let pixels = decode_4bpp_tile(bytes);
        let (tx, ty) = (t % tiles_per_row, t / tiles_per_row);
        for (y, row) in pixels.iter().enumerate() {
            for (x, &px) in row.iter().enumerate() {
                let rgba = palette
                    .get(px as usize)
                    .map(|&c| bgr555_to_rgba8(c))
                    .unwrap_or([0xFF, 0x00, 0xFF, 0xFF]);
                img.put(tx * TILE_SIZE + x, ty * TILE_SIZE + y, rgba);
            }
        }
    }
    img
}

/// Color used for a metatile in the synthetic/placeholder renderer (until
/// real tile graphics are decoded): palette color selected by metatile id.
pub fn metatile_color(palette: &Palette, metatile: &Metatile) -> [u8; 4] {
    palette
        .colors
        .get(metatile.id as usize % palette.colors.len().max(1))
        .map(|&c| bgr555_to_rgba8(c))
        .unwrap_or([0xFF, 0x00, 0xFF, 0xFF])
}

/// Render a room to RGBA at `px_per_metatile` resolution using placeholder
/// metatile colors. Deterministic; used by tests and as a minimap source.
pub fn render_room_rgba(
    level: &Level,
    room_index: usize,
    px_per_metatile: usize,
) -> Result<RgbaImage, EditError> {
    let room = level.rooms.get(room_index).ok_or(EditError::RoomOutOfRange(room_index))?;
    let mut img = RgbaImage::new(
        room.width as usize * px_per_metatile,
        room.height as usize * px_per_metatile,
    );
    for ty in 0..room.height {
        for tx in 0..room.width {
            let id = room.tile(tx, ty).unwrap_or(0);
            let rgba = level
                .metatiles
                .get(id as usize)
                .map(|m| metatile_color(&level.palette, m))
                .unwrap_or([0xFF, 0x00, 0xFF, 0xFF]);
            for py in 0..px_per_metatile {
                for px in 0..px_per_metatile {
                    img.put(
                        tx as usize * px_per_metatile + px,
                        ty as usize * px_per_metatile + py,
                        rgba,
                    );
                }
            }
        }
    }
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::level::synthetic_level;
    use crate::snes::tiles::encode_4bpp_tile;

    #[test]
    fn tileset_render_is_deterministic_and_correct() {
        // One tile: all pixels palette index 1; palette[1] = pure red.
        let tile = encode_4bpp_tile(&[[1u8; 8]; 8]);
        let img = render_tileset_rgba(&tile, &[0x0000, 0x001F], 8);
        assert_eq!((img.width, img.height), (64, 8));
        // First tile occupies x 0..8; all red.
        for y in 0..8 {
            for x in 0..8 {
                let i = (y * img.width + x) * 4;
                assert_eq!(&img.pixels[i..i + 4], &[255, 0, 0, 255]);
            }
        }
    }

    #[test]
    fn room_render_is_deterministic() {
        let level = synthetic_level();
        let a = render_room_rgba(&level, 0, 2).unwrap();
        let b = render_room_rgba(&level, 0, 2).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.width, level.rooms[0].width as usize * 2);
        assert_eq!(a.height, level.rooms[0].height as usize * 2);
    }

    #[test]
    fn room_render_rejects_bad_room_index() {
        let level = synthetic_level();
        assert!(render_room_rgba(&level, 99, 2).is_err());
    }
}
