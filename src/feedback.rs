//! "What would make this better?" - a link in the footer that opens a small
//! box for a note to the developer, sent anonymously to the factory's
//! feedback server. Factory-owned: keep this file as-is.
//!
//! The server URL is baked in at scaffold time from app-factory's
//! `config.json` `feedbackUrl`. When that was empty the link is not shown.
//!
//! It needs only `theme::palette`, `theme::card` and `theme::semibold` from
//! the app's theme, so it also fits apps whose theme predates the template.
//!
//! Apps for children use `Feedback::grown_ups_only()`: the link then reads
//! "For grown-ups" and the box opens behind a sum a child can't do (two
//! two-digit numbers multiplied), as app stores expect of a parental gate.

use std::sync::mpsc;
use std::time::Duration;

use eframe::egui;

use crate::theme;

const URL: &str = "https://feedback.leatham.net/v1/feedback";
const APP: &str = "right-ctrl-back";
const MAX_CHARS: usize = 2000;

fn small() -> egui::FontId {
    egui::FontId::new(12.5, egui::FontFamily::Proportional)
}

fn title() -> egui::FontId {
    egui::FontId::new(20.0, theme::semibold())
}

fn primary_button<'a>(p: theme::Palette, text: &str, enabled: bool, width: f32) -> egui::Button<'a> {
    let label = egui::RichText::new(text)
        .family(theme::semibold())
        .size(15.0)
        .color(if enabled { p.on_accent } else { p.muted });
    egui::Button::new(label)
        .fill(if enabled { p.accent } else { p.track })
        .stroke(egui::Stroke::NONE)
        .corner_radius(10)
        .min_size(egui::vec2(width, 40.0))
}

fn secondary_button<'a>(p: theme::Palette, text: &str) -> egui::Button<'a> {
    egui::Button::new(egui::RichText::new(text).color(p.text))
        .fill(p.control)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(8)
        .min_size(egui::vec2(0.0, 32.0))
}

pub fn enabled() -> bool {
    URL.starts_with("http")
}

#[derive(Default)]
enum State {
    #[default]
    Closed,
    Gate { a: u32, b: u32, answer: String, wrong: bool },
    Editing { error: Option<String> },
    Sending(mpsc::Receiver<Result<(), String>>),
    Sent,
}

/// The box's words. English by default; a translated app passes its own
/// with `Feedback::set_texts` (and again whenever its language changes).
#[derive(Clone, Copy)]
pub struct Texts {
    pub link: &'static str,
    pub title: &'static str,
    pub intro: &'static str,
    pub hint: &'static str,
    pub send: &'static str,
    pub sending: &'static str,
    pub cancel: &'static str,
    pub close: &'static str,
    pub thanks: &'static str,
    pub failed: &'static str,
}

impl Texts {
    pub const ENGLISH: Texts = Texts {
        link: "What would make this better?",
        title: "What would make this better?",
        intro: "Something missing, confusing or annoying? Tell the developer. Only your note and the app version are sent, never your name or email.",
        hint: "It would be better if...",
        send: "Send",
        sending: "Sending...",
        cancel: "Cancel",
        close: "Close",
        thanks: "Thank you. Every note is read.",
        failed: "Couldn't send it. Check your connection and try again.",
    };
}

impl Default for Texts {
    fn default() -> Self {
        Self::ENGLISH
    }
}

#[derive(Default)]
pub struct Feedback {
    state: State,
    text: String,
    gated: bool,
    texts: Texts,
}

/// Two numbers from 13 to 29, different each time the gate opens.
fn gate_numbers() -> (u32, u32) {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ d.as_secs() as u32)
        .unwrap_or(7);
    (13 + n % 17, 13 + (n / 17) % 17)
}

/// Whether `answer` is `a` times `b`, allowing spaces and thousands commas.
pub fn gate_passes(a: u32, b: u32, answer: &str) -> bool {
    let digits: String = answer.chars().filter(|c| !c.is_whitespace() && *c != ',').collect();
    digits.parse::<u32>().is_ok_and(|n| n == a * b)
}

impl Feedback {
    /// For apps used by children: a grown-ups' link and a gate before the box.
    #[allow(dead_code)]
    pub fn grown_ups_only() -> Self {
        Self { gated: true, ..Default::default() }
    }

    /// Words for a translated app; see `Texts`.
    #[allow(dead_code)]
    pub fn set_texts(&mut self, texts: Texts) {
        self.texts = texts;
    }

