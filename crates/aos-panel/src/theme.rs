//! The visual language, in one place, so there is only ever one of it.
//!
//! **Colour carries state and nothing else.** Never decoration and never a category, so a
//! coloured thing on this screen is always something that might need a person. The palette is
//! deliberately short: one accent for live, three state hues, and a colourless grey that means
//! nobody knows.
//!
//! **Depth comes from the background ramp**, not from shadows or borders. Four steps.
//!
//! **Monospace throughout**, because almost everything here is an identifier, a sequence
//! number or an exit code, and those line up or they are hard to scan. Nothing below 11px.
//!
//! **Text labels, not colour alone.** A refusal says REFUSED as well as being red, because a
//! colour blind reader and a black and white screenshot both have to work.

use eframe::egui::{Color32, FontFamily, FontId, Rounding, Stroke, TextStyle};

/// The background ramp, darkest first.
pub const VOID: Color32 = Color32::from_rgb(6, 8, 11);
pub const PANEL: Color32 = Color32::from_rgb(11, 14, 19);
pub const RAISED: Color32 = Color32::from_rgb(17, 21, 28);
pub const HOVER: Color32 = Color32::from_rgb(24, 30, 39);

/// Hairlines. The layout is built from rules rather than from boxes with edges.
pub const RULE: Color32 = Color32::from_rgb(30, 37, 48);
pub const RULE_BRIGHT: Color32 = Color32::from_rgb(48, 58, 74);

/// Text, brightest first. Three steps and no more.
pub const TEXT: Color32 = Color32::from_rgb(214, 222, 233);
pub const DIM: Color32 = Color32::from_rgb(138, 150, 166);
pub const FAINT: Color32 = Color32::from_rgb(88, 99, 114);

/// The one accent. Live, selected, needs you.
pub const ACCENT: Color32 = Color32::from_rgb(255, 176, 63);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(122, 84, 30);

/// State colours. These are the only other hues on the screen.
pub const GOOD: Color32 = Color32::from_rgb(94, 196, 140);
pub const WARN: Color32 = Color32::from_rgb(226, 165, 62);
pub const BAD: Color32 = Color32::from_rgb(232, 92, 84);
pub const COLD: Color32 = Color32::from_rgb(96, 158, 214);

/// Unknown is deliberately colourless. It is a gap in what is known, not a state to act on,
/// and painting every gap as an alarm teaches somebody to ignore the alarms.
pub const UNKNOWN: Color32 = Color32::from_rgb(96, 106, 122);

pub const CORNER: Rounding = Rounding::same(3.0_f32);

pub fn hairline() -> Stroke {
    Stroke::new(1.0_f32, RULE)
}

pub fn edge(color: Color32) -> Stroke {
    Stroke::new(1.0_f32, color)
}

/// Text roles, named for what they are for, so a size is never chosen at a call site.
pub fn heading() -> FontId {
    FontId::new(15.0, FontFamily::Monospace)
}

pub fn body() -> FontId {
    FontId::new(13.0, FontFamily::Monospace)
}

/// Small caps labels. 11px is the floor, and spacing does the work instead of going smaller.
pub fn label() -> FontId {
    FontId::new(11.0, FontFamily::Monospace)
}

pub fn big() -> FontId {
    FontId::new(20.0, FontFamily::Monospace)
}

