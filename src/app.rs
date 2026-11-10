//! The window: status, target picker, key tester and the required footer.
//! The remapping itself runs on the hook thread (see `hook.rs`), so it keeps
//! working while this window is hidden in the tray.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui;

use crate::hook::{self, HookState, Shared};
use crate::keys::{self, Kind, MEDIA_KEYS, Target};

const WIDTH: f32 = 520.0;
const HEIGHT: f32 = 560.0;

pub fn run(args: Vec<String>) -> eframe::Result<()> {
    let tray = crate::startup::wants_tray(&args);
    // A second copy would double-handle every key press; wake the first.
    // (A sign-in or post-update launch with --tray just exits quietly.)
    if hook::already_running(!tray) {
        return Ok(());
    }
    let viewport = egui::ViewportBuilder::default()
        .with_title("Right Ctrl Back")
        .with_inner_size([WIDTH, HEIGHT])
        .with_min_inner_size([420.0, 420.0])
        .with_visible(!tray);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Right Ctrl Back",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc.egui_ctx.clone())))),
    )
}

pub struct App {
    settings: crate::settings::Settings,
    version: String,
    shared: Arc<Shared>,
}

/// One line saying what the app is doing right now.
pub fn status_line(state: HookState, paused: bool, target: &Target) -> (String, bool) {
    match state {
        HookState::Starting => ("Starting...".into(), true),
        HookState::Failed => (
            "Could not start the keyboard hook. Another program may be blocking it.".into(),
            false,
        ),
        HookState::Running if paused => ("Paused - the AI key behaves as Windows sends it.".into(), true),
        HookState::Running if target.incomplete() => (
            "Waiting for you to choose a program to launch.".into(),
            true,
        ),
        HookState::Running => (format!("Active - the AI key now acts as: {}", target.describe()), true),
    }
}

impl App {
    fn new(ctx: egui::Context) -> Self {
        let settings = crate::settings::load();
        crate::startup::apply(settings.start_with_windows);
        let shared = hook::start(ctx, settings.target.clone());
        Self {
            settings,
            version: crate::update::current_version(),
            shared,
        }
    }

    fn set_target(&mut self, t: Target) {
        if t != self.settings.target {
            self.settings.target = t.clone();
            self.shared.set_target(t);
            crate::settings::save(&self.settings);
        }
    }

    fn toggle_pause(&self) {
        let now = !self.shared.paused.load(Ordering::SeqCst);
        self.shared.set_paused(now);
    }

    fn main_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Right Ctrl Back");
        ui.label("Turn the AI key back into Right Ctrl, or anything else you choose.");
        ui.add_space(8.0);

        // Status + pause.
        let paused = self.shared.paused.load(Ordering::SeqCst);
        let (text, ok) = status_line(self.shared.state(), paused, &self.settings.target);
        ui.horizontal(|ui| {
            if ok {
                ui.label(text);
            } else {
                ui.colored_label(ui.visuals().error_fg_color, text);
            }
        });
        ui.horizontal(|ui| {
            let label = if paused { "Resume  (Ctrl+P)" } else { "Pause  (Ctrl+P)" };
            if ui.button(label).clicked() {
                self.toggle_pause();
            }
            if ui.button("Quit  (Ctrl+Q)").clicked() {
                hook::quit();
            }
        });
        ui.add_space(10.0);
        ui.separator();

        self.target_ui(ui);

        ui.add_space(10.0);
        ui.separator();
        self.tester_ui(ui);

