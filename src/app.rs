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
        Some((image.original().to_vec(), image.current().to_vec()))
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
