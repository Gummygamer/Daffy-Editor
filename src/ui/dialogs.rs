//! Modal-ish windows: About / legal notice.

use crate::app::DaffyApp;

pub fn about_window(app: &mut DaffyApp, ctx: &egui::Context) {
    if !app.show_about {
        return;
    }
    let mut open = app.show_about;
    egui::Window::new("About Daffy Editor")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(concat!("Daffy Editor v", env!("CARGO_PKG_VERSION")));
            ui.label("A native level editor / reverse-engineering workbench for");
            ui.label("Daffy Duck: The Marvin Missions (SNES).");
            ui.separator();
            ui.label("This tool contains NO game data. You must supply your own");
            ui.label("legally obtained ROM. Share your work as IPS/BPS patches,");
            ui.label("never as modified ROMs. See docs/LEGAL.md.");
            ui.separator();
            ui.label("Supported ROM: USA, LoROM, 1 MiB, CRC32 5F02A044.");
        });
    app.show_about = open;
}
