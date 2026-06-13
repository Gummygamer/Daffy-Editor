//! Application state and high-level actions. All ROM bytes live here in a
//! single controlled `RomImage`; GUI modules call methods, they never touch
//! raw buffers.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::editor::history::EditorHistory;
use crate::editor::selection::Selection;
use crate::editor::tools::Tool;
use crate::level::{level_count, load_rom_level};
use crate::model::level::{synthetic_level, Level};
use crate::model::project::{Project, RomIdentity};
use crate::model::validation::{validate_project, ValidationIssue};
use crate::patch::bps::create_bps;
use crate::patch::ips::create_ips;
use crate::rendering::viewport_model::ViewportModel;
use crate::rom::info::{analyze_rom, RomInfo};
use crate::rom::loader::load_rom_file;
use crate::rom::version::RomVersion;
use crate::rom::writer::RomImage;

pub struct RomState {
    pub image: RomImage,
    pub info: RomInfo,
    pub path: Option<PathBuf>,
}

/// Locally persisted user preferences (via eframe storage).
#[derive(Serialize, Deserialize)]
pub struct Prefs {
    pub viewport: ViewportModel,
    pub last_dir: Option<PathBuf>,
    pub show_grid: bool,
    pub show_screen_bounds: bool,
    pub show_objects: bool,
    pub show_collision: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            viewport: ViewportModel::default(),
            last_dir: None,
            show_grid: true,
            show_screen_bounds: true,
            show_objects: true,
            show_collision: false,
        }
    }
}

pub struct DaffyApp {
    pub rom: Option<RomState>,
    pub project: Project,
    pub project_path: Option<PathBuf>,
    pub history: EditorHistory,
    pub selection: Selection,
    pub tool: Tool,
    pub active_metatile: u16,
    pub active_room: usize,
    /// Index of the ROM level currently loaded into `project.levels[0]`.
    pub active_level: usize,
    /// Number of levels the loaded ROM exposes (0 when no ROM / unrecognized).
    pub rom_level_count: usize,
    pub prefs: Prefs,
    pub status: String,
    pub hovered_tile: Option<(u32, u32)>,
    pub validation: Vec<ValidationIssue>,
    pub show_about: bool,
    /// Cache of rendered metatile textures (real tile pixels), keyed by metatile
    /// id, for the level currently in `tile_textures_level`. Cleared on switch.
    pub tile_textures: std::collections::HashMap<u16, egui::TextureHandle>,
    /// Which level id [`Self::tile_textures`] was built for (invalidation key).
    pub tile_textures_level: Option<u32>,
}

impl DaffyApp {
    /// Drop the cached metatile textures (call when the active level's graphics
    /// change, so the viewport rebuilds them lazily).
    pub fn invalidate_tile_textures(&mut self) {
        self.tile_textures.clear();
        self.tile_textures_level = None;
    }
}

