# Security Policy

## Reporting A Vulnerability

Please report suspected vulnerabilities privately. Use GitHub private vulnerability reporting or a GitHub Security Advisory for `planetaryescape/spotuify` if available.

Do not paste access tokens, refresh tokens, auth files, daemon logs, or bug-report bundles into public issues. If a public issue is the only available channel, describe the impact and reproduction shape without secrets so the maintainer can move the report to a private channel.

## What To Include

- Affected version, install source, and platform.
- Steps to reproduce, expected behavior, and actual behavior.
- Whether the issue exposes credentials, executes commands, changes playback/library state, or affects release artifacts.
- Redacted logs or `spotuify bug-report` output after manual review.

## Supported Versions

Security fixes target the latest released version. Older versions may receive fixes when the issue is severe and a backport is practical.

## Credential Handling

`spotuify` stores long-lived Spotify credentials in a private auth file under the app config directory with restrictive file permissions on Unix. The live Web API bearer is minted by the daemon and is treated as a secret. Commands that reveal secrets require explicit `--reveal-secret` flags.
