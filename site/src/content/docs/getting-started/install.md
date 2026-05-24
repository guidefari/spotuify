---
title: "Install"
description: "Install spotuify, set config, login, and verify playback."
---

Install `spotuify`, give it Spotify app credentials, then run `doctor` before you trust playback.

## Requirements

- Spotify account. Premium is required for local playback through the embedded librespot device (`spotuify-hume`).
- A Spotify Developer app with a redirect URI such as `http://127.0.0.1:8888/callback`.
- A terminal. Kitty or iTerm2 gives better cover art, but the app has text fallbacks.

```bash
spotuify --help
```

## Homebrew

```bash
brew tap planetaryescape/tap
brew install spotuify
spotuify --help
```

## Cargo

```bash
cargo install --git https://github.com/planetaryescape/spotuify --locked
spotuify --help
```

From this repo:

```bash
cargo build --release
./target/release/spotuify --help
```

## Configure Spotify

There is nothing to configure to get started. spotuify uses Spotify's first-party login, so there is no Client ID or Client Secret to create or paste. Premium is required for playback.

If you would rather authenticate with your own Spotify Developer app, that is the one case where you set keys (see [Use your own Spotify app](#use-your-own-spotify-app)).

## Login

```bash
spotuify login
spotuify doctor
```

What you get: a browser opens, you approve, and the refresh token is stored in the OS credential vault. The daemon then mints a full-access Web API token from your session. The doctor report tells you whether auth, daemon, device visibility, and Spotify API access work.

## Use your own Spotify app

Optional, and most people should skip it. To authenticate with your own Spotify Developer app instead of the first-party login, create an app at the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard) with redirect URI `http://127.0.0.1:8888/callback`, then set the client id before logging in:

```bash
export SPOTUIFY_CLIENT_ID=your-app-client-id
spotuify login
```

Apps in Spotify's Development Mode cannot create playlists or save tracks (Spotify returns `403`). That restriction is the reason the default login does not use one.

## Start the daemon

```bash
spotuify daemon start
spotuify daemon status --format json
```

Install the platform user service when you want the daemon to survive shell sessions.

```bash
spotuify daemon install-service
```

## First sound

```bash
spotuify devices
spotuify play "imagine dragons" --type track
```

If playback fails with no active device, activate or transfer to the device you want:

```bash
spotuify transfer spotuify-hume
spotuify play "imagine dragons"
```

## See Also

- [First Run](/getting-started/first-run/)
- [Player and Daemon](/guides/player-and-daemon/)
- [Troubleshooting](/reference/troubleshooting/)
