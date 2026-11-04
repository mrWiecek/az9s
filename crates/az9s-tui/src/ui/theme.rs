//! Colours, by role. Views ask `theme()` for a role (accent, muted, …)
//! instead of naming a colour, so one setting changes the whole UI.
//!
//! - `default`: the original look, for dark terminals.
//! - `light`: for light terminal backgrounds (no white or yellow text).
//! - `mono`: no colours at all; selection and search matches use reversed
//!   and underlined text. Also chosen when `NO_COLOR` is set
//!   (<https://no-color.org>).
//! - `high-contrast`: bright terminal colours only.
//! - Colour schemes with their own background (24-bit colours): `dracula`,
//!   `nord`, `gruvbox`, `catppuccin` (Mocha), `tokyo-night`,
//!   `solarized-dark`, `solarized-light`. They paint the whole screen, so
//!   they look the same whatever the terminal's background is.

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
    /// The theme's own background (painted over the whole screen), or
    /// `None` for the terminal's
    pub background: Option<Color>,
}

/// A 24-bit colour from `0xRRGGBB`.
const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
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
        background: None,
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
        background: None,
    };

    pub const HIGH_CONTRAST: Theme = Theme {
        name: ThemeName::HighContrast,
        accent: Color::LightCyan,
        text: Color::White,
        muted: Color::Gray,
        subtle: Color::White,
        highlight: Color::LightYellow,
        ok: Color::LightGreen,
        error: Color::LightRed,
        special: Color::LightMagenta,
        info: Color::LightBlue,
        selection_fg: Color::Black,
        selection_bg: Color::LightYellow,
        match_bg: Color::LightCyan,
        current_match_bg: Color::LightRed,
        background: None,
    };

    pub const DRACULA: Theme = Theme {
        name: ThemeName::Dracula,
        accent: rgb(0xbd93f9),
        text: rgb(0xf8f8f2),
        muted: rgb(0x6272a4),
        subtle: rgb(0xbfbfbf),
        highlight: rgb(0xf1fa8c),
        ok: rgb(0x50fa7b),
        error: rgb(0xff5555),
        special: rgb(0xff79c6),
        info: rgb(0x8be9fd),
        selection_fg: rgb(0x282a36),
        selection_bg: rgb(0xbd93f9),
        match_bg: rgb(0xf1fa8c),
        current_match_bg: rgb(0xffb86c),
        background: Some(rgb(0x282a36)),
    };

    pub const NORD: Theme = Theme {
        name: ThemeName::Nord,
        accent: rgb(0x88c0d0),
        text: rgb(0xeceff4),
        muted: rgb(0x7b88a1),
        subtle: rgb(0xd8dee9),
        highlight: rgb(0xebcb8b),
        ok: rgb(0xa3be8c),
        error: rgb(0xbf616a),
        special: rgb(0xb48ead),
        info: rgb(0x81a1c1),
        selection_fg: rgb(0x2e3440),
        selection_bg: rgb(0x88c0d0),
        match_bg: rgb(0xebcb8b),
        current_match_bg: rgb(0xd08770),
        background: Some(rgb(0x2e3440)),
    };

    pub const GRUVBOX: Theme = Theme {
        name: ThemeName::Gruvbox,
        accent: rgb(0x8ec07c),
        text: rgb(0xebdbb2),
        muted: rgb(0x928374),
        subtle: rgb(0xa89984),
        highlight: rgb(0xfabd2f),
        ok: rgb(0xb8bb26),
        error: rgb(0xfb4934),
        special: rgb(0xd3869b),
        info: rgb(0x83a598),
        selection_fg: rgb(0x282828),
        selection_bg: rgb(0x8ec07c),
        match_bg: rgb(0xfabd2f),
        current_match_bg: rgb(0xfe8019),
        background: Some(rgb(0x282828)),
    };

    pub const CATPPUCCIN: Theme = Theme {
        name: ThemeName::Catppuccin,
        accent: rgb(0x89b4fa),
        text: rgb(0xcdd6f4),
        muted: rgb(0x7f849c),
        subtle: rgb(0xa6adc8),
        highlight: rgb(0xf9e2af),
        ok: rgb(0xa6e3a1),
        error: rgb(0xf38ba8),
        special: rgb(0xcba6f7),
        info: rgb(0x74c7ec),
        selection_fg: rgb(0x1e1e2e),
        selection_bg: rgb(0x89b4fa),
        match_bg: rgb(0xf9e2af),
        current_match_bg: rgb(0xfab387),
        background: Some(rgb(0x1e1e2e)),
    };

    pub const TOKYO_NIGHT: Theme = Theme {
        name: ThemeName::TokyoNight,
        accent: rgb(0x7aa2f7),
        text: rgb(0xc0caf5),
        muted: rgb(0x737aa2),
        subtle: rgb(0xa9b1d6),
        highlight: rgb(0xe0af68),
        ok: rgb(0x9ece6a),
        error: rgb(0xf7768e),
        special: rgb(0xbb9af7),
        info: rgb(0x7dcfff),
        selection_fg: rgb(0x1a1b26),
        selection_bg: rgb(0x7aa2f7),
        match_bg: rgb(0xe0af68),
        current_match_bg: rgb(0xff9e64),
        background: Some(rgb(0x1a1b26)),
    };

    pub const SOLARIZED_DARK: Theme = Theme {
        name: ThemeName::SolarizedDark,
        accent: rgb(0x268bd2),
        text: rgb(0x93a1a1),
        muted: rgb(0x657b83),
        subtle: rgb(0x839496),
        highlight: rgb(0xb58900),
        ok: rgb(0x859900),
        error: rgb(0xdc322f),
        special: rgb(0xd33682),
        info: rgb(0x2aa198),
        selection_fg: rgb(0xfdf6e3),
        selection_bg: rgb(0x268bd2),
        match_bg: rgb(0xb58900),
        current_match_bg: rgb(0xcb4b16),
        background: Some(rgb(0x002b36)),
    };

    pub const SOLARIZED_LIGHT: Theme = Theme {
        name: ThemeName::SolarizedLight,
        accent: rgb(0x268bd2),
        text: rgb(0x073642),
        muted: rgb(0x839496),
        subtle: rgb(0x657b83),
        highlight: rgb(0xb58900),
        ok: rgb(0x859900),
        error: rgb(0xdc322f),
        special: rgb(0xd33682),
        info: rgb(0x2aa198),
        selection_fg: rgb(0xfdf6e3),
        selection_bg: rgb(0x268bd2),
        match_bg: rgb(0xb58900),
        current_match_bg: rgb(0xcb4b16),
        background: Some(rgb(0xfdf6e3)),
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
        background: None,
    };

    pub fn named(name: ThemeName) -> Theme {
        match name {
            ThemeName::Default => Self::DEFAULT,
            ThemeName::Light => Self::LIGHT,
            ThemeName::Mono => Self::MONO,
            ThemeName::HighContrast => Self::HIGH_CONTRAST,
            ThemeName::Dracula => Self::DRACULA,
            ThemeName::Nord => Self::NORD,
            ThemeName::Gruvbox => Self::GRUVBOX,
            ThemeName::Catppuccin => Self::CATPPUCCIN,
            ThemeName::TokyoNight => Self::TOKYO_NIGHT,
            ThemeName::SolarizedDark => Self::SOLARIZED_DARK,
            ThemeName::SolarizedLight => Self::SOLARIZED_LIGHT,
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

/// Clear `area` (for a popup) to the theme's background.
pub fn clear(f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
    f.render_widget(ratatui::widgets::Clear, area);
    paint_background(f, area);
}

/// Paint the theme's own background, if it has one, over `area`.
pub fn paint_background(f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
    if let Some(bg) = theme().background {
        f.render_widget(
            ratatui::widgets::Block::default().style(Style::default().bg(bg).fg(theme().text)),
            area,
        );
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
