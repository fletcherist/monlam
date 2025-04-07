mod audio;
mod config;
mod daw;
mod ui;
mod group;

use daw::DawApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Monlam",
        options,
        Box::new(|cc| Box::new(DawApp::new(cc))),
    )
}
