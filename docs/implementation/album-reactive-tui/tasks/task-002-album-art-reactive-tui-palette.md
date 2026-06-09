---
id: task-002
title: Add album-art-reactive TUI palette
status: completed
phase: phase-002
depends_on:
  - task-001
blocks:
  - task-003
risk:
  level: medium
  blast_radius: medium
execution:
  executor_type: frontier_model
  lane: frontier
  preferred_model: pe-default-frontier
  skills:
    - spotuify
    - build-and-fix
    - ui-skills
    - code-review
scope:
  allowed_paths:
    - crates/spotuify-tui/src/**
    - docs/implementation/album-reactive-tui/**
  blocked_paths:
    - .env*
    - .git/**
    - target/**
    - target-cli/**
    - clients/**
validation:
  test_commands:
    - cargo fmt --check
    - scripts/cargo-test -p spotuify-tui --tests
    - cargo clippy -p spotuify-tui --all-targets -- -D warnings
  success_criteria:
    - "TUI derives a terminal-safe palette from the active cover art without adding extra Spotify API calls."
    - "Palette updates are keyed by current art URL and stale cover fetches cannot install stale colors."
    - "Contrast remains readable for dark, light, monochrome, and high-saturation covers."
    - "Default theme remains stable when there is no cover art, no color support, or palette extraction fails."
    - "Snapshot/unit coverage proves default and dynamic palette rendering."
---

# Add album-art-reactive TUI palette

## Goal

Make the terminal TUI accents react to the active album art colors.

## Context

The TUI already requests cover art through `Request::CoverArt`, decodes it as `image::DynamicImage`, and renders via `ratatui-image`. Piggyback palette extraction on that decoded image. Do not introduce a new Spotify request path for colors.

Candidate files:

- `crates/spotuify-tui/src/app.rs`
- `crates/spotuify-tui/src/ui.rs`
- `crates/spotuify-tui/src/widgets/style.rs`
- `crates/spotuify-tui/src/widgets/album_art.rs`
- `crates/spotuify-tui/src/widgets/spectrum.rs`

## Work

- Add a small palette type for accent, soft accent/background, foreground/readable text, and now-playing rail.
- Extract the palette from decoded cover images using a bounded, deterministic algorithm or a battle-tested crate if adding one is justified.
- Apply dynamic colors through shared style helpers rather than scattering raw `Color::Rgb(...)`.
- Keep Spotify green available as fallback and for brand-specific states where appropriate.
- Avoid flicker on fast track changes by preserving the existing stale URL guard.

## Out of scope

- Do not add macOS GUI theming.
- Do not change queue behavior.
- Do not add recommendations or playlist browsing.
