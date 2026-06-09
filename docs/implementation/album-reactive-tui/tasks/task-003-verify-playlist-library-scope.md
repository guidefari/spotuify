---
id: task-003
title: Verify playlist library scope and harden messaging
status: completed
phase: phase-003
depends_on:
  - task-002
blocks:
  - task-004
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
    - crates/spotuify-daemon/src/**
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
    - scripts/cargo-test -p spotuify-tui --tests
  success_criteria:
    - "Playlists page remains scoped to the user's playlist library."
    - "Owned/followed playlist metadata from `/me/playlists` is not accidentally filtered out by owner-only logic."
    - "Inaccessible Spotify-curated playlist tracks are explained clearly without hiding usable user playlists."
    - "No recommendations/browse behavior is mixed into the Playlists page."
---

# Verify playlist library scope and harden messaging

## Goal

Preserve the intentional Playlists product decision while ensuring followed playlist metadata is not mistakenly excluded.

## Context

Spotify documentation says current-user playlist listings can include owned and followed playlists, subject to scopes. Spotuify should keep Playlists as the user's playlist library, not turn it into a recommendation or browse page.

## Work

- Confirm the current `/me/playlists` mapping does not apply owner-only filtering.
- Check `inaccessible_playlist_ids` behavior and messaging around Spotify-curated / third-party restricted playlists.
- Add or adjust tests/messages only if there is an actual mismatch.

## Out of scope

- Do not add recommendations.
- Do not add a global Spotify Browse page.
- Do not make destructive playlist mutations.
