# Phase 0 - Stabilize Current App

## Goal

Make current Spotify auth/device/search/playback behavior reliable enough to build on.

## Deliverables

- Keep package-scoped `scripts/cargo-test -p <crate> --tests` and
  `cargo clippy -p <crate> --all-targets -- -D warnings` green while
  iterating; run full workspace tests/clippy and release builds at merge or
  release gates.
- `doctor` must complete with bounded timeouts.
- `doctor` must show preferred device visibility.
- TUI input loop must never await Spotify network calls.
- Search must use valid Spotify API params.
- Playback must activate preferred device or show actionable error.

## Work items

1. [x] Audit and retire all keychain calls.
2. [x] Audit all Spotify calls from TUI input path. TUI actions dispatch
   bounded async work instead of awaiting Spotify calls inside key handling.
3. [x] Add CLI command surfaces for search/play/device/status verification.
4. [x] Improve `doctor` device diagnostics:
   - preferred device configured
   - preferred device visible
   - active device
   - restricted devices
5. [x] Improve playback error messages through typed player/backend errors.

## Verification commands

```text
cargo fmt --check
cargo clippy -p <crate> --all-targets -- -D warnings
scripts/cargo-test -p <crate> --tests
cargo build --locked --release \
  --features "embedded-playback system-integrations loopback-cpal <audio-backend>"
./target/release/spotuify doctor
```

Current focused evidence also includes CLI parser/help coverage for
`doctor`, `devices`, `status`, `search`, and playback commands, player
backend timeout tests, and daemon diagnostics tests.

## Definition of done

The current binary can prove auth, device visibility, search validity, and playback control without freezing.
