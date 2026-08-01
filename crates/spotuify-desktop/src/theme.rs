//! System-aware semantic theme state for the desktop client.
//!
//! Views depend only on [`Palette`]. Theme selection and system appearance
//! live behind this module so a future preferences surface can swap
//! [`ThemePreference`] or [`ThemeFamily`] without changing view code.

use gpui::{App, BorrowAppContext, Global, ReadGlobal, WindowAppearance};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ThemePreference {
    System,
    Light,
    Dark,
}

impl ThemePreference {
    pub(crate) const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    const fn persisted(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Palette {
    pub bg_root: u32,
    pub bg_surface: u32,
    pub bg_elevated: u32,
    pub bg_sidebar: u32,
    pub border: u32,
    pub border_strong: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub accent_subtle: u32,
    pub nav_active: u32,
    pub nav_hover: u32,
    pub queue_current: u32,
    pub queue_idle: u32,
    pub button_primary: u32,
    pub button_primary_text: u32,
    pub button_secondary: u32,
    pub button_secondary_hover: u32,
    pub slider_track: u32,
    pub slider_fill: u32,
    pub error: u32,
    pub error_surface: u32,
    pub error_border: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThemeFamily {
    pub light: Palette,
    pub dark: Palette,
}

#[derive(Clone, Copy)]
struct ThemeColors {
    base: u32,
    mantle: u32,
    crust: u32,
    surface_0: u32,
    surface_1: u32,
    text: u32,
    subtext_1: u32,
    subtext_0: u32,
    accent: u32,
    error: u32,
}

impl ThemeColors {
    const fn palette(self) -> Palette {
        Palette {
            bg_root: self.base,
            bg_surface: self.mantle,
            bg_elevated: self.surface_0,
            bg_sidebar: self.mantle,
            border: self.surface_0,
            border_strong: self.surface_1,
            text_primary: self.text,
            text_secondary: self.subtext_1,
            text_muted: self.subtext_0,
            accent: self.accent,
            accent_hover: self.accent,
            accent_subtle: self.surface_0,
            nav_active: self.surface_0,
            nav_hover: self.crust,
            queue_current: self.surface_0,
            queue_idle: self.mantle,
            button_primary: self.accent,
            button_primary_text: self.crust,
            button_secondary: self.surface_0,
            button_secondary_hover: self.surface_1,
            slider_track: self.surface_1,
            slider_fill: self.accent,
            error: self.error,
            error_surface: self.surface_0,
            error_border: self.surface_1,
        }
    }
}

impl ThemeFamily {
    /// Catppuccin Latte + Mocha, reduced to ten semantic source colors each.
    pub const CATPPUCCIN: Self = Self {
        light: ThemeColors {
            base: 0xeff1f5,
            mantle: 0xe6e9ef,
            crust: 0xdce0e8,
            surface_0: 0xccd0da,
            surface_1: 0xbcc0cc,
            text: 0x4c4f69,
            subtext_1: 0x5c5f77,
            subtext_0: 0x6c6f85,
            accent: 0xfe640b,
            error: 0xd20f39,
        }
        .palette(),
        dark: ThemeColors {
            base: 0x1e1e2e,
            mantle: 0x181825,
            crust: 0x11111b,
            surface_0: 0x313244,
            surface_1: 0x45475a,
            text: 0xcdd6f4,
            subtext_1: 0xbac2de,
            subtext_0: 0xa6adc8,
            accent: 0xfab387,
            error: 0xf38ba8,
        }
        .palette(),
    };
}

pub(crate) struct DesktopTheme {
    preference: ThemePreference,
    family: ThemeFamily,
    active: Palette,
}

impl Global for DesktopTheme {}

impl DesktopTheme {
    pub(crate) fn new(appearance: WindowAppearance) -> Self {
        let family = ThemeFamily::CATPPUCCIN;
        let preference = load_preference();
        let active = resolve_palette(preference, family, appearance);
        Self {
            preference,
            family,
            active,
        }
    }

    fn sync_appearance(&mut self, appearance: WindowAppearance) {
        self.active = resolve_palette(self.preference, self.family, appearance);
    }

    pub(crate) fn preference(&self) -> ThemePreference {
        self.preference
    }

    pub(crate) fn set_preference(
        &mut self,
        preference: ThemePreference,
        appearance: WindowAppearance,
    ) {
        self.preference = preference;
        self.sync_appearance(appearance);
    }

    #[allow(dead_code)]
    pub(crate) fn set_family(&mut self, family: ThemeFamily, appearance: WindowAppearance) {
        self.family = family;
        self.sync_appearance(appearance);
    }
}

pub(crate) trait ActiveTheme {
    fn desktop_theme(&self) -> &Palette;
}

impl ActiveTheme for App {
    fn desktop_theme(&self) -> &Palette {
        &DesktopTheme::global(self).active
    }
}

pub(crate) fn init(cx: &mut App) {
    let appearance = cx.window_appearance();
    cx.set_global(DesktopTheme::new(appearance));
}

pub(crate) fn sync_system_appearance(appearance: WindowAppearance, cx: &mut impl BorrowAppContext) {
    cx.update_global::<DesktopTheme, _>(|theme, _| theme.sync_appearance(appearance));
}

pub(crate) fn preference(cx: &App) -> ThemePreference {
    DesktopTheme::global(cx).preference()
}

pub(crate) fn choose_preference(
    preference: ThemePreference,
    appearance: WindowAppearance,
    cx: &mut impl BorrowAppContext,
) -> std::io::Result<()> {
    cx.update_global::<DesktopTheme, _>(|theme, _| theme.set_preference(preference, appearance));
    persist_preference(preference)
}

fn preference_path() -> std::path::PathBuf {
    spotuify_protocol::paths::config_dir().join("desktop-theme")
}

fn load_preference() -> ThemePreference {
    std::fs::read_to_string(preference_path())
        .ok()
        .and_then(|value| ThemePreference::parse(&value))
        .unwrap_or(ThemePreference::System)
}

fn persist_preference(preference: ThemePreference) -> std::io::Result<()> {
    let path = preference_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", preference.persisted()))
}

fn resolve_palette(
    preference: ThemePreference,
    family: ThemeFamily,
    appearance: WindowAppearance,
) -> Palette {
    let use_dark = match preference {
        ThemePreference::Dark => true,
        ThemePreference::Light => false,
        ThemePreference::System => matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
    };
    if use_dark {
        family.dark
    } else {
        family.light
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_preference_resolves_both_appearance_palettes() {
        assert_eq!(
            resolve_palette(
                ThemePreference::System,
                ThemeFamily::CATPPUCCIN,
                WindowAppearance::Light,
            ),
            ThemeFamily::CATPPUCCIN.light
        );
        assert_eq!(
            resolve_palette(
                ThemePreference::System,
                ThemeFamily::CATPPUCCIN,
                WindowAppearance::Dark,
            ),
            ThemeFamily::CATPPUCCIN.dark
        );
    }

    #[test]
    fn explicit_preference_ignores_system_appearance() {
        assert_eq!(
            resolve_palette(
                ThemePreference::Dark,
                ThemeFamily::CATPPUCCIN,
                WindowAppearance::Light,
            ),
            ThemeFamily::CATPPUCCIN.dark
        );
        assert_eq!(
            resolve_palette(
                ThemePreference::Light,
                ThemeFamily::CATPPUCCIN,
                WindowAppearance::Dark,
            ),
            ThemeFamily::CATPPUCCIN.light
        );
    }
}
