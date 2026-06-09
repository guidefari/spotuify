---
id: task-004
title: Probe Spotify recommendations before adding a page
status: completed
phase: phase-004
depends_on:
  - task-003
blocks: []
risk:
  level: medium
  blast_radius: low
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
    - crates/spotuify-cli/src/**
    - crates/spotuify-daemon/src/**
    - crates/spotuify-protocol/src/**
    - crates/spotuify-spotify/src/**
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
    - scripts/cargo-test -p spotuify-spotify --tests
    - scripts/cargo-test -p spotuify-daemon --tests
    - scripts/cargo-test -p spotuify-cli --tests
    - scripts/cargo-test -p spotuify-tui --tests
  success_criteria:
    - "Live read-only probe determines whether Spotify recommendations works for the current auth mode."
    - "If unsupported/deprecated/inaccessible, no recommendations UI is added; build log records evidence and product decision."
    - "If supported, CLI-first recommendations surface is implemented before any TUI page."
    - "Any recommendations UI is separate from Playlists."
---

# Probe Spotify recommendations before adding a page

## Goal

Determine whether a recommendations page is viable before building it.

## Context

Spotify currently documents `GET /recommendations` but marks it deprecated. Do not make this a core product dependency without a live probe.

## Work

- Use `spotuify auth bearer` and a read-only curl probe, or add a temporary/local probe if needed.
- Record evidence in the build log.
- If the endpoint is unavailable, stop at the evidence and do not add UI.
- If the endpoint works, implement CLI first and only then add a separate TUI page/tab.

## Host evidence

- 2026-06-09: host checked the official Spotify docs; `GET /recommendations` is still listed but marked Deprecated.
- 2026-06-09: host ran one live read-only probe through the prod daemon bearer. Response was HTTP 429 with `API rate limit exceeded` and `retry-after: 9`.
- Do not issue another live recommendations request in this task. Use the recorded evidence to decide whether any code should be added.

## Out of scope

- Do not train or infer ML models from Spotify content.
- Do not mix recommendations into Playlists.
- Do not perform playback or playlist mutations.
