// src/main.rs
#![windows_subsystem = "windows"]

fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/logo.png");
    let img = image::load_from_memory(bytes)
        .expect("logo.png")
        .into_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData { rgba: img.into_raw(), width: w, height: h }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Squarez")
            .with_inner_size([1280.0, 800.0])
            .with_icon(std::sync::Arc::new(load_icon())),
        ..Default::default()
    };
    eframe::run_native(
        "Squarez",
        options,
        Box::new(|cc| Ok(Box::new(squarez::app::App::new(cc)))),
    )
}
