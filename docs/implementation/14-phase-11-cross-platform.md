# Phase 11 - Cross-Platform and Install Story

## Goal

Make spotuify installable on Linux and Windows, not just macOS-with-apple-native-keyring. Ship signed, installable artifacts so the README quickstart is actually one command per platform.

## Current status on 2026-06-02

This phase doc started as an implementation plan. The shipped slice is narrower
than the original target, and the code/docs should be read with this current
truth:

- Shipped release artifacts: Linux x86_64, macOS Apple Silicon, and macOS Intel
  tarballs from `.github/workflows/release.yml`.
- Shipped channels: GitHub Releases, Homebrew tap, `cargo install --git`, Nix
  flake/source build, and the checksum-verifying `install.sh` path.
- Shipped supervision templates: launchd, systemd user, and Windows Task
  Scheduler XML, wired through `daemon install-service` / `uninstall-service`.
- Not shipped as prebuilt release channels: Windows binaries, Linux musl,
  Linux arm64, AUR, Scoop, and `.deb` packages.
- Not shipped: macOS codesigning/notarization. README documents the Gatekeeper
  quarantine workaround instead.
- Release Please uses `release-type = "simple"` for changelog, manifest, and
  `Cargo.toml`; `.github/workflows/release-lockfile.yml` owns `Cargo.lock`
  synchronization for release PRs.

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
- Fall-through: if Secret Service is unavailable on Linux (headless
  server), fail with a clear message. Encrypted file fallback was
  considered but is not exposed as a stable credential path yet.
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
- Current implementation is a hand-rolled `cargo build --target <triple>` matrix
  in `.github/workflows/release.yml`, not `cargo dist`.
- Current release targets:
  `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, and
  `x86_64-apple-darwin`.
- Each current release artifact is a tarball with the `spotuify` binary,
  `install.sh`, `README.md`, `install/` service templates, and `docs/recipes/`.
- macOS tarballs are not signed or notarized today. README documents the
  Gatekeeper quarantine workaround and points users at checksums/provenance.
- Linux musl, Linux arm64, and Windows remain source-build paths, not published
  binary artifacts.

### Distribution channels
- **Homebrew tap**: separate repo `planetaryescape/homebrew-spotuify`, auto-bumped by the tag-driven release workflow.
- **AUR package**: not shipped yet.
- **Scoop manifest**: not shipped yet.
- **Nix flake**: `flake.nix` in main repo following spotatui pattern.
- **cargo-deb**: not in the current release matrix.
- **GitHub Releases**: source of truth for tarballs; checksums + provenance attestations attached.
- Document `cargo install spotuify` works for developers who want from-source.

### CLI completions and man pages
- `spotuify generate completions bash|zsh|fish|powershell|elvish` (clap-derived).
- `spotuify generate man-page` outputs man-page source.
- Current release artifacts do not bundle generated completion or man-page
  files. Generate them locally from the installed binary when needed.

### release-please integration
- `.release-please-manifest.json` and `release-please-config.json` drive the
  release PR and changelog.
- `release-type = "simple"` updates `CHANGELOG.md`, the manifest, and
  workspace `Cargo.toml`. It does not update `Cargo.lock` by itself.
- `.github/workflows/release-lockfile.yml` runs only on same-repo
  release-please PRs and commits `Cargo.lock` when `cargo update --workspace`
  changes workspace package versions.
- The tag-driven release workflow decides artifact scope with
  `scripts/release_change_scope.sh`; docs-only or metadata-only tags can create
  a GitHub Release without rebuilding binaries or Homebrew.

## Platform-specific gotchas

### Linux
- Secret Service is required for stable Linux credential storage; document the
  GNOME Keyring / KWallet prerequisite and fail clearly when DBus/keyring is
  unavailable. The headless encrypted-file fallback remains planned, not a
  stable CLI flag.
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

1. [x] Audit every `keyring`, `dirs`, `directories` call site; gate features per `target_os`.
2. [x] Centralize path resolution in `spotuify-protocol::paths`. Runtime/socket/cache/config/data paths no longer use cache dir for sockets. Windows named-pipe paths and IPC aliases are pre-staged; the daemon accept loop still needs the final named-pipe wrapper before Windows IPC is fully live.
3. [x] Add Pulse env vars in `spotuify-player::embedded` init (Linux-only `#[cfg]`).
4. [x] Author launchd plist, systemd unit, Windows Task XML. Add `daemon install-service`/`uninstall-service` subcommands.
5. [x] Set up the release matrix in `.github/workflows/release.yml`.
6. [x] Release workflow covers Linux GNU x86_64, macOS arm64, and macOS Intel. Linux musl, Linux arm64, and Windows remain release-matrix follow-ups.
7. [ ] Apple Developer signing key setup; codesign + notarize in CI remains external release-ops work.
8. [x] Homebrew formula generation/update workflow exists. The separate tap repo/token must be provisioned outside this repo.
9. [ ] AUR PKGBUILD repo + maintenance docs are classified as distribution-channel follow-up outside this repo.
10. [ ] Scoop bucket repo + manifest are classified as distribution-channel follow-up outside this repo.
11. [x] Nix flake.
12. [ ] cargo-deb integration in the release matrix.
13. [x] Per-platform quickstart sections in README rewritten. Clean-VM verification remains manual release QA.
14. [x] Headless encrypted-file credentials are deliberately not documented as a stable flag; README says the fallback is planned, not shipped.
15. [x] Document the Windows/macOS daemon-mode media-key limitation in troubleshooting.

## Verification

Current release QA should verify:

- GitHub Release tag produces Linux x86_64, macOS Apple Silicon, and macOS
  Intel tarballs with valid `.sha256` files and provenance attestations.
- Homebrew install/upgrade works from `planetaryescape/spotuify`.
- `cargo install --git https://github.com/planetaryescape/spotuify --tag v{version} --locked spotuify` installs the tagged version.
- `install.sh` installs the Linux x86_64 archive after checksum verification.
- `systemctl --user start spotuify-daemon` on Linux starts the user service.
- `launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/dev.spotuify.daemon.plist` on macOS starts the user service.
- `scripts/cargo-test -p spotuify-mcp default_socket_path_uses_shared_runtime_resolver --quiet` and `scripts/cargo-test -p spotuify-protocol default_socket_path_uses_shared_runtime_resolver --quiet` cover shared socket-path resolution.

AUR, Scoop, Windows prebuilt binaries, `.deb`, Linux musl, and Linux arm64 are
not current release verification gates because those channels are not shipped.

## Definition of done

The shipped Phase 11 slice provides cross-platform credential-store
selection, centralized path resolution, service-file templates, install
commands, a three-target release matrix, Nix/Homebrew/source-build paths, and
README quickstarts. Fully verified signed distribution across every external
channel (Apple notarization, AUR, Scoop, `.deb`, Windows prebuilt binaries,
clean-VM smoke) remains release-operations follow-up rather than core app
functionality.
