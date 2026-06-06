---
title: "Install"
description: "Install spotuify, set config, login, and verify playback."
---

Install `spotuify`, log in, then run `doctor` before you trust playback.

## Requirements

- Spotify account. Premium is required for local playback through the embedded librespot device (`spotuify-hume`).
- A terminal. Kitty or iTerm2 gives better cover art, but the app has text fallbacks.

```bash
spotuify --help
```

## Homebrew

```bash
brew tap planetaryescape/spotuify
brew trust --formula planetaryescape/spotuify/spotuify
brew install planetaryescape/spotuify/spotuify
spotuify --help
```

To update an existing Homebrew install:

```bash
brew update
brew upgrade planetaryescape/spotuify/spotuify
```

`brew trust --formula` keeps installs working when Homebrew tap-trust checks are enabled for third-party taps.

Release archives include SHA256 checksums and GitHub artifact provenance attestations. macOS binaries are not notarized yet, so Gatekeeper may still ask you to approve the first launch.

## Install script

For macOS and Linux x86_64 release archives, the installer downloads both the archive and its published `.sha256` file before installing:

```bash
curl -fsSLO https://raw.githubusercontent.com/planetaryescape/spotuify/main/install.sh
bash install.sh
spotuify --help
```

## Cargo

```bash
cargo install --git https://github.com/planetaryescape/spotuify --locked
spotuify --help
```

## Windows x64

Download `spotuify-v*-windows-x86_64.zip` from GitHub Releases, unzip it, put `spotuify.exe` on your `PATH`, then run:

```powershell
spotuify.exe --help
spotuify daemon install-service
```

Windows binaries are beta until login, daemon startup, playback, and Task Scheduler install are verified on a real Windows machine.

From this repo:

```bash
cargo build --release
./target/release/spotuify --help
```

## Configure Spotify

`spotuify` is BYO Spotify app GA: the supported GA setup is for users who can create their own Spotify Developer app. It is not broad consumer no-developer setup yet; that would require a reviewed/shared Spotify app or a product decision to make first-party/keymaster auth the default.

Create a Spotify Developer app at the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard) with redirect URI `http://127.0.0.1:8888/callback`, then add its client id to your config during onboarding. A client secret is optional for PKCE. Premium is required for playback.

The first-party/keymaster flow still exists for experiments, but it is opt-in with `SPOTUIFY_USE_FIRST_PARTY=1`.

## Login

```bash
spotuify login
spotuify doctor
```

What you get: a browser opens, you approve, and the OAuth token is stored in the local auth file under the app config directory. The doctor report tells you whether auth, daemon, device visibility, and Spotify API access work.

## Set your Spotify app

Set the client id in config, or export it before logging in:

```bash
export SPOTUIFY_CLIENT_ID=your-app-client-id
spotuify login
```

Apps in Spotify's Development Mode can be limited by Spotify policy. Apply for Extended Quota Mode if playlist or library writes return `403`.

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
