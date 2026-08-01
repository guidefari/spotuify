//! System-aware semantic theme state for the desktop client.
//!
//! Views depend only on [`Palette`]. Theme selection and system appearance
//! live behind this module so a future preferences surface can swap
//! [`ThemePreference`] or [`ThemeFamily`] without changing view code.

use gpui::{App, BorrowAppContext, Global, ReadGlobal, WindowAppearance};

/// Explicit variants are reserved for the future theme preference control.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ThemePreference {
    System,
    Light,
    Dark,
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

impl ThemeFamily {
    pub const SPOTUIFY: Self = Self {
        light: Palette {
            bg_root: 0xf7f2ed,
            bg_surface: 0xfffaf6,
            bg_elevated: 0xeee5de,
            bg_sidebar: 0xf0e7e0,
            border: 0xddd0c6,
            border_strong: 0xc7b5a8,
            text_primary: 0x241d19,
            text_secondary: 0x5f5149,
            text_muted: 0x786961,
            accent: 0xb85f2e,
            accent_hover: 0x99491f,
            accent_subtle: 0xf2d9c7,
            nav_active: 0xefd7c8,
            nav_hover: 0xf4e5da,
            queue_current: 0xefd7c8,
            queue_idle: 0xfffaf6,
            button_primary: 0xb85f2e,
            button_primary_text: 0xffffff,
            button_secondary: 0xeadfd7,
            button_secondary_hover: 0xdecec3,
            slider_track: 0xd7c8be,
            slider_fill: 0xb85f2e,
            error: 0xa33b3b,
            error_surface: 0xf8dddd,
            error_border: 0xdbaaaa,
        },
        dark: Palette {
            bg_root: 0x141211,
            bg_surface: 0x1c1816,
            bg_elevated: 0x27201d,
            bg_sidebar: 0x201a17,
            border: 0x3a302b,
            border_strong: 0x51443c,
            text_primary: 0xf5eee8,
            text_secondary: 0xd0c1b7,
            text_muted: 0xa99a90,
            accent: 0xe3a06f,
            accent_hover: 0xf0b886,
            accent_subtle: 0x4a3024,
            nav_active: 0x3a2922,
            nav_hover: 0x2d221e,
            queue_current: 0x3a2922,
            queue_idle: 0x1c1816,
            button_primary: 0xe3a06f,
            button_primary_text: 0x211812,
            button_secondary: 0x332923,
            button_secondary_hover: 0x44352d,
            slider_track: 0x443831,
            slider_fill: 0xe3a06f,
            error: 0xe88989,
            error_surface: 0x3b2222,
            error_border: 0x6a3d36,
        },
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
        let family = ThemeFamily::SPOTUIFY;
        let preference = ThemePreference::System;
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

    #[allow(dead_code)]
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
                ThemeFamily::SPOTUIFY,
                WindowAppearance::Light,
            ),
            ThemeFamily::SPOTUIFY.light
        );
        assert_eq!(
            resolve_palette(
                ThemePreference::System,
                ThemeFamily::SPOTUIFY,
                WindowAppearance::Dark,
            ),
            ThemeFamily::SPOTUIFY.dark
        );
    }

    #[test]
    fn explicit_preference_ignores_system_appearance() {
        assert_eq!(
            resolve_palette(
                ThemePreference::Dark,
                ThemeFamily::SPOTUIFY,
                WindowAppearance::Light,
            ),
            ThemeFamily::SPOTUIFY.dark
        );
        assert_eq!(
            resolve_palette(
                ThemePreference::Light,
                ThemeFamily::SPOTUIFY,
                WindowAppearance::Dark,
            ),
            ThemeFamily::SPOTUIFY.light
        );
    }
}
