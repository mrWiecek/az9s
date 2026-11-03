//! Colours, by role. Views ask `theme()` for a role (accent, muted, …)
//! instead of naming a colour, so one setting changes the whole UI.
//!
//! - `default`: the original look, for dark terminals.
//! - `light`: for light terminal backgrounds (no white or yellow text).
//! - `mono`: no colours at all; selection and search matches use reversed
//!   and underlined text. Also chosen when `NO_COLOR` is set
//!   (<https://no-color.org>).

use az9s_core::config::ThemeName;
use ratatui::style::{Color, Modifier, Style};
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: ThemeName,
    /// Borders of popups, key hints, headings, highlights of structure
    pub accent: Color,
    /// Normal text: names
    pub text: Color,
    /// Secondary text: IDs, details, hints
    pub muted: Color,
    /// Placeholder and empty-state messages
    pub subtle: Color,
    /// Values worth noticing: counts, locations, labels
    pub highlight: Color,
    /// Good / ready / enabled
    pub ok: Color,
    /// Errors, disabled
    pub error: Color,
    /// Filters, tenants: things that change what you see
    pub special: Color,
    /// Rarely used extra colour (network types)
    pub info: Color,
    /// Selected row
    selection_fg: Color,
    selection_bg: Color,
    /// Search matches
    match_bg: Color,
    current_match_bg: Color,
}

impl Theme {
    pub const DEFAULT: Theme = Theme {
        name: ThemeName::Default,
        accent: Color::Cyan,
        text: Color::White,
        muted: Color::DarkGray,
        subtle: Color::Gray,
        highlight: Color::Yellow,
        ok: Color::Green,
        error: Color::Red,
        special: Color::Magenta,
        info: Color::Blue,
        selection_fg: Color::Black,
        selection_bg: Color::Cyan,
        match_bg: Color::Yellow,
        current_match_bg: Color::LightRed,
    };

    pub const LIGHT: Theme = Theme {
        name: ThemeName::Light,
        accent: Color::Blue,
        text: Color::Black,
        muted: Color::DarkGray,
        subtle: Color::DarkGray,
        highlight: Color::Magenta,
        ok: Color::Green,
        error: Color::Red,
        special: Color::Magenta,
        info: Color::Blue,
        selection_fg: Color::White,
        selection_bg: Color::Blue,
        match_bg: Color::Yellow,
        current_match_bg: Color::LightRed,
    };

    pub const MONO: Theme = Theme {
        name: ThemeName::Mono,
        accent: Color::Reset,
        text: Color::Reset,
        muted: Color::Reset,
        subtle: Color::Reset,
        highlight: Color::Reset,
        ok: Color::Reset,
        error: Color::Reset,
        special: Color::Reset,
        info: Color::Reset,
        selection_fg: Color::Reset,
        selection_bg: Color::Reset,
        match_bg: Color::Reset,
        current_match_bg: Color::Reset,
    };

    pub fn named(name: ThemeName) -> Theme {
        match name {
            ThemeName::Default => Self::DEFAULT,
            ThemeName::Light => Self::LIGHT,
            ThemeName::Mono => Self::MONO,
        }
    }

    fn mono(&self) -> bool {
        self.name == ThemeName::Mono
    }

    /// The selected row (and key hints in the footer).
    pub fn selection(&self) -> Style {
        if self.mono() {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default()
                .fg(self.selection_fg)
                .bg(self.selection_bg)
                .add_modifier(Modifier::BOLD)
        }
    }

    /// A search match; `current` is the one `n`/`N` is on.
    pub fn search_match(&self, current: bool) -> Style {
        match (self.mono(), current) {
            (true, false) => Style::default().add_modifier(Modifier::UNDERLINED),
            (true, true) => Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
            (false, false) => Style::default().fg(Color::Black).bg(self.match_bg),
            (false, true) => Style::default()
                .fg(Color::Black)
                .bg(self.current_match_bg)
                .add_modifier(Modifier::BOLD),
        }
    }

    /// Foreground for text drawn on the selection (e.g. matched letters in
    /// the palette's selected row).
    pub fn on_selection(&self) -> Color {
        self.selection_fg
    }
}

static THEME: RwLock<Theme> = RwLock::new(Theme::DEFAULT);

/// The current theme.
pub fn theme() -> Theme {
    *THEME.read().unwrap_or_else(|e| e.into_inner())
}

/// Use `name` from now on; `NO_COLOR` (set and non-empty) forces `mono`.
pub fn set_theme(name: ThemeName) {
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    *THEME.write().unwrap_or_else(|e| e.into_inner()) = Theme::named(resolve(name, no_color));
}

/// The theme to use: `NO_COLOR` wins over the configured one.
fn resolve(name: ThemeName, no_color: bool) -> ThemeName {
    if no_color {
        ThemeName::Mono
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_color_wins_over_the_config() {
        assert_eq!(resolve(ThemeName::Light, true), ThemeName::Mono);
        assert_eq!(resolve(ThemeName::Light, false), ThemeName::Light);
    }

    #[test]
    fn default_keeps_the_original_colours() {
        let t = Theme::DEFAULT;
        assert_eq!(t.selection().bg, Some(Color::Cyan));
        assert_eq!(t.search_match(false).bg, Some(Color::Yellow));
        assert_eq!(t.search_match(true).bg, Some(Color::LightRed));
    }

    #[test]
    fn light_avoids_white_and_yellow_text() {
        let t = Theme::LIGHT;
        for c in [t.accent, t.text, t.muted, t.subtle, t.highlight] {
            assert!(
                !matches!(c, Color::White | Color::Yellow | Color::Gray),
                "{c:?}"
            );
        }
    }

    #[test]
    fn mono_uses_no_colour_only_modifiers() {
        let t = Theme::MONO;
        for style in [t.selection(), t.search_match(false), t.search_match(true)] {
            assert_eq!(style.fg, None);
            assert_eq!(style.bg, None);
            assert!(!style.add_modifier.is_empty());
        }
        assert_ne!(
            t.search_match(false),
            t.search_match(true),
            "current match stands out"
        );
    }
}
