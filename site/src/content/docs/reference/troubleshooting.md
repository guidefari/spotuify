---
title: "Troubleshooting"
description: "Fix auth, daemon, device, cache, search, lyrics, and visualizer issues."
---

Start with `doctor`. It is the shortest path to useful failure data.

## Run doctor

```bash
spotuify doctor
spotuify doctor --format json
```

## Daemon unavailable

```bash
spotuify daemon status
spotuify daemon start
spotuify logs tail 200
```

If a script must fail instead of starting the daemon:

```bash
spotuify --no-daemon-start status
```

## Auth failure

```bash
spotuify config get client_id
spotuify config get redirect_uri
spotuify login
```

If you changed app credentials, login again.

## Permissions out of date

The TUI shows the banner *"Spotify permissions out of date. Quit,
run `spotuify logout && spotuify login`, then restart."* when your
stored token was issued before a scope that newer features require
(like follow/unfollow or playlist add). The fix is exactly what the
banner says:

```bash
spotuify logout
spotuify login
```

```bash
spotuify status
```

The status line under `tokens.scopes_missing` lists which scopes you
still need to grant on next login.

## macOS keychain prompt storm

Each cold start of `spotuify` (or `spotuify daemon`) reads your Spotify
OAuth token from the macOS Keychain. On a fresh binary the system
prompts for approval; the in-memory token cache only deduplicates
within a single process.

To kill the prompts on a binary you trust:

- Click **Always Allow** the next time macOS prompts for that exact
  binary. The grant is bound to the binary identity, so it survives
  daemon restarts but resets when you rebuild from source.

For local development and tests:

```bash
# Skip the proactive scope-drift check at startup (one fewer
# keychain hit per cold start; the first real API call still
# reads the token).
SPOTUIFY_SKIP_KEYCHAIN_ON_START=1 spotuify daemon start
```

`SPOTUIFY_FAKE_SPOTIFY=1` already implies the skip. Fake-mode runs
never touch the keychain.

## No active device

```bash
spotuify devices
spotuify transfer spotuify-hume
spotuify play "imagine dragons"
```

If the device list is empty, start the daemon and reconnect:

```bash
spotuify daemon restart
spotuify reconnect
spotuify devices
```

## Search looks empty

```bash
spotuify sync library
spotuify cache status --format json
spotuify reindex
spotuify search "test" --source local
```

## Cache looks broken

```bash
spotuify cache repair
spotuify cache status
```

Last resort:

```bash
spotuify cache reset --confirm
spotuify sync all
```

## Lyrics are missing

```bash
spotuify lyrics show
spotuify lyrics fetch spotify:track:...
spotuify lyrics offset spotify:track:... +50ms
```

Lyrics depend on configured providers and cache state. Spotify Web API itself does not guarantee lyrics.

## Visualizer is blank

```bash
spotuify viz status --format json
spotuify viz source auto
spotuify viz enable
```

On macOS loopback capture needs a virtual device such as BlackHole unless the embedded sink tap is active.

## Bug report

```bash
spotuify bug-report --log-lines 500 --output spotuify-report.tar.gz
```

The bundle is local. Inspect it before sharing.

## See Also

- [Player and Daemon](/guides/player-and-daemon/)
- [Config](/reference/config/)
- [IPC Protocol](/reference/ipc/)