impl DaffyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let prefs = cc
            .storage
            .and_then(|s| eframe::get_value::<Prefs>(s, eframe::APP_KEY))
            .unwrap_or_default();
        let mut project = Project::default();
        project.levels.push(synthetic_level());
        let mut app = Self {
            rom: None,
            project,
            project_path: None,
            history: EditorHistory::new(),
            selection: Selection::None,
            tool: Tool::Select,
            active_metatile: 0,
            active_room: 0,
            active_level: 0,
            rom_level_count: 0,
            prefs,
            status: "Ready. Open a legally obtained ROM via File > Open ROM…".to_string(),
            hovered_tile: None,
            validation: Vec::new(),
            show_about: false,
            tile_textures: std::collections::HashMap::new(),
            tile_textures_level: None,
        };
        app.revalidate();
        app
    }

    pub fn level(&self) -> Option<&Level> {
        self.project.levels.first()
    }

    pub fn level_mut(&mut self) -> Option<&mut Level> {
        self.project.levels.first_mut()
    }

    pub fn revalidate(&mut self) {
        self.validation = validate_project(&self.project);
    }

    // ---------- ROM ----------

    pub fn open_rom(&mut self, path: PathBuf) {
        match load_rom_file(&path) {
            Ok(rom) => {
                let info = analyze_rom(&rom.data, rom.had_copier_header);
                self.status = match info.version {
                    RomVersion::Unknown => format!(
                        "⚠ Loaded ROM with UNKNOWN hash (CRC32 {:08X}). Not the supported USA \
                         ROM — structure assumptions may not hold; treat all views as unverified.",
                        info.crc32
                    ),
                    v => format!("Loaded {}.", v.display_name()),
                };
                self.project.rom = Some(RomIdentity::from(&info));
                self.prefs.last_dir = path.parent().map(|p| p.to_path_buf());
                self.rom = Some(RomState {
                    image: RomImage::new(rom.data),
                    info,
                    path: Some(path),
                });
                self.load_real_levels();
                self.revalidate();
            }
            Err(e) => self.status = format!("Failed to open ROM: {e}"),
        }
    }

    /// Replace the placeholder synthetic level with real level 0 decoded from the
    /// loaded ROM. Only runs for the recognized USA ROM (offsets are specific to
    /// it); unknown ROMs keep the synthetic prototype and a warning.
    pub fn load_real_levels(&mut self) {
        let Some(rom) = &self.rom else { return };
        if rom.info.version == RomVersion::Unknown {
            self.rom_level_count = 0;
            return;
        }
        let bytes = rom.image.original().to_vec();
        self.rom_level_count = level_count(&bytes);
        self.load_rom_level_into_project(0, &bytes);
    }

    /// Switch the editor to ROM level `n` (decoding it fresh). Discards unsaved
    /// per-level edits — switching is a navigation action, not an edit.
    pub fn set_active_level(&mut self, n: usize) {
        let Some(rom) = &self.rom else { return };
        let bytes = rom.image.original().to_vec();
        self.load_rom_level_into_project(n, &bytes);
    }

    fn load_rom_level_into_project(&mut self, n: usize, bytes: &[u8]) {
        match load_rom_level(bytes, n) {
            Ok(level) => {
                self.active_level = n;
                self.project.levels = vec![level];
                self.invalidate_tile_textures();
                self.history = EditorHistory::new();
                self.selection = Selection::None;
                self.active_room = 0;
                self.active_metatile = 0;
                self.status = format!("Loaded ROM level {n} of {}.", self.rom_level_count);
                self.revalidate();
            }
            Err(e) => self.status = format!("Could not decode ROM level {n}: {e}"),
        }
    }

    // ---------- project ----------

    pub fn save_project(&mut self, path: PathBuf) {
        match self.project.save_to_file(&path) {
            Ok(()) => {
                self.history.mark_saved();
                self.project_path = Some(path);
                self.status = "Project saved.".to_string();
            }
            Err(e) => self.status = format!("Failed to save project: {e}"),
        }
    }

    pub fn open_project(&mut self, path: PathBuf) {
        match Project::load_from_file(&path) {
            Ok(p) => {
                self.project = p;
                if self.project.levels.is_empty() {
                    self.project.levels.push(synthetic_level());
                }
                self.history = EditorHistory::new();
                self.selection = Selection::None;
                self.active_room = 0;
                self.invalidate_tile_textures();
                self.project_path = Some(path);
                self.status = "Project loaded.".to_string();
                self.revalidate();
            }
            Err(e) => self.status = format!("Failed to open project: {e}"),
        }
    }

    // ---------- export ----------

    /// Build the modified ROM bytes by applying the project's byte-level
    /// changes to a copy of the original. Fails (None + status) without a ROM.
    fn modified_rom_bytes(&mut self) -> Option<(Vec<u8>, Vec<u8>)> {
        let Some(rom) = &mut self.rom else {
            self.status = "Open a ROM first.".to_string();
            return None;
        };
        let mut image = RomImage::new(rom.image.original().to_vec());
        for change in &self.project.changes {
            if let Err(e) = image.write_bytes(change.offset, &change.modified) {
                self.status = format!("Change at {:#x} is invalid: {e}", change.offset);
                return None;
            }
        }
        if let Err(e) = Self::apply_tile_edits(&self.project, &mut image) {
            self.status = format!("Tile edit could not be written to ROM: {e}");
            return None;
        }
        if let Err(e) = Self::apply_object_edits(&self.project, &mut image) {
            self.status = format!("Object edit could not be written to ROM: {e}");
            return None;
        }
        Some((image.original().to_vec(), image.current().to_vec()))
    }

    /// Encode the level model's tilemap cells back into the ROM image. Each cell
    /// is re-encoded from its original word so unedited cells produce no diff;
    /// only painted metatiles change bytes. This is what carries Paint-tool
    /// edits into exported ROMs/patches (the level model is the source of truth;
    /// undo/redo are reflected automatically because we read the current model).
    fn apply_tile_edits(project: &Project, image: &mut RomImage) -> Result<(), crate::error::RomError> {
        use crate::level::cell::encode_cell;
        for level in &project.levels {
            for room in &level.rooms {
                let Some(base) = room.map_rom_offset else { continue };
                for (i, tile) in room.tiles.iter().enumerate() {
                    let off = base + i * 2;
                    // Read the original cell in its own scope so the immutable
                    // borrow ends before the mutable write below.
                    let orig_cell = match image.original().get(off..off + 2) {
                        Some(bytes) => u16::from_le_bytes([bytes[0], bytes[1]]),
                        None => continue,
                    };
                    let new_cell = encode_cell(orig_cell, tile.metatile);
                    if new_cell != orig_cell {
                        image.write_bytes(off, &new_cell.to_le_bytes())?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Write object position edits back into the ROM image. Each object carries
    /// the PC offset of its 24-byte spawn record (X word at `+0x06`, Y word at
    /// `+0x08`); we re-encode X/Y from the current model so unmoved objects
    /// produce no diff. This is the object-move counterpart to
    /// [`Self::apply_tile_edits`] (undo/redo are reflected because we read the
    /// live model). Synthetic objects have no `rom_offset` and are skipped.
    fn apply_object_edits(project: &Project, image: &mut RomImage) -> Result<(), crate::error::RomError> {
        for level in &project.levels {
            for room in &level.rooms {
                for obj in &room.objects {
                    let Some(base) = obj.rom_offset else { continue };
                    image.write_bytes(base + 0x06, &(obj.x as u16).to_le_bytes())?;
                    image.write_bytes(base + 0x08, &(obj.y as u16).to_le_bytes())?;
                }
            }
        }
        Ok(())
    }

    pub fn export_ips(&mut self, path: PathBuf) {
        let Some((original, modified)) = self.modified_rom_bytes() else { return };
        if original == modified {
            self.status = "No byte-level changes to export yet.".to_string();
            return;
        }
        match create_ips(&original, &modified) {
            Ok(patch) => match std::fs::write(&path, patch) {
                Ok(()) => self.status = format!("IPS patch exported to {}.", path.display()),
                Err(e) => self.status = format!("Failed to write patch: {e}"),
            },
            Err(e) => self.status = format!("IPS export failed: {e}"),
        }
    }

    pub fn export_bps(&mut self, path: PathBuf) {
        let Some((original, modified)) = self.modified_rom_bytes() else { return };
        if original == modified {
            self.status = "No byte-level changes to export yet.".to_string();
            return;
        }
        match create_bps(&original, &modified, "made with daffy-editor") {
            Ok(patch) => match std::fs::write(&path, patch) {
                Ok(()) => self.status = format!("BPS patch exported to {}.", path.display()),
                Err(e) => self.status = format!("Failed to write patch: {e}"),
            },
            Err(e) => self.status = format!("BPS export failed: {e}"),
        }
    }

    /// Local-only convenience export; patches are the preferred distribution
    /// format (see docs/LEGAL.md).
    pub fn export_modified_rom(&mut self, path: PathBuf) {
        let Some((_, modified)) = self.modified_rom_bytes() else { return };
        match std::fs::write(&path, modified) {
            Ok(()) => {
                self.status = format!(
                    "Modified ROM written to {} (for local use only — share patches, not ROMs).",
                    path.display()
                );
            }
            Err(e) => self.status = format!("Failed to write ROM: {e}"),
        }
    }

    // ---------- editing ----------

    pub fn undo(&mut self) {
        let Some(level) = self.project.levels.first_mut() else { return };
        if self.history.undo(level) {
            self.status = "Undid last edit.".to_string();
            self.revalidate();
        }
    }

    pub fn redo(&mut self) {
        let Some(level) = self.project.levels.first_mut() else { return };
        if self.history.redo(level) {
            self.status = "Redid edit.".to_string();
            self.revalidate();
        }
    }
}

impl eframe::App for DaffyApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.prefs);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        crate::ui::menu::shortcuts(self, ctx);
        crate::ui::menu::menu_bar(self, ctx);
        crate::ui::panels::side_panel(self, ctx);
        crate::ui::panels::status_bar(self, ctx);
        crate::ui::viewport::central_viewport(self, ctx);
        crate::ui::dialogs::about_window(self, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::level::{Level, Object, Palette, Provenance, Room, Tile};

    fn room_with_map(offset: usize, tiles: Vec<u16>) -> Room {
        Room {
            id: 0,
            name: "t".into(),
            width: tiles.len() as u32,
            height: 1,
            tiles: tiles.into_iter().map(|metatile| Tile { metatile }).collect(),
            map_rom_offset: Some(offset),
            objects: vec![],
            enemy_spawns: vec![],
            exits: vec![],
            transitions: vec![],
            checkpoints: vec![],
            collision: None,
        }
    }

    fn project_with_room(room: Room) -> Project {
        let level = Level {
            id: 0,
            name: "t".into(),
            provenance: Provenance::Synthetic,
            palette: Palette { colors: vec![] },
            metatiles: vec![],
            gfx: Default::default(),
            rooms: vec![room],
        };
        Project { levels: vec![level], ..Project::default() }
    }

    #[test]
    fn paint_edit_is_written_into_exported_rom_bytes() {
        // ROM tilemap: two cells at offset 4, both metatile index 1 ($0020).
        let mut rom = vec![0u8; 16];
        rom[4..6].copy_from_slice(&0x0020u16.to_le_bytes());
        rom[6..8].copy_from_slice(&0x0020u16.to_le_bytes());
        let mut image = RomImage::new(rom);

        // "Paint" the second cell with metatile 5; first cell untouched.
        let project = project_with_room(room_with_map(4, vec![1, 5]));
        DaffyApp::apply_tile_edits(&project, &mut image).unwrap();

        let cur = image.current();
        // Untouched cell produces no diff.
        assert_eq!(u16::from_le_bytes([cur[4], cur[5]]), 0x0020);
        // Painted cell now selects metatile 5 ($00A0 == 5 << 5).
        assert_eq!(u16::from_le_bytes([cur[6], cur[7]]), 0x00A0);
        assert!(image.is_modified());
    }

    #[test]
    fn moved_object_is_written_into_exported_rom_bytes() {
        // A 24-byte spawn record at offset 4: handler ptr at [0..3], X at +6, Y at +8.
        let mut rom = vec![0u8; 64];
        rom[4..7].copy_from_slice(&[0x50, 0xD9, 0x80]); // non-zero handler ptr
        rom[10..12].copy_from_slice(&100u16.to_le_bytes()); // X
        rom[12..14].copy_from_slice(&200u16.to_le_bytes()); // Y
        let mut image = RomImage::new(rom);

        let mut room = room_with_map(40, vec![1]);
        room.objects.push(Object {
            id: 0,
            kind: 0x80_D950,
            x: 300,
            y: 150,
            params: vec![],
            label: "moved".into(),
            rom_offset: Some(4),
        });
        let project = project_with_room(room);
        DaffyApp::apply_object_edits(&project, &mut image).unwrap();

        let cur = image.current();
        assert_eq!(u16::from_le_bytes([cur[10], cur[11]]), 300);
        assert_eq!(u16::from_le_bytes([cur[12], cur[13]]), 150);
        // Handler pointer untouched.
        assert_eq!(&cur[4..7], &[0x50, 0xD9, 0x80]);
    }

    #[test]
    fn unmoved_object_produces_no_byte_diff() {
        let mut rom = vec![0u8; 64];
        rom[10..12].copy_from_slice(&100u16.to_le_bytes());
        rom[12..14].copy_from_slice(&200u16.to_le_bytes());
        let mut image = RomImage::new(rom);

        let mut room = room_with_map(40, vec![1]);
        room.objects.push(Object {
            id: 0,
            kind: 0,
            x: 100,
            y: 200,
            params: vec![],
            label: "still".into(),
            rom_offset: Some(4),
        });
        let project = project_with_room(room);
        DaffyApp::apply_object_edits(&project, &mut image).unwrap();
        assert!(!image.is_modified());
    }

    #[test]
    fn unedited_tilemap_produces_no_byte_diff() {
        let mut rom = vec![0u8; 16];
        // A cell with flag + low bits set; loader stored index 1.
        rom[4..6].copy_from_slice(&0x8033u16.to_le_bytes());
        let mut image = RomImage::new(rom);

        // metatile_index(0x8033) == 1; re-encoding to 1 must reproduce the byte.
        let project = project_with_room(room_with_map(4, vec![1]));
        DaffyApp::apply_tile_edits(&project, &mut image).unwrap();

        assert!(!image.is_modified());
    }
}
