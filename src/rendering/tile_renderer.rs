//! Deterministic CPU-side rendering of tiles, tilesets and rooms into RGBA
//! buffers. The GUI uploads these as egui textures; tests assert on the bytes.

use crate::error::EditError;
use crate::level::cell::{metatile_word_offset, METATILE_DIM};
use crate::model::level::{Level, Metatile, Palette, TileGraphics};
use crate::snes::palette::bgr555_to_rgba8;
use crate::snes::tiles::{decode_4bpp_tile, TILE_4BPP_BYTES, TILE_SIZE};

/// Edge length of a rendered metatile in pixels (a 4×4 block of 8×8 tiles).
pub const METATILE_RENDER_PX: usize = METATILE_DIM * TILE_SIZE; // 32

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

/// Render one metatile to a `METATILE_RENDER_PX`-square RGBA image using real
/// reconstructed tile pixels.
///
/// The metatile's 16 tile words form a 4×4 grid of 8×8 tiles. For each tile word
/// the character (`word & 0x3FF`) selects a tile in [`TileGraphics::vram`] (at
/// VRAM word `char_base + char * 16`); the `$DB` attribute byte for that
/// character supplies the palette row (bits 2..5) and h/v-flip (bits 6/7), as the
/// game's renderer does. Pixel index 0 is the SNES backdrop (`palette[0]`); other
/// indices map through the tile's 16-color palette row. Returns `None` if the
/// graphics are empty (caller should fall back to [`metatile_color`]).
pub fn render_metatile_rgba(
    gfx: &TileGraphics,
    palette: &Palette,
    metatile: &Metatile,
) -> Option<RgbaImage> {
    if gfx.is_empty() {
        return None;
    }
    let mut img = RgbaImage::new(METATILE_RENDER_PX, METATILE_RENDER_PX);
    let backdrop = palette.colors.first().map(|&c| bgr555_to_rgba8(c)).unwrap_or([0, 0, 0, 255]);

    for subrow in 0..METATILE_DIM {
        for subcol in 0..METATILE_DIM {
            let widx = metatile_word_offset(subcol, subrow) / 2;
            let word = metatile.tiles.get(widx).copied().unwrap_or(0);
            let chr = (word & 0x03FF) as usize;
            let attr = gfx.attr.get(chr).copied().unwrap_or(0);
            let pal_row = ((attr >> 2) & 0x07) as usize;
            let hflip = attr & 0x40 != 0;
            let vflip = attr & 0x80 != 0;

            let byte0 = (gfx.char_base as usize + chr * 16) * 2;
            let Some(tile) = gfx.vram.get(byte0..byte0 + TILE_4BPP_BYTES) else { continue };
            let bytes: &[u8; TILE_4BPP_BYTES] = tile.try_into().expect("32 bytes");
            let pixels = decode_4bpp_tile(bytes);

            for (y, row) in pixels.iter().enumerate() {
                for (x, &px) in row.iter().enumerate() {
                    let rgba = if px == 0 {
                        backdrop
                    } else {
                        palette
                            .colors
                            .get(pal_row * 16 + px as usize)
                            .map(|&c| bgr555_to_rgba8(c))
                            .unwrap_or([0xFF, 0x00, 0xFF, 0xFF])
                    };
                    let fx = if hflip { TILE_SIZE - 1 - x } else { x };
                    let fy = if vflip { TILE_SIZE - 1 - y } else { y };
                    img.put(subcol * TILE_SIZE + fx, subrow * TILE_SIZE + fy, rgba);
                }
            }
        }
    }
    Some(img)
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
    fn metatile_render_uses_real_tiles_palette_row_and_flip() {
        // VRAM: char 1 is a tile whose top row is index 1, index 2 elsewhere.
        let mut tile_px = [[2u8; 8]; 8];
        tile_px[0] = [1u8; 8]; // distinguishable top row
        let mut vram = vec![0u8; 64];
        vram[32..64].copy_from_slice(&encode_4bpp_tile(&tile_px)); // char 1 @ byte 32

        // Attribute for char 1: palette row 1 (bits 2..5), no flip.
        let mut attr = vec![0u8; 0x400];
        attr[1] = 0x04; // (row 1) << 2

        let gfx = TileGraphics { vram, attr, char_base: 0 };

        // Palette: backdrop black; row 1 (offset 16): color1=red, color2=green.
        let mut colors = vec![0u16; 256];
        colors[0] = 0x0000; // backdrop
        colors[16 + 1] = 0x001F; // red
        colors[16 + 2] = 0x03E0; // green
        let palette = Palette { colors };

        // Metatile: only the top-left subtile uses char 1; rest are char 0 (zeros).
        let mut tiles = vec![0u16; 16];
        tiles[0] = 1; // (subcol 0, subrow 0)
        let mt = Metatile { id: 0, tiles, palette_row: 0, collision: 0 };

        let img = render_metatile_rgba(&gfx, &palette, &mt).unwrap();
        assert_eq!((img.width, img.height), (32, 32));
        let at = |x: usize, y: usize| {
            let i = (y * img.width + x) * 4;
            [img.pixels[i], img.pixels[i + 1], img.pixels[i + 2], img.pixels[i + 3]]
        };
        // Top-left tile, row 0 -> index 1 -> red; row 1 -> index 2 -> green.
        assert_eq!(at(0, 0), [255, 0, 0, 255]);
        assert_eq!(at(7, 1), [0, 255, 0, 255]);
        // A char-0 subtile (top-right) is all index 0 -> backdrop black.
        assert_eq!(at(8, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn metatile_render_none_when_no_graphics() {
        let mt = Metatile { id: 0, tiles: vec![0u16; 16], palette_row: 0, collision: 0 };
        let palette = Palette { colors: vec![0u16; 256] };
        assert!(render_metatile_rgba(&TileGraphics::default(), &palette, &mt).is_none());
    }

    #[test]
    fn room_render_rejects_bad_room_index() {
        let level = synthetic_level();
        assert!(render_room_rgba(&level, 99, 2).is_err());
    }
}
