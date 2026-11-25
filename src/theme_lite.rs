//! Just enough of app-factory's house theme (`template/src/theme.rs`) for the
//! factory's `feedback.rs`, in an app that has not had the design pass yet.
//! It takes its colours from egui's current look and changes nothing else.
//! `main.rs` loads it as `theme` (`#[path = "theme_lite.rs"] mod theme;`);
//! `./factory.ps1 polish` replaces it with the real theme.

use eframe::egui::{self, Color32, FontFamily, Stroke};

#[derive(Clone, Copy)]
pub struct Palette {
    pub surface: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub control: Color32,
    pub track: Color32,
    pub error: Color32,
}

pub fn palette(ctx: &egui::Context) -> Palette {
    let v = ctx.style_of(ctx.theme()).visuals.clone();
    Palette {
        surface: v.window_fill,
        border: v.widgets.noninteractive.bg_stroke.color,
        text: v.text_color(),
        muted: v.weak_text_color(),
        accent: v.selection.bg_fill,
        on_accent: Color32::WHITE,
        control: v.widgets.inactive.weak_bg_fill,
        track: v.widgets.inactive.bg_fill,
        error: v.error_fg_color,
    }
}

/// egui's default fonts have no semibold, so this is the regular face.
pub fn semibold() -> FontFamily {
    FontFamily::Proportional
}

pub fn card(p: Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(16, 12))
}
