//! Canonical internal level representation.
//!
//! This model is deliberately independent of the (still unknown) on-ROM
//! binary format. Binary codecs in `crate::codecs` translate between ROM
//! bytes and these structures once findings are confirmed. Every `Level`
//! carries a `Provenance` so the UI can label synthetic/speculative data.

use serde::{Deserialize, Serialize};

use crate::error::EditError;

/// Width/height of one SNES screen in 16x16-pixel metatiles (256x224 px
/// visible area; 224 is not a multiple of 16, editors conventionally use 16x14).
pub const SCREEN_W_METATILES: u32 = 16;
pub const SCREEN_H_METATILES: u32 = 14;
/// Metatile edge length in pixels (16x16 = 2x2 hardware tiles). Speculative
/// for this game until the real format is confirmed; standard for the era.
pub const METATILE_PX: u32 = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provenance {
    /// Hand-made data for UI development; not from any ROM.
    Synthetic,
    /// Backed by a confirmed finding in docs/reverse-engineering/.
    Confirmed { note: String },
    /// Decoded with an unverified hypothesis; may be wrong.
    Speculative { note: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    /// Index into the level's metatile set.
    pub metatile: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metatile {
    pub id: u16,
    /// SNES tilemap words composing this metatile, row-major. ROM-backed
    /// metatiles are a 4x4 block of 8x8 tiles (16 words, see
    /// [`crate::level::cell`]); the synthetic prototype uses 4 (2x2).
    pub tiles: Vec<u16>,
    pub palette_row: u8,
    /// Collision class (meaning TBD by reverse engineering).
    pub collision: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    /// BGR555 colors.
    pub colors: Vec<u16>,
}

/// Reconstructed tile pixel graphics for a scene: the SNES VRAM contents and the
/// per-character attribute table the renderer reads, recovered statically by
/// replaying the scene-setup routine's graphics loads (see
/// [`crate::level::loader`]).
///
/// `vram` is raw 4bpp planar tile data placed at each mode-0 DMA's true `$2116`
/// word address (so byte offset `word_addr * 2`). A tile character `c` is decoded
/// from `vram[(char_base + c * 16) * 2 ..][..32]`. `attr` is the `$DB` table: one
/// byte per character giving the SNES tilemap high byte (palette row in bits
/// 2..5, h/v-flip in bits 6/7). Both empty for synthetic / no-ROM levels, in
/// which case the editor falls back to flat metatile colors.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileGraphics {
    /// Raw VRAM bytes (4bpp planar tiles), char `c` at byte `(char_base+c*16)*2`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vram: Vec<u8>,
    /// `$DB` per-character attribute bytes (palette row + flip flags).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attr: Vec<u8>,
    /// VRAM word address of tile character 0 (the BG character base), in 16-word
    /// tile units folded into VRAM words. `char c` lives at word `char_base+c*16`.
    #[serde(default)]
    pub char_base: u16,
}

impl TileGraphics {
    /// Whether any real tile pixels are present (false for synthetic/no-ROM).
    pub fn is_empty(&self) -> bool {
        self.vram.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Object {
    pub id: u32,
    pub kind: u16,
    /// Position in pixels within the room.
    pub x: u32,
    pub y: u32,
    pub params: Vec<u8>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnemySpawn {
    pub id: u32,
    pub kind: u16,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exit {
    pub id: u32,
    pub x: u32,
    pub y: u32,
    pub target_level: u32,
    pub target_room: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub id: u32,
    pub from_room: u32,
    pub to_room: u32,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: u32,
    pub room: u32,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollisionMap {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<u8>,
}

/// One screen-sized cell of a room, derived from room dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Screen {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    pub id: u32,
    pub name: String,
    /// Dimensions in metatiles.
    pub width: u32,
    pub height: u32,
    /// Row-major, `width * height` entries.
    pub tiles: Vec<Tile>,
    /// PC byte offset of this room's tilemap in the original ROM (each cell is a
    /// little-endian 16-bit word at `map_rom_offset + i*2`). Set by the ROM
    /// loader; `None` for synthetic / no-ROM rooms. Tile edits are written back
    /// to the exported ROM through this offset, so it must survive a project
    /// save/load round-trip — `#[serde(default)]` keeps older projects loading.
    #[serde(default)]
    pub map_rom_offset: Option<usize>,
    pub objects: Vec<Object>,
    pub enemy_spawns: Vec<EnemySpawn>,
    pub exits: Vec<Exit>,
    pub transitions: Vec<Transition>,
    pub checkpoints: Vec<Checkpoint>,
    pub collision: Option<CollisionMap>,
}

impl Room {
    pub fn tile(&self, x: u32, y: u32) -> Option<u16> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.tiles.get((y * self.width + x) as usize).map(|t| t.metatile)
    }

    pub fn set_tile(&mut self, x: u32, y: u32, metatile: u16) -> Result<u16, EditError> {
        if x >= self.width || y >= self.height {
            return Err(EditError::TileOutOfRange { x, y, width: self.width, height: self.height });
        }
        let idx = (y * self.width + x) as usize;
        let prev = self.tiles[idx].metatile;
        self.tiles[idx].metatile = metatile;
        Ok(prev)
    }

    /// Screen grid covering this room.
    pub fn screens(&self) -> Vec<Screen> {
        let sx = self.width.div_ceil(SCREEN_W_METATILES);
        let sy = self.height.div_ceil(SCREEN_H_METATILES);
        (0..sy).flat_map(|y| (0..sx).map(move |x| Screen { x, y })).collect()
    }

    pub fn pixel_width(&self) -> u32 {
        self.width * METATILE_PX
    }

    pub fn pixel_height(&self) -> u32 {
        self.height * METATILE_PX
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Level {
    pub id: u32,
    pub name: String,
    pub provenance: Provenance,
    pub palette: Palette,
    pub metatiles: Vec<Metatile>,
    /// Reconstructed tile pixel graphics (empty for synthetic / no-ROM levels,
    /// in which case the editor renders flat metatile colors). Derived from the
    /// ROM on load and **not serialized** — it is a large, re-derivable cache and
    /// embedding ROM-decoded pixels in a saved project is both wasteful and
    /// undesirable.
    #[serde(skip)]
    pub gfx: TileGraphics,
    pub rooms: Vec<Room>,
}

/// Deterministic synthetic level used by the UI prototype and tests.
/// It is invented data and contains nothing from any ROM.
pub fn synthetic_level() -> Level {
    let palette = Palette {
        colors: vec![
            0x0000, // 0: black / transparent
            0x7FFF, // 1: white
            0x001F, // 2: red
            0x03E0, // 3: green
            0x7C00, // 4: blue
            0x03FF, // 5: yellow
            0x7C1F, // 6: magenta
            0x7FE0, // 7: cyan
            0x0210, // 8: dark warm
            0x1084, // 9: dark gray
            0x2108, // 10: gray
            0x39CE, // 11: light gray
            0x0010, // 12: deep red
            0x0200, // 13: deep green
            0x4000, // 14: deep blue
            0x5294, // 15: silver
        ],
    };
    let metatiles = (0u16..16)
        .map(|id| Metatile {
            id,
            tiles: vec![id * 4, id * 4 + 1, id * 4 + 2, id * 4 + 3],
            palette_row: 0,
            collision: u8::from(id >= 8), // upper half are "solid" in the prototype
        })
        .collect::<Vec<_>>();

    let width = SCREEN_W_METATILES * 2; // two screens wide
    let height = SCREEN_H_METATILES;
    let tiles = (0..width * height)
        .map(|i| {
            let (x, y) = (i % width, i / width);
            let metatile = if y == height - 1 {
                8 // ground row
            } else if y == height - 2 && x % 7 == 3 {
                9 // scattered platforms
            } else if x % 11 == 5 && y == height - 3 {
                10
            } else {
                ((x + y) % 4) as u16 // background pattern
            };
            Tile { metatile }
        })
        .collect();

    let room = Room {
        id: 0,
        name: "Synthetic Room A".to_string(),
        width,
        height,
        tiles,
        map_rom_offset: None,
        objects: vec![
            Object { id: 0, kind: 1, x: 40, y: 160, params: vec![0], label: "player-start".into() },
            Object { id: 1, kind: 2, x: 200, y: 160, params: vec![3, 1], label: "powerup".into() },
        ],
        enemy_spawns: vec![
            EnemySpawn { id: 0, kind: 10, x: 320, y: 176 },
            EnemySpawn { id: 1, kind: 11, x: 420, y: 144 },
        ],
        exits: vec![Exit { id: 0, x: 496, y: 176, target_level: 1, target_room: 1 }],
        transitions: vec![Transition {
            id: 0,
            from_room: 0,
            to_room: 1,
            kind: "door".to_string(),
        }],
        checkpoints: vec![Checkpoint { id: 0, room: 0, x: 256, y: 176 }],
        collision: None,
    };

    let mut room_b = Room {
        id: 1,
        name: "Synthetic Room B".to_string(),
        width: SCREEN_W_METATILES,
        height: SCREEN_H_METATILES,
        tiles: (0..SCREEN_W_METATILES * SCREEN_H_METATILES)
            .map(|i| Tile { metatile: (i % 8) as u16 })
            .collect(),
        map_rom_offset: None,
        objects: vec![],
        enemy_spawns: vec![],
        exits: vec![],
        transitions: vec![],
        checkpoints: vec![],
        collision: None,
    };
    room_b.objects.push(Object {
        id: 0,
        kind: 3,
        x: 64,
        y: 96,
        params: vec![],
        label: "marker".into(),
    });

    Level {
        id: 0,
        name: "Synthetic Prototype Level".to_string(),
        provenance: Provenance::Synthetic,
        palette,
        metatiles,
        gfx: TileGraphics::default(),
        rooms: vec![room, room_b],
    }
}