    /// The footer link. Draws nothing when no server was configured.
    pub fn link(&mut self, ui: &mut egui::Ui) {
        if !enabled() {
            return;
        }
        let label = if self.gated { "For grown-ups: what would make this better?" } else { self.texts.link };
        let text = egui::RichText::new(label)
            .font(small())
            .color(ui.visuals().hyperlink_color);
        if ui
            .add(egui::Label::new(text).sense(egui::Sense::click()))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.open();
        }
    }

    pub fn open(&mut self) {
        if matches!(self.state, State::Closed | State::Sent) {
            if matches!(self.state, State::Sent) {
                self.text.clear();
            }
            self.state = if self.gated {
                let (a, b) = gate_numbers();
                State::Gate { a, b, answer: String::new(), wrong: false }
            } else {
                State::Editing { error: None }
            };
        }
    }

    /// The box itself, as a modal over the window. Call every frame.
    /// `offline` (screenshot mode) never sends anything.
    pub fn show(&mut self, ctx: &egui::Context, version: &str, offline: bool) {
        if let State::Sending(rx) = &self.state {
            match rx.try_recv() {
                Ok(Ok(())) => self.state = State::Sent,
                Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.state = State::Editing {
                        error: Some(self.texts.failed.into()),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if matches!(self.state, State::Closed) {
            return;
        }

        let p = theme::palette(ctx);
        let t = self.texts;
        let mut close = false;
        let mut send = false;
        let mut gate_try = false;
        let modal = egui::Modal::new(egui::Id::new("feedback"))
            .frame(theme::card(p).inner_margin(egui::Margin::same(20)))
            .show(ctx, |ui| {
                // Fits the window down to its minimum width.
                ui.set_width((ui.ctx().content_rect().width() - 80.0).clamp(240.0, 380.0));
                ui.label(
                    egui::RichText::new(t.title)
                        .font(title())
                        .color(p.text),
                );
                ui.add_space(4.0);
                if let State::Gate { a, b, answer, wrong } = &mut self.state {
                    ui.label(egui::RichText::new("This is for grown-ups. Answer this first.").color(p.muted));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(format!("What is {a} × {b}?")).color(p.text));
                    ui.add_space(6.0);
                    let field = ui.add(
                        egui::TextEdit::singleline(answer)
                            .hint_text("Answer")
                            .char_limit(6)
                            .desired_width(120.0),
                    );
                    if answer.is_empty() && !field.has_focus() {
                        field.request_focus();
                    }
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if *wrong {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("That's not it. Here's another one.")
                                .font(small())
                                .color(p.error),
                        );
                    }
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        let ready = !answer.trim().is_empty();
                        if ui.add(primary_button(p, "Continue", ready, 120.0)).clicked() || (enter && ready) {
                            gate_try = true;
                        }
                        if ui.add(secondary_button(p, "Cancel")).clicked() {
                            close = true;
                        }
                    });
                    return;
                }
                if let State::Sent = self.state {
                    ui.label(egui::RichText::new(t.thanks).color(p.muted));
                    ui.add_space(16.0);
                    if ui.add(primary_button(p, t.close, true, ui.available_width())).clicked() {
                        close = true;
                    }
                    return;
                }

                ui.label(egui::RichText::new(t.intro).color(p.muted));
                ui.add_space(12.0);
                let sending = matches!(self.state, State::Sending(_));
                let note = ui.add_enabled(
                    !sending,
                    egui::TextEdit::multiline(&mut self.text)
                        .hint_text(t.hint)
                        .char_limit(MAX_CHARS)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
                if self.text.is_empty() && !sending && !note.has_focus() {
                    note.request_focus();
                }
                if let State::Editing { error: Some(e) } = &self.state {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(e).font(small()).color(p.error));
                }
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let ready = !sending && !self.text.trim().is_empty();
                    let label = if sending { t.sending } else { t.send };
                    if ui.add(primary_button(p, label, ready, 120.0)).clicked() {
                        send = true;
                    }
                    if ui.add_enabled(!sending, secondary_button(p, t.cancel)).clicked() {
                        close = true;
                    }
                });
            });
        if modal.should_close() && !matches!(self.state, State::Sending(_)) {
            close = true;
        }

        if gate_try && let State::Gate { a, b, answer, wrong } = &mut self.state {
            if gate_passes(*a, *b, answer) {
                self.state = State::Editing { error: None };
            } else {
                (*a, *b) = gate_numbers();
                answer.clear();
                *wrong = true;
            }
        }
        if send {
            if offline {
                self.state = State::Sent;
            } else {
                self.state = State::Sending(spawn_send(ctx.clone(), version, self.text.trim()));
            }
        }
        if close {
            if matches!(self.state, State::Sent) {
                self.text.clear();
            }
            self.state = State::Closed;
        }
    }
}

fn spawn_send(ctx: egui::Context, version: &str, text: &str) -> mpsc::Receiver<Result<(), String>> {
    let body = serde_json::json!({
        "app": APP,
        "version": version,
        "platform": "windows",
        "text": text,
    })
    .to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(post(URL, &body));
        ctx.request_repaint();
    });
    rx
}

fn post(url: &str, body: &str) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    agent
        .post(url)
        .header("Content-Type", "application/json")
        .send(body)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn gate_needs_the_product() {
        assert!(super::gate_passes(17, 24, "408"));
        assert!(super::gate_passes(17, 24, " 408 "));
        assert!(!super::gate_passes(17, 24, "407"));
        assert!(!super::gate_passes(17, 24, ""));
        assert!(!super::gate_passes(17, 24, "four hundred"));
    }

    #[test]
    fn gate_numbers_are_two_digit() {
        for _ in 0..50 {
            let (a, b) = super::gate_numbers();
            assert!((13..=29).contains(&a) && (13..=29).contains(&b));
        }
    }

    /// Sends a real note. Run against a local feedback server with
    /// `FEEDBACK_TEST_URL=http://127.0.0.1:8080/v1/feedback cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn sends_a_note() {
        let url = std::env::var("FEEDBACK_TEST_URL").expect("set FEEDBACK_TEST_URL");
        let body = serde_json::json!({
            "app": "template-test", "version": "0.0.0", "platform": "windows", "text": "test note",
        });
        super::post(&url, &body.to_string()).unwrap();
    }
}
