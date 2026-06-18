// Aegis — console opérateur native (egui). Boot : runtime tokio + config + fenêtre eframe.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod poll;
mod tray;
mod views;

use aegis_core::Config;

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let rt_handle = rt.handle().clone();
    let cfg = Config::load();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aegis")
            .with_inner_size([920.0, 600.0])
            .with_min_inner_size([720.0, 460.0]),
        ..Default::default()
    };

    let res = eframe::run_native(
        "Aegis",
        native_options,
        Box::new(move |cc| Ok(Box::new(app::AegisApp::new(cc, rt_handle, cfg)))),
    );
    drop(rt); // garde le runtime vivant jusqu'à la sortie de la fenêtre
    res.expect("eframe");
}
