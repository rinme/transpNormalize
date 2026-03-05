//! GUI entry point for remove_transparent_margins.
//!
//! On Windows, `#![windows_subsystem = "windows"]` suppresses the console window
//! so only the GUI window is shown.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod gui;
mod processing;

use eframe::egui;

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Remove Transparent Margins")
            .with_inner_size([860.0, 560.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "Remove Transparent Margins",
        options,
        Box::new(|cc| Ok(Box::new(gui::App::new(cc)))),
    )
    .unwrap_or_else(|e| eprintln!("GUI error: {e}"));
}
