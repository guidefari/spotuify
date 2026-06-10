//! Windows SMTC driver (`cfg(windows)`).
//!
//! souvlaki's SystemMediaTransportControls needs a real `HWND` whose
//! message pump is running, and the SMTC must live on the same thread
//! that owns that window. A daemon has no UI, so we spawn a dedicated
//! thread that creates a hidden message-only `winit` window, builds the
//! souvlaki controls against its `HWND`, and runs the event loop forever
//! to pump SMTC button presses. The main thread pushes
//! metadata/playback updates over an `EventLoopProxy`; SMTC button
//! presses flow back over the same `commands_tx` the other platforms
//! use.
//!
//! Runtime note: this path is cross-compile-verified (cargo-xwin) but
//! must still get manual SMTC QA on a real Windows box — there is no
//! Windows runner in CI. If anything here fails, `MediaControlsHandle`
//! init returns `Err`, `SystemIntegration::spawn` logs it, and the
//! daemon runs without media controls (it never bricks playback).

use std::sync::mpsc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use spotuify_protocol::PlaybackCommand;
use tokio::sync::mpsc as tokio_mpsc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowId};

use crate::media_controls::{map_media_control_event, souvlaki_event_to_action};

/// Owned metadata/playback update marshalled to the window thread (the
/// souvlaki API borrows `&str`, so the proxy payload owns its strings).
pub enum ControlUpdate {
    Metadata {
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
        cover_url: Option<String>,
        duration_ms: Option<u64>,
    },
    Playback(PlaybackKind),
}

pub enum PlaybackKind {
    Playing(Option<u64>),
    Paused(Option<u64>),
}

/// Handle to the Windows media-controls thread. Dropping it leaves the
/// detached thread running for the process lifetime (same as the daemon).
pub struct WindowsMediaControls {
    proxy: EventLoopProxy<ControlUpdate>,
}

impl WindowsMediaControls {
    /// Spawn the hidden-window thread and block until the souvlaki
    /// controls are attached (or fail). Returns once the proxy is live.
    pub fn new(
        bus_name: String,
        commands_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    ) -> Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<EventLoopProxy<ControlUpdate>, String>>();

        std::thread::Builder::new()
            .name("spotuify-smtc".to_string())
            .spawn(move || run_event_loop(bus_name, commands_tx, ready_tx))
            .context("failed to spawn Windows media-controls thread")?;

        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(proxy)) => Ok(Self { proxy }),
            Ok(Err(message)) => Err(anyhow!(message)),
            Err(_) => Err(anyhow!(
                "Windows media-controls thread did not become ready"
            )),
        }
    }

    pub fn send(&self, update: ControlUpdate) {
        if self.proxy.send_event(update).is_err() {
            tracing::warn!("Windows media-controls event loop is gone; dropping update");
        }
    }
}

struct SmtcApp {
    bus_name: String,
    commands_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    ready_tx: Option<mpsc::Sender<Result<EventLoopProxy<ControlUpdate>, String>>>,
    proxy: EventLoopProxy<ControlUpdate>,
    controls: Option<souvlaki::MediaControls>,
    // Keep the hidden window alive for the SMTC's lifetime.
    _window: Option<Window>,
}

impl ApplicationHandler<ControlUpdate> for SmtcApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.controls.is_some() {
            return; // already initialised
        }
        let Some(ready_tx) = self.ready_tx.take() else {
            return;
        };
        match self.build_controls(event_loop) {
            Ok((window, controls)) => {
                self._window = Some(window);
                self.controls = Some(controls);
                let _ = ready_tx.send(Ok(self.proxy.clone()));
            }
            Err(err) => {
                let _ = ready_tx.send(Err(format!("{err:#}")));
                event_loop.exit();
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, update: ControlUpdate) {
        let Some(controls) = self.controls.as_mut() else {
            return;
        };
        match update {
            ControlUpdate::Metadata {
                title,
                artist,
                album,
                cover_url,
                duration_ms,
            } => {
                let metadata = souvlaki::MediaMetadata {
                    title: title.as_deref(),
                    artist: artist.as_deref(),
                    album: album.as_deref(),
                    cover_url: cover_url.as_deref(),
                    duration: duration_ms.map(Duration::from_millis),
                };
                if let Err(err) = controls.set_metadata(metadata) {
                    tracing::warn!(error = %err, "SMTC metadata update failed");
                }
            }
            ControlUpdate::Playback(kind) => {
                let playback = match kind {
                    PlaybackKind::Playing(ms) => souvlaki::MediaPlayback::Playing {
                        progress: ms.map(|m| souvlaki::MediaPosition(Duration::from_millis(m))),
                    },
                    PlaybackKind::Paused(ms) => souvlaki::MediaPlayback::Paused {
                        progress: ms.map(|m| souvlaki::MediaPosition(Duration::from_millis(m))),
                    },
                };
                if let Err(err) = controls.set_playback(playback) {
                    tracing::warn!(error = %err, "SMTC playback update failed");
                }
            }
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

impl SmtcApp {
    fn build_controls(
        &self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(Window, souvlaki::MediaControls)> {
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_title("spotuify-media-controls"),
            )
            .context("failed to create hidden SMTC window")?;

        let RawWindowHandle::Win32(handle) = window
            .window_handle()
            .context("hidden window has no handle")?
            .as_raw()
        else {
            anyhow::bail!("hidden window did not yield a Win32 handle");
        };
        let hwnd = handle.hwnd.get() as *mut std::ffi::c_void;

        let mut controls = souvlaki::MediaControls::new(souvlaki::PlatformConfig {
            display_name: "spotuify",
            dbus_name: &self.bus_name,
            hwnd: Some(hwnd),
        })
        .context("failed to create souvlaki SMTC controls")?;

        let commands_tx = self.commands_tx.clone();
        controls
            .attach(move |event| {
                if let Some(command) =
                    souvlaki_event_to_action(event).and_then(map_media_control_event)
                {
                    let _ = commands_tx.send(command);
                }
            })
            .context("failed to attach SMTC controls")?;

        Ok((window, controls))
    }
}

fn run_event_loop(
    bus_name: String,
    commands_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    ready_tx: mpsc::Sender<Result<EventLoopProxy<ControlUpdate>, String>>,
) {
    let event_loop = match EventLoop::<ControlUpdate>::with_user_event()
        .with_any_thread(true)
        .build()
    {
        Ok(event_loop) => event_loop,
        Err(err) => {
            let _ = ready_tx.send(Err(format!("failed to build SMTC event loop: {err}")));
            return;
        }
    };
    let proxy = event_loop.create_proxy();
    let mut app = SmtcApp {
        bus_name,
        commands_tx,
        ready_tx: Some(ready_tx),
        proxy,
        controls: None,
        _window: None,
    };
    if let Err(err) = event_loop.run_app(&mut app) {
        tracing::warn!(error = %err, "SMTC event loop exited");
    }
}
