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

If you see `not logged in; run spotuify login`, do exactly that:

```bash
spotuify login
spotuify doctor
```

`spotuify login` opens the browser, and the daemon mints a Web API token
from your stored OAuth credentials. Configure a Spotify Developer app
`client_id` first if the config does not have one yet.

### 403 on playlist writes

If playlist or library writes return `403`, your Spotify app is probably
still in Development Mode. Apply for Extended Quota Mode in the Spotify
dashboard.

```bash
spotuify auth status
spotuify doctor
```

## Permissions out of date

The TUI shows the banner *"Spotify permissions out of date. Quit,
run `spotuify logout && spotuify login`, then restart."* when a token was
issued before a scope that newer features require, like follow/unfollow
or playlist add. The fix is exactly what the banner says:

```bash
spotuify logout
spotuify login
```

## macOS keychain prompt storm

`spotuify` first tries the private auth cache at
`<data_dir>/auth/token.json`. If that file is missing, it falls back to
the macOS Keychain. On a fresh or unsigned binary, macOS may prompt for
approval.

The daemon treats an unanswered Keychain prompt as `AuthRequired`. It
emits one auth event, the notification bridge shows one auth
notification per error kind, and later health checks fail fast without
touching Keychain again. Run `spotuify login` when you are ready to
repair auth.

To kill the prompts on a binary you trust:

- Click **Always Allow** the next time macOS prompts for that exact
  binary. The grant is bound to the binary identity, so it survives
  daemon restarts but resets when you rebuild from source.

If you've run unsigned dev builds repeatedly, each one is a new identity
that **Always Allow** can't pin, so the clicks pile up and can corrupt the
token item's access list, after which even the trusted installed binary
prompts on every ~20s read. Reset it by recreating the token from a
trusted binary:

```bash
spotuify daemon stop
spotuify logout      # deletes the token + its corrupted access list
spotuify login       # recreates a clean item, trusting the installed binary
spotuify daemon start
```

For local development and tests, use fake mode when you do not want any
Keychain access at all:

```bash
SPOTUIFY_FAKE_SPOTIFY=1 spotuify
```

## No active device

```bash
spotuify devices --format json
spotuify transfer spotuify-hume
spotuify play "imagine dragons"
```

The daemon should expose its embedded librespot device even when Spotify's
device registry lags. If the device list is empty, start the daemon and
reconnect:

```bash
spotuify daemon restart
spotuify reconnect
spotuify devices
```

### Can't transfer to an Echo / Alexa speaker

Amazon Echo and other Alexa-controlled speakers appear in `spotuify devices`,
but Spotify's Web API routinely refuses to *start* playback on them from a
third-party client, so `transfer` returns `404 Not found`. Wake the device via
Alexa (or the Spotify app) first, then transfer while it's in an active
session:

```bash
# Start anything on the Echo via Alexa, then:
spotuify transfer "Office Echo"
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
