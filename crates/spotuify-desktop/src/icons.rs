use gpui::prelude::*;
use gpui::{svg, AnyView, App, AssetSource, SharedString, Window};
use std::borrow::Cow;

use crate::theme::ActiveTheme;

macro_rules! phosphor_icons {
    ($($variant:ident => $path:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) enum AppIcon {
            $($variant),+
        }

        impl AppIcon {
            pub(crate) const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub(crate) const fn path(self) -> &'static str {
                match self {
                    $(Self::$variant => $path),+
                }
            }
        }

        fn icon_bytes(path: &str) -> Option<&'static [u8]> {
            match path {
                $($path => Some(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/", $path))),)+
                _ => None,
            }
        }
    };
}

phosphor_icons! {
    NowPlaying => "icons/phosphor/regular/waveform.svg",
    Search => "icons/phosphor/regular/magnifying-glass.svg",
    LikedSongs => "icons/phosphor/regular/heart.svg",
    Albums => "icons/phosphor/regular/disc.svg",
    Artists => "icons/phosphor/regular/users-three.svg",
    Podcasts => "icons/phosphor/regular/broadcast.svg",
    Playlists => "icons/phosphor/regular/playlist.svg",
    History => "icons/phosphor/regular/clock-counter-clockwise.svg",
    Notifications => "icons/phosphor/regular/bell.svg",
    Devices => "icons/phosphor/regular/devices.svg",
    Lyrics => "icons/phosphor/regular/quotes.svg",
    Preferences => "icons/phosphor/regular/gear.svg",
    Queue => "icons/phosphor/regular/queue.svg",
    Volume => "icons/phosphor/regular/speaker-high.svg",
    Previous => "icons/phosphor/bold/skip-back.svg",
    Next => "icons/phosphor/bold/skip-forward.svg",
    Shuffle => "icons/phosphor/bold/shuffle.svg",
    Repeat => "icons/phosphor/bold/repeat.svg",
    RepeatOne => "icons/phosphor/bold/repeat-once.svg",
    Play => "icons/phosphor/fill/play.svg",
    Pause => "icons/phosphor/fill/pause.svg",
    LikedSongsFilled => "icons/phosphor/fill/heart.svg",
}

pub(crate) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(icon_bytes(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(AppIcon::ALL
            .iter()
            .map(|icon| icon.path())
            .filter(|icon_path| icon_path.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

pub(crate) fn icon(icon: AppIcon, size: f32, color: u32) -> impl IntoElement {
    svg()
        .path(icon.path())
        .size(gpui::px(size))
        .text_color(gpui::rgb(color))
}

struct IconTooltip {
    label: SharedString,
}

impl Render for IconTooltip {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut gpui::Context<'_, Self>,
    ) -> impl IntoElement {
        gpui::div()
            .rounded_md()
            .bg(gpui::rgb(cx.desktop_theme().bg_elevated))
            .border_1()
            .border_color(gpui::rgb(cx.desktop_theme().border_strong))
            .px_3()
            .py_2()
            .text_xs()
            .text_color(gpui::rgb(cx.desktop_theme().text_primary))
            .child(self.label.clone())
    }
}

pub(crate) fn tooltip(label: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView {
    let label = label.into();
    move |_, cx| {
        cx.new(|_| IconTooltip {
            label: label.clone(),
        })
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_an_embedded_svg() {
        let assets = Assets;
        for icon in AppIcon::ALL {
            let bytes = assets
                .load(icon.path())
                .expect("icon asset lookup should succeed")
                .expect("icon asset should be embedded");
            assert!(bytes.starts_with(b"<svg"), "{} is not SVG", icon.path());
        }
    }
}
