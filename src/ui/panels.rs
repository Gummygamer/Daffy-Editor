//! Side panel (ROM info, room/metatile/palette pickers, objects, validation)
//! and bottom status bar.

use egui_extras::{Column, TableBuilder};

use crate::app::DaffyApp;
use crate::editor::selection::Selection;
use crate::editor::tools::Tool;
use crate::model::level::Provenance;
use crate::rendering::tile_renderer::metatile_color;
use crate::rom::version::RomVersion;
use crate::snes::palette::bgr555_to_rgba8;

pub fn side_panel(app: &mut DaffyApp, ctx: &egui::Context) {
    egui::SidePanel::left("side_panel").default_width(300.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            rom_info_section(app, ui);
            ui.separator();
            level_section(app, ui);
            ui.separator();
            metatile_picker(app, ui);
            ui.separator();
            palette_viewer(app, ui);
            ui.separator();
            object_list(app, ui);
            ui.separator();
            validation_section(app, ui);
        });
    });
}

fn rom_info_section(app: &DaffyApp, ui: &mut egui::Ui) {
    ui.heading("ROM");
    let Some(rom) = &app.rom else {
        ui.label("No ROM loaded. The editor never ships ROM data;");
        ui.label("open your own legally obtained copy.");
        return;
    };
    let info = &rom.info;
    match info.version {
        RomVersion::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(255, 120, 60),
                "⚠ UNKNOWN ROM — hash not recognized.\nAll structure views are unverified.",
            );
        }
        v => {
            ui.colored_label(egui::Color32::from_rgb(110, 220, 110), v.display_name());
        }
    }
    egui::Grid::new("rom_info_grid").num_columns(2).striped(true).show(ui, |ui| {
        ui.label("Size");
        ui.monospace(format!("{} bytes ({} KiB)", info.size, info.size / 1024));
        ui.end_row();
        ui.label("CRC32");
        ui.monospace(format!("{:08X}", info.crc32));
        ui.end_row();
        ui.label("SHA-1");
        ui.monospace(&info.sha1_hex);
        ui.end_row();
        ui.label("Copier header");
        ui.monospace(if info.had_copier_header { "yes (512 B, stripped)" } else { "no" });
        ui.end_row();
        if let Some(h) = &info.internal {
            ui.label("Internal title");
            ui.monospace(&h.title);
            ui.end_row();
            ui.label("Map mode");
            ui.monospace(format!(
                "{:#04X}{}",
                h.map_mode,
                if h.map_mode & 0x0F == 0 { " (LoROM)" } else { "" }
            ));
            ui.end_row();
            ui.label("ROM / SRAM size");
            ui.monospace(format!(
                "{} KiB / {} KiB",
                1u32 << h.rom_size,
                if h.sram_size == 0 { 0 } else { 1u32 << h.sram_size }
            ));
            ui.end_row();
            ui.label("Checksum");
            ui.monospace(format!("{:04X} (~{:04X})", h.checksum, h.checksum_complement));
            ui.end_row();
        }
    });
}

fn level_section(app: &mut DaffyApp, ui: &mut egui::Ui) {
    ui.heading("Level");
    let Some(level) = app.project.levels.first() else {
        ui.label("No level data.");
        return;
    };
    match &level.provenance {
        Provenance::Synthetic => {
            ui.colored_label(
                egui::Color32::YELLOW,
                "SYNTHETIC placeholder data — not from the ROM.\n\
                 The real level format is not reverse engineered yet.",
            );
        }
        Provenance::Speculative { note } => {
            ui.colored_label(egui::Color32::ORANGE, format!("SPECULATIVE: {note}"));
        }
        Provenance::Confirmed { note } => {
            ui.colored_label(egui::Color32::LIGHT_GREEN, format!("Confirmed: {note}"));
        }
    }
    let room_names: Vec<String> =
        level.rooms.iter().map(|r| format!("{} — {}", r.id, r.name)).collect();
    let mut active = app.active_room.min(room_names.len().saturating_sub(1));
    egui::ComboBox::from_label("Room").selected_text(room_names.get(active).cloned().unwrap_or_default()).show_ui(
        ui,
        |ui| {
            for (i, name) in room_names.iter().enumerate() {
                ui.selectable_value(&mut active, i, name);
            }
        },
    );
    if active != app.active_room {
        app.active_room = active;
        app.selection = Selection::None;
    }
    ui.horizontal(|ui| {
        ui.label("Tool:");
        ui.selectable_value(&mut app.tool, Tool::Select, Tool::Select.label());
        ui.selectable_value(&mut app.tool, Tool::Paint, Tool::Paint.label());
    });
}

