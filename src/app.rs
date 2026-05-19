// src/app.rs
use crate::project::Project;
use crate::theme::{Theme, load_fonts};

pub struct App {
    pub project: Project,
    pub theme: Theme,
}

impl Default for App {
    fn default() -> Self {
        Self {
            project: Project::new(32, 32, "Untitled".to_string()),
            theme: Theme::default(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.theme.apply(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Squarez — theme test");
        });
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        load_fonts(&cc.egui_ctx);
        Self::default()
    }
}
