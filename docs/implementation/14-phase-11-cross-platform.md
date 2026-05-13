# Phase 11 - Cross-Platform and Install Story

## Goal

Make spotuify installable on Linux and Windows, not just macOS-with-apple-native-keyring. Ship signed, installable artifacts so the README quickstart is actually one command per platform.

## Evidence base

- ncspot CI matrix: ubuntu-latest, ubuntu-24.04-arm, macos-14, windows-latest. Each gets a different audio backend default.
- spotatui CD matrix: x86_64-linux-gnu, x86_64-apple-darwin (Intel runner), aarch64-apple-darwin, x86_64-pc-windows-msvc. Per-target audio feature sets baked in. `cargo-deb` for Debian. AUR + Homebrew publishing scripts in CI.
- spotify-player: Windows/macOS quirk — souvlaki on those platforms needs a real window handle; they create a hidden winit window. Daemon mode is incompatible with souvlaki on those platforms (exit 1 documented).
- ncspot moved IPC socket from cache dir → runtime dir in v1.0.0 because sockets in cache dir = staleness.
- ncspot/spotatui both use `librespot::cache::Cache` for credential persistence — no keyring. spotuify should be different: use OS-native credential storage.

## Deliverables

### Keyring per platform
- `keyring` crate with platform-conditional features:
  - macOS: `apple-native`
  - Linux: `linux-native-sync-persistent` (Secret Service via DBus; requires GNOME Keyring or KWallet)
  - Windows: `windows-native` (Credential Manager)
- Bounded read/write timeouts (already implemented at 20s for mac; extend to other platforms).
- Fall-through: if Secret Service unavailable on Linux (headless server), allow encrypted file fallback under `~/.local/share/spotuify/credentials.encrypted` with explicit `--allow-file-credentials` opt-in.
- Document the GNOME Keyring / KWallet prerequisite in the Linux quickstart.