fn metatile_picker(app: &mut DaffyApp, ui: &mut egui::Ui) {
    ui.heading("Metatile picker");
    let Some(level) = app.project.levels.first() else { return };
    let palette = level.palette.clone();
    let metatiles = level.metatiles.clone();
    ui.horizontal_wrapped(|ui| {
        for m in &metatiles {
            let [r, g, b, a] = metatile_color(&palette, m);
            let color = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
            let selected = app.active_metatile == m.id;
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
            ui.painter().rect_filled(rect, 2.0, color);
            if selected {
                ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(2.0, egui::Color32::WHITE));
            }
            if resp.clicked() {
                app.active_metatile = m.id;
                app.tool = Tool::Paint;
            }
            resp.on_hover_text(format!("metatile {} (collision {})", m.id, m.collision));
        }
    });
    ui.label(format!("Active: {} — click canvas with Paint tool", app.active_metatile));
}

fn palette_viewer(app: &DaffyApp, ui: &mut egui::Ui) {
    ui.heading("Palette (BGR555)");
    let Some(level) = app.project.levels.first() else { return };
    ui.horizontal_wrapped(|ui| {
        for (i, &c) in level.palette.colors.iter().enumerate() {
            let [r, g, b, a] = bgr555_to_rgba8(c);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 1.0, egui::Color32::from_rgba_unmultiplied(r, g, b, a));
            resp.on_hover_text(format!("#{i}: ${c:04X} → rgb({r}, {g}, {b})"));
        }
    });
}

fn object_list(app: &mut DaffyApp, ui: &mut egui::Ui) {
    ui.heading("Objects");
    let room_idx = app.active_room;
    let Some(level) = app.project.levels.first() else { return };
    let Some(room) = level.rooms.get(room_idx) else { return };
    let rows: Vec<(u32, String, u32, u32)> =
        room.objects.iter().map(|o| (o.id, o.label.clone(), o.x, o.y)).collect();
    if rows.is_empty() {
        ui.label("(none in this room)");
        return;
    }
    TableBuilder::new(ui)
        .striped(true)
        .column(Column::auto())
        .column(Column::remainder())
        .column(Column::auto())
        .header(18.0, |mut header| {
            header.col(|ui| {
                ui.strong("id");
            });
            header.col(|ui| {
                ui.strong("label");
            });
            header.col(|ui| {
                ui.strong("pos");
            });
        })
        .body(|mut body| {
            for (i, (id, label, x, y)) in rows.iter().enumerate() {
                body.row(18.0, |mut row| {
                    let selected =
                        app.selection == Selection::Object { room: room_idx, index: i };
                    row.col(|ui| {
                        ui.monospace(format!("{id}"));
                    });
                    row.col(|ui| {
                        if ui.selectable_label(selected, label).clicked() {
                            app.selection = Selection::Object { room: room_idx, index: i };
                        }
                    });
                    row.col(|ui| {
                        ui.monospace(format!("({x}, {y})"));
                    });
                });
            }
        });
}

fn validation_section(app: &DaffyApp, ui: &mut egui::Ui) {
    ui.heading("Validation");
    if app.validation.is_empty() {
        ui.colored_label(egui::Color32::LIGHT_GREEN, "No issues.");
        return;
    }
    for issue in &app.validation {
        let color = match issue.severity {
            crate::model::validation::Severity::Error => egui::Color32::from_rgb(255, 90, 90),
            crate::model::validation::Severity::Warning => egui::Color32::from_rgb(255, 180, 70),
            crate::model::validation::Severity::Info => egui::Color32::GRAY,
        };
        ui.colored_label(color, format!("[{:?}] {} — {}", issue.severity, issue.context, issue.message));
    }
}

pub fn status_bar(app: &DaffyApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(&app.status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if app.history.is_dirty() {
                    ui.colored_label(egui::Color32::YELLOW, "● unsaved");
                }
                ui.label(format!("zoom {:.2}×", app.prefs.viewport.zoom));
                if let Some((tx, ty)) = app.hovered_tile {
                    ui.monospace(format!("tile ({tx}, {ty})"));
                }
                if let Some(rom) = &app.rom {
                    ui.monospace(format!("CRC32 {:08X}", rom.info.crc32));
                }
            });
        });
    });
}