/// Spreads a short label out, which reads as deliberate where a tiny font reads as cramped.
pub fn spaced(text: &str) -> String {
    text.chars()
        .flat_map(|c| [c, ' '])
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Applies the whole language to a context. Called every frame rather than once, because egui
/// reapplies the desktop preference and this desktop is in light mode.
pub fn install(ctx: &eframe::egui::Context) {
    use eframe::egui::{ThemePreference, Visuals, style::Selection};

    ctx.options_mut(|o| o.theme_preference = ThemePreference::Dark);

    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = VOID;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = VOID;
    visuals.faint_bg_color = RAISED;
    visuals.selection = Selection {
        bg_fill: ACCENT_DIM,
        stroke: Stroke::new(1.0_f32, ACCENT),
    };
    visuals.widgets.noninteractive.bg_stroke = hairline();
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, DIM);
    visuals.widgets.inactive.bg_fill = RAISED;
    visuals.widgets.inactive.weak_bg_fill = RAISED;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.rounding = CORNER;
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.weak_bg_fill = HOVER;
    visuals.widgets.hovered.bg_stroke = edge(RULE_BRIGHT);
    visuals.widgets.hovered.rounding = CORNER;
    visuals.widgets.active.bg_fill = HOVER;
    visuals.widgets.active.weak_bg_fill = HOVER;
    visuals.widgets.active.bg_stroke = edge(ACCENT);
    visuals.widgets.active.rounding = CORNER;
    visuals.window_stroke = hairline();
    visuals.popup_shadow = eframe::egui::epaint::Shadow::NONE;
    visuals.window_shadow = eframe::egui::epaint::Shadow::NONE;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, heading()),
        (TextStyle::Body, body()),
        (TextStyle::Monospace, body()),
        (TextStyle::Button, body()),
        (TextStyle::Small, label()),
    ]
    .into();
    style.spacing.item_spacing = eframe::egui::vec2(8.0, 6.0);
    style.spacing.button_padding = eframe::egui::vec2(9.0, 5.0);
    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color32) -> f32 {
        let (r, g, b, _) = c.to_tuple();
        0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
    }

    #[test]
    fn no_text_role_is_too_small_to_read() {
        for f in [heading(), body(), label(), big()] {
            assert!(f.size >= 11.0, "{f:?} is too small");
        }
    }

    /// Unknown must not look like something to act on, so it carries no hue.
    #[test]
    fn unknown_is_colourless() {
        let (r, g, b, _) = UNKNOWN.to_tuple();
        let spread = r.max(g).max(b) - r.min(g).min(b);
        assert!(spread < 32, "unknown should be near grey, got {r},{g},{b}");
    }

    #[test]
    fn the_states_that_matter_are_told_apart_by_hue() {
        assert_ne!(BAD, WARN);
        assert_ne!(BAD, GOOD);
        assert_ne!(UNKNOWN, GOOD);
    }

    #[test]
    fn text_stands_off_the_background() {
        assert!(luminance(TEXT) - luminance(VOID) > 100.0, "body text");
        assert!(luminance(ACCENT) - luminance(VOID) > 100.0, "the accent");
        assert!(luminance(DIM) - luminance(VOID) > 40.0, "secondary text");
        assert!(luminance(BAD) - luminance(VOID) > 40.0, "a refusal");
    }

    /// The desktop here is in light mode and egui follows it every frame. Asserting on visuals
    /// alone passes against a build where the desktop wins a fraction of a second later.
    #[test]
    fn the_installed_theme_is_dark_whatever_the_desktop_prefers() {
        let ctx = eframe::egui::Context::default();
        ctx.options_mut(|o| o.theme_preference = eframe::egui::ThemePreference::Light);
        install(&ctx);

        assert_eq!(
            ctx.options(|o| o.theme_preference),
            eframe::egui::ThemePreference::Dark
        );
        let visuals = ctx.style().visuals.clone();
        assert!(visuals.dark_mode);
        assert_eq!(visuals.override_text_color, Some(TEXT));
        assert!(luminance(TEXT) - luminance(visuals.panel_fill) > 100.0);
    }

    /// `default-features = false` drops `default_fonts`, and without it egui lays out no
    /// glyphs at all while still painting every rectangle. That failure looks exactly like
    /// dark text on a dark background and is nothing of the sort, so it is measured in glyphs.
    #[test]
    fn there_are_fonts_and_they_produce_actual_glyphs() {
        let ctx = eframe::egui::Context::default();
        install(&ctx);
        let _ = ctx.run(Default::default(), |_| {});

        for font in [heading(), body(), label(), big()] {
            let galley = ctx.fonts(|f| f.layout_no_wrap("AOSD".to_string(), font.clone(), TEXT));
            let glyphs: usize = galley.rows.iter().map(|r| r.glyphs.len()).sum();
            assert_eq!(
                glyphs, 4,
                "{font:?} produced {glyphs} glyphs for four letters"
            );
        }
    }

    #[test]
    fn a_label_is_spread_not_shrunk() {
        assert_eq!(spaced("AOS"), "A O S");
        assert_eq!(spaced(""), "");
    }
}
