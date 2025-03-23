mod audio;
mod daw;
mod ui;

use daw::{DawApp, DawState};
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Monlam DAW",
        options,
        Box::new(|_cc| Box::new(DawApp::new())),
    )
}
