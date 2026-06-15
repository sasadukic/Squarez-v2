// src/main.rs
#![windows_subsystem = "windows"]

fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/icon.ico");
    let img = image::load_from_memory(bytes)
        .expect("icon.ico")
        .into_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData { rgba: img.into_raw(), width: w, height: h }
}

fn main() -> eframe::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::io::AsRawFd;
        let null = std::fs::File::open("/dev/null").unwrap();
        let fd = null.as_raw_fd();
        unsafe {
            libc::dup2(fd, 0);
            libc::dup2(fd, 1);
            libc::dup2(fd, 2);
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Squarez")
            .with_inner_size([1280.0, 800.0])
            .with_maximized(true)
            .with_icon(std::sync::Arc::new(load_icon())),
        ..Default::default()
    };
    eframe::run_native(
        "Squarez",
        options,
        Box::new(|cc| Ok(Box::new(squarez::app::App::new(cc)))),
    )
}
