//! Menu bar, native file dialogs and keyboard shortcuts.

use crate::app::DaffyApp;
use crate::model::project::PROJECT_FILE_EXTENSION;

fn rom_dialog(app: &DaffyApp) -> rfd::FileDialog {
    let mut d = rfd::FileDialog::new()
        .add_filter("SNES ROM", &["sfc", "smc", "bin"])
        .add_filter("All files", &["*"]);
    if let Some(dir) = &app.prefs.last_dir {
        d = d.set_directory(dir);
    }
    d
}

pub fn shortcuts(app: &mut DaffyApp, ctx: &egui::Context) {
    use egui::{Key, Modifiers};
    let mut undo = false;
    let mut redo = false;
    let mut open = false;
    let mut save = false;
    let mut copy = false;
    let mut paste = false;
    let mut delete = false;
    ctx.input_mut(|i| {
        undo = i.consume_key(Modifiers::COMMAND, Key::Z);
        redo = i.consume_key(Modifiers::COMMAND, Key::Y)
            || i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
        open = i.consume_key(Modifiers::COMMAND, Key::O);
        save = i.consume_key(Modifiers::COMMAND, Key::S);
        copy = i.consume_key(Modifiers::COMMAND, Key::C);
        paste = i.consume_key(Modifiers::COMMAND, Key::V);
        delete = i.consume_key(Modifiers::NONE, Key::Delete);
    });
    if undo {
        app.undo();
    }
    if redo {
        app.redo();
    }
    if open {
        if let Some(path) = rom_dialog(app).pick_file() {
            app.open_rom(path);
        }
    }
    if save {
        save_project_action(app);
    }
    if copy {
        app.copy_selection();
    }
    if paste {
        app.paste_clipboard();
    }
    if delete {
        app.delete_selection();
    }
}

fn save_project_action(app: &mut DaffyApp) {
    let path = app.project_path.clone().or_else(|| {
        rfd::FileDialog::new()
            .set_file_name(format!("{}.{PROJECT_FILE_EXTENSION}", app.project.name))
            .add_filter("Daffy Editor project", &["json"])
            .save_file()
    });
    if let Some(path) = path {
        app.save_project(path);
    }
}

pub fn menu_bar(app: &mut DaffyApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open ROM…    (Ctrl+O)").clicked() {
                    ui.close_menu();
                    if let Some(path) = rom_dialog(app).pick_file() {
                        app.open_rom(path);
                    }
                }
                ui.separator();
                if ui.button("Open Project…").clicked() {
                    ui.close_menu();
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Daffy Editor project", &["json"])
                        .pick_file()
                    {
                        app.open_project(path);
                    }
                }
                if ui.button("Save Project    (Ctrl+S)").clicked() {
                    ui.close_menu();
                    save_project_action(app);
                }
                if ui.button("Save Project As…").clicked() {
                    ui.close_menu();
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(format!("{}.{PROJECT_FILE_EXTENSION}", app.project.name))
                        .add_filter("Daffy Editor project", &["json"])
                        .save_file()
                    {
                        app.save_project(path);
                    }
                }
                ui.separator();
                let can_export = app.rom.is_some();
                if ui
                    .add_enabled(can_export, egui::Button::new("Export IPS Patch…"))
                    .clicked()
                {
                    ui.close_menu();
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("daffy-hack.ips")
                        .add_filter("IPS patch", &["ips"])
                        .save_file()
                    {
                        app.export_ips(path);
                    }
                }
                if ui
                    .add_enabled(can_export, egui::Button::new("Export BPS Patch…"))
                    .clicked()
                {
                    ui.close_menu();
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("daffy-hack.bps")
                        .add_filter("BPS patch", &["bps"])
                        .save_file()
                    {
                        app.export_bps(path);
                    }
                }
                if ui
                    .add_enabled(
                        can_export,
                        egui::Button::new("Export Modified ROM (local only)…"),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("daffy-modified.sfc")
                        .add_filter("SNES ROM", &["sfc"])
                        .save_file()
                    {
                        app.export_modified_rom(path);
                    }
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(
                        app.history.can_undo(),
                        egui::Button::new("Undo    (Ctrl+Z)"),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    app.undo();
                }
                if ui
                    .add_enabled(
                        app.history.can_redo(),
                        egui::Button::new("Redo    (Ctrl+Y)"),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    app.redo();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        app.can_delete_selection(),
                        egui::Button::new("Copy    (Ctrl+C)"),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    app.copy_selection();
                }
                if ui
                    .add_enabled(app.can_paste(), egui::Button::new("Paste    (Ctrl+V)"))
                    .clicked()
                {
                    ui.close_menu();
                    app.paste_clipboard();
                }
                if ui
                    .add_enabled(
                        app.can_delete_selection(),
                        egui::Button::new("Delete    (Del)"),
                    )
                    .clicked()
                {
                    ui.close_menu();
                    app.delete_selection();
                }
            });

            ui.menu_button("View", |ui| {
                if ui.button("Reset View").clicked() {
                    ui.close_menu();
                    app.prefs.viewport = Default::default();
                }
                ui.checkbox(&mut app.prefs.show_grid, "Tile grid");
                ui.checkbox(&mut app.prefs.show_screen_bounds, "Screen boundaries");
                ui.checkbox(&mut app.prefs.show_objects, "Object overlay");
                ui.checkbox(&mut app.prefs.show_collision, "Collision overlay");
            });

            ui.menu_button("Help", |ui| {
                if ui.button("About / Legal").clicked() {
                    ui.close_menu();
                    app.show_about = true;
                }
            });
        });
    });
}