### Socket paths
- macOS: `~/Library/Application Support/spotuify/spotuify.sock`
- Linux: `$XDG_RUNTIME_DIR/spotuify/spotuify.sock`, fallback `/run/user/$uid/spotuify/`, fallback `/tmp/spotuify-$uid/`
- Windows: Named Pipe `\\.\pipe\spotuify-{user}` (preferred); TCP loopback on a unique port as alternative if named-pipe support proves problematic. Port recorded at `%LOCALAPPDATA%\spotuify\port` with a bearer-token auth file.
- Never put sockets in cache dir (ncspot's lesson learned).
- Multi-instance support: if existing socket is responsive, new daemon refuses to start; if stale, deletes and takes over. PID file at `<sock>.pid` for ownership detection.

### Audio backend per platform (cross-reference Phase 9)

| Target | Default audio backend | System deps |
|---|---|---|
| `x86_64-unknown-linux-gnu` | alsa | libasound2-dev, libpulse-dev (for optional pulse), libpipewire-0.3-dev (for optional pipewire) |
| `aarch64-unknown-linux-gnu` | alsa | same |
| `x86_64-unknown-linux-musl` | rodio | none extra |
| `aarch64-apple-darwin` | portaudio | none extra (CoreAudio via portaudio) |
| `x86_64-apple-darwin` | portaudio | none extra |
| `x86_64-pc-windows-msvc` | rodio | none extra |

Pulse env vars (Linux only) set before librespot init for nice pavucontrol display:
```rust
std::env::set_var("PULSE_PROP_application.name", "spotuify");
std::env::set_var("PULSE_PROP_application.icon_name", "spotuify");
std::env::set_var("PULSE_PROP_stream.description", "Spotify (spotuify)");
```

### Daemon supervision
- macOS LaunchAgent: `install/launchd/dev.spotuify.daemon.plist`. Loaded via `launchctl bootstrap gui/$(id -u)`.
- Linux systemd user unit: `install/systemd/user/spotuify-daemon.service`. Enabled via `systemctl --user enable --now spotuify-daemon`.
- Windows: Task Scheduler XML in `install/windows/spotuify-daemon-task.xml`. Optionally explore `service-manager` crate for native Windows Service.
- `spotuify daemon install-service` and `spotuify daemon uninstall-service` subcommands handle the platform-appropriate registration.

### Souvlaki / system media controls (cross-reference Phase 14)
- Linux: works in daemon mode (no window handle needed for D-Bus MPRIS).
- macOS: requires AppKit `NSApplication.shared` event loop. If daemon is detached and there's no TUI front-end alive, MPRIS-equivalent unavailable. Strategy: route media-key events through the daemon-aware MPRIS layer only when a UI process is alive; for headless daemon, skip media controls and document.
- Windows: SMTC requires a window handle. Same strategy as macOS — hidden window only when a UI is up.

### Cross-compilation & releases
- Use `cargo dist` or hand-rolled `cargo build --target <triple>` matrix in CI.
- Targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`, `x86_64-pc-windows-msvc`.
- Each release artifact is a tarball/zip with: `spotuify` binary, `LICENSE`, `README.md`, `install/` directory with platform service files, completions for bash/zsh/fish/PowerShell.
- macOS: codesign + notarize via Apple Developer account. The non-notarized binary triggers Gatekeeper warnings.
- Linux musl build for portability (alpine, NixOS, containers).
- Windows: signed with self-signed cert at minimum; document SmartScreen workaround. Long-term: EV cert.

### Distribution channels
- **Homebrew tap**: separate repo `bhekanik/homebrew-tap`, auto-bumped via `release-please` workflow.
- **AUR package**: `spotuify-bin` (binary) + `spotuify` (source). PKGBUILD in a separate AUR repo.
- **Scoop manifest**: `spotuify` in a separate `bhekanik/scoop-bucket` repo.
- **Nix flake**: `flake.nix` in main repo following spotatui pattern.
- **cargo-deb**: Debian package via CI for Ubuntu/Debian users.
- **GitHub Releases**: source of truth for all artifacts; checksums + signature files attached.
- Document `cargo install spotuify` works for developers who want from-source.

### CLI completions and man pages
- `spotuify generate completions bash|zsh|fish|powershell|elvish` (clap-derived).
- `spotuify generate man-page` outputs man-page source.
- Built into release artifacts under `install/completions/` and `install/man/`.

### release-please integration
- Existing `.release-please-manifest.json` and `release-please-config.json` get wired to publish artifacts.
- `cargo dist init` integrates with release-please.
- Each release bumps version in `Cargo.toml`, generates changelog, builds matrix, uploads artifacts, bumps Homebrew tap.

## Platform-specific gotchas

### Linux
- Secret Service required; document GNOME Keyring / KWallet prerequisite or `--allow-file-credentials`.
- `XDG_RUNTIME_DIR` may be unset on minimal systems; fall back to `/run/user/$uid/` then `/tmp/`.
- PipeWire is now ubiquitous on modern distros; alsa-backend works through pipewire-alsa compatibility shim by default.

### Windows
- Spotify PKCE redirect: `http://127.0.0.1:<port>/callback` works. NOT `localhost`.
- No `fork`; daemon backgrounding via `CreateProcess(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)`.
- Console UTF-8: emit even on legacy terminals (call `SetConsoleOutputCP(CP_UTF8)` at startup).
- Antivirus false-positives common for new binaries — submit to Microsoft Defender exclusion list / get EV cert eventually.
- ANSI color: `colorchoice-cli` handles 16/256/truecolor detection.

### macOS
- Already primary platform; do not regress.
- Apple Silicon vs Intel: separate binaries.
- App bundle (`.app`) optional — useful only if shipping a GUI shim that opens Terminal.
- HomeKit/Bluetooth audio quirks: rely on RecoveringSink (Phase 9) for resilience.

## Work items

1. Audit every `keyring`, `dirs`, `directories` call site; gate features per `target_os`.
2. Refactor socket path resolution into `crates/spotuify-daemon/src/paths.rs`. Add Windows named-pipe code path.
3. Add Pulse env vars in `spotuify-player::embedded` init (Linux-only `#[cfg]`).
4. Author launchd plist, systemd unit, Windows Task XML. Add `daemon install-service`/`uninstall-service` subcommands.
5. Set up `cargo dist` or equivalent matrix in `.github/workflows/release.yml`.
6. CI matrix: ubuntu-latest, ubuntu-latest-arm64, ubuntu-musl, macos-14 (apple silicon), macos-13 (intel), windows-2022.
7. Apple Developer signing key setup; codesign + notarize in CI via env-stored credentials.
8. Homebrew tap repo + auto-bump action.
9. AUR PKGBUILD repo + maintenance docs.
10. Scoop bucket repo + manifest.
11. Nix flake.
12. cargo-deb integration.
13. Per-platform quickstart sections in README rewritten and verified on clean VMs.
14. Document `--allow-file-credentials` for headless Linux servers.
15. Document the Windows daemon-mode media-key limitation in the troubleshooting section.

## Verification

- Ubuntu 24.04 fresh install: `apt-get install spotuify` (via .deb) works, `spotuify doctor` clean, `spotuify login` round-trips through Secret Service.
- Fedora 42: same via dnf or `cargo install`.
- Arch Linux: `yay -S spotuify-bin` works.
- macOS Sequoia (M1 + Intel): `brew install bhekanik/tap/spotuify` works, `spotuify login` round-trips to Keychain.
- Windows 11: `scoop install spotuify` works, Credential Manager round-trip succeeds, named-pipe IPC works.
- Headless Linux server (no DBus): `spotuify --allow-file-credentials login` works.
- `systemctl --user start spotuify-daemon` on Linux → daemon running, survives logout if `loginctl enable-linger`.
- `launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/dev.spotuify.daemon.plist` on macOS → daemon running.
- A GH Release tagged via release-please produces all six binary artifacts with valid checksums and codesigning where applicable.
- `spotuify generate completions zsh > _spotuify && fpath=(. $fpath)` makes tab-completion work in a fresh zsh.

## Definition of done

A v0.2.0 release is published with installable artifacts for mac/linux/win/musl. Each platform's native credential store works. The README quickstart is verifiably reproducible on a fresh machine for each platform. systemd/launchd/Task Scheduler integration documented and tested.
