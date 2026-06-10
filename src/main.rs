#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Daffy Editor — Daffy Duck: The Marvin Missions"),
        ..Default::default()
    };
    eframe::run_native(
        "daffy-editor",
        options,
        Box::new(|cc| Ok(Box::new(daffy_editor::app::DaffyApp::new(cc)))),
    )
}