        ui.add_space(10.0);
        ui.separator();
        let mut start = self.settings.start_with_windows;
        if ui
            .checkbox(&mut start, "Start with Windows")
            .on_hover_text("Starts hidden in the notification area when you sign in, so the AI key is always remapped.")
            .changed()
        {
            self.settings.start_with_windows = start;
            crate::startup::apply(start);
            crate::settings::save(&self.settings);
        }
        ui.weak(
            "Closing this window keeps Right Ctrl Back running in the notification area; \
             right-click its icon to pause or quit.",
        );
        ui.weak(
            "Limitation: Windows does not let a normal app see or send keys in windows \
             running as administrator, so the AI key is not remapped while one has focus.",
        );
    }

    fn target_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong("When I press the AI key, send");
        let mut target = self.settings.target.clone();
        let mut kind = target.kind();
        egui::ComboBox::from_id_salt("kind")
            .selected_text(kind.label())
            .width(220.0)
            .show_ui(ui, |ui| {
                for k in Kind::ALL {
                    ui.selectable_value(&mut kind, k, k.label());
                }
            });
        if kind != target.kind() {
            self.shared.capturing.store(false, Ordering::SeqCst);
            target = kind.default_target();
        }

        match &mut target {
            Target::RightCtrl => {
                ui.weak("A real, held Right Ctrl. Works with Shift, Alt and any other key.");
            }
            Target::Menu => {
                ui.weak("The context-menu key (opens the right-click menu from the keyboard).");
            }
            Target::Media(vk) => {
                egui::ComboBox::from_id_salt("media")
                    .selected_text(keys::vk_name(*vk))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for (v, name) in MEDIA_KEYS {
                            ui.selectable_value(vk, v, name);
                        }
                    });
            }
            Target::Key(vk) => {
                if let Some(c) = self.shared.take_captured() {
                    if c != keys::VK_ESCAPE {
                        *vk = c;
                    }
                }
                let capturing = self.shared.capturing.load(Ordering::SeqCst);
                ui.horizontal(|ui| {
                    ui.label(format!("Key: {}", keys::vk_name(*vk)));
                    let label = if capturing { "Press a key... (Esc cancels)" } else { "Choose key" };
                    if ui.button(label).clicked() {
                        self.shared.capturing.store(!capturing, Ordering::SeqCst);
                    }
                });
                ui.weak("Held down for as long as the AI key is held. Win and Shift keys cannot be chosen.");
            }
            Target::Launch(path) => {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(path)
                            .hint_text(r"notepad.exe, C:\Tools\app.exe, or https://...")
                            .desired_width(300.0),
                    );
                });
                if path.trim().is_empty() {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "Enter a program, file or web address to open.",
                    );
                } else {
                    ui.weak("Opens once each time you press the AI key.");
                }
            }
        }
        self.set_target(target);
    }

    fn tester_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong("Key tester");
        let on = self.shared.tester_on.load(Ordering::SeqCst);
        ui.horizontal(|ui| {
            if ui.button(if on { "Stop tester" } else { "Start tester" }).clicked() {
                self.shared.tester_on.store(!on, Ordering::SeqCst);
                self.shared.clear_log();
            }
            if on && ui.button("Clear").clicked() {
                self.shared.clear_log();
            }
        });
        if !on {
            ui.weak(
                "Shows what your keyboard sends and what Right Ctrl Back sends instead. \
                 While it is on, every key you press is listed here (kept only in memory).",
            );
            return;
        }
        let lines = self.shared.log_snapshot();
        egui::ScrollArea::vertical()
            .max_height(180.0)
            .stick_to_bottom(true)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if lines.is_empty() {
                    ui.weak("Press the AI key, or any other key.");
                }
                for l in lines {
                    ui.horizontal(|ui| {
                        if l.emitted {
                            ui.strong("sent    ");
                            ui.label(l.text);
                        } else {
                            ui.weak("keyboard");
                            ui.weak(l.text);
                        }
                    });
                }
            });
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
        let ctx = ui_root.ctx().clone();

        // Closing hides to the tray so the key stays remapped.
        if ctx.input(|i| i.viewport().close_requested()) && hook::tray_available(&self.shared) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.shared.tester_on.store(false, Ordering::SeqCst);
            self.shared.capturing.store(false, Ordering::SeqCst);
            self.shared.clear_log();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::P)) {
            self.toggle_pause();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Q)) {
            hook::quit();
        }

        egui::Panel::bottom("footer").show(ui_root, |ui| self.footer(ui));
        egui::CentralPanel::default().show(ui_root, |ui| self.main_ui(ui));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_active() {
        let (s, ok) = status_line(HookState::Running, false, &Target::RightCtrl);
        assert!(ok);
        assert!(s.contains("Right Ctrl"));
    }

    #[test]
    fn status_paused_and_failed() {
        let (s, _) = status_line(HookState::Running, true, &Target::RightCtrl);
        assert!(s.starts_with("Paused"));
        let (_, ok) = status_line(HookState::Failed, false, &Target::RightCtrl);
        assert!(!ok);
    }

    #[test]
    fn status_launch_without_path() {
        let (s, _) = status_line(HookState::Running, false, &Target::Launch(String::new()));
        assert!(s.contains("choose a program"));
    }
}
