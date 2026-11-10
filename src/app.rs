//! The app itself. The build stage replaces the body of `App::main_ui` (and
//! adds modules alongside) with the real feature; the footer with the
//! auto-update toggle and version is required on every factory app.

use eframe::egui;

const WIDTH: f32 = 480.0;
const HEIGHT: f32 = 360.0;

pub fn run(_args: Vec<String>) -> eframe::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("Right Ctrl Back")
        .with_inner_size([WIDTH, HEIGHT])
        .with_min_inner_size([360.0, 240.0]);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Right Ctrl Back",
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    )
}

pub struct App {
    settings: crate::settings::Settings,
    version: String,
}

impl App {
    fn new() -> Self {
        Self {
            settings: crate::settings::load(),
            version: crate::update::current_version(),
        }
    }

    fn main_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Right Ctrl Back");
        ui.label("Turn the AI key on new laptops back into Right Ctrl, or anything else you choose.");
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut auto_update = self.settings.auto_update;
            if ui
                .checkbox(&mut auto_update, "Automatically update")
                .on_hover_text("Checks for new versions about once a day and installs them in the background.")
                .changed()
            {
                self.settings.auto_update = auto_update;
                crate::settings::save(&self.settings);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!("v{}", self.version));
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui_root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::bottom("footer").show(ui_root, |ui| self.footer(ui));
        egui::CentralPanel::default().show(ui_root, |ui| self.main_ui(ui));
    }
}
