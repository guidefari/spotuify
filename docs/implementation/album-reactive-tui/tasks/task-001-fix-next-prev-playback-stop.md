---
id: task-001
title: Fix next/previous playback stop regression
status: completed
phase: phase-001
depends_on: []
blocks:
  - task-002
risk:
  level: high
  blast_radius: medium
execution:
  executor_type: frontier_model
  lane: frontier
  preferred_model: pe-default-frontier
  skills:
    - spotuify
    - build-and-fix
    - code-review
scope:
  allowed_paths:
    - crates/spotuify-core/src/**
    - crates/spotuify-daemon/src/**
    - crates/spotuify-player/src/**
    - crates/spotuify-protocol/src/**
    - crates/spotuify-spotify/src/**
    - crates/spotuify-tui/src/**
    - docs/implementation/album-reactive-tui/**
    - scripts/**
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
    - scripts/cargo-test -p spotuify-daemon --tests
    - cargo clippy -p spotuify-tui -p spotuify-daemon -p spotuify-player -p spotuify-spotify --all-targets -- -D warnings
  success_criteria:
    - "Root cause for next/previous stopping playback is identified from code and, where possible, spotuify CLI/log evidence."
    - "Next and previous keep playback active when Spotify returns a valid next/previous track or emits a transient empty playback response."
    - "The fix respects daemon-owned optimistic state; the TUI does not locally mutate playback as a shortcut."
    - "Regression coverage exists for the failing path."
    - "No unrelated refactors or release/version changes."
---

# Fix next/previous playback stop regression

## Goal

Fix the feedback that playback stops whenever the user hits next/previous navigation keys.

## Context

The app was recently changed to keep queue appends optimistic. The same daemon-owned state discipline applies here: the daemon should emit/reconcile transport state and the TUI should render events. Do not patch the TUI by mutating local playback before daemon confirmation.

Use `spotuify` to debug `spotuify`:

- `spotuify status --format json`
- `spotuify queue --format json`
- `spotuify logs tail 200`
- `spotuify next`
- `spotuify previous`

Live playback mutation is allowed for this task because the user explicitly reported a playback-control regression and asked to fix it.

## Work

- Trace TUI next/previous key handling through IPC command dispatch.
- Trace daemon handling through optimistic event emission, Spotify Web API call, playback clock reconciliation, and player/device behavior.
- Fix whichever layer causes playback to become paused/stopped after next/previous.
- Add focused tests around the regression.
- Keep changes inside the allowed paths.

## Review retry note

The first worker attempt correctly identified transient empty Spotify playback snapshots as part of the bug, but the proposed fix dropped every empty Web API poll. Do not make stale playback immortal:

- A transient empty readback immediately around next/previous must not stop playback.
- A genuine no-active-session state must still be able to clear or stop stale playback after an appropriate guard.
- Add regression coverage for both sides: immediate transient empty during next/previous is ignored, while real no-active-session does not leave the old track playing forever.

## Out of scope

- Do not add album-art color theming in this task.
- Do not change playlist product behavior.
- Do not bump version, tag, release, or edit website/download artifacts.
