# Security Best Practices Report

Date: 2026-05-27

Scope: full `spotuify` repository with emphasis on public software distribution, the static docs site, release automation, local daemon/IPC, credential handling, and agent/MCP surfaces.

Rubric: [`docs/security-audit-rubric.md`](docs/security-audit-rubric.md)

## Executive Summary

No obviously malicious behavior was found. The codebase has several good security controls: tokens are primarily stored in OS credential storage, first-party bearer tokens are not persisted, OAuth uses state validation, MCP HTTP is loopback-only with bearer auth, IPC frames are bounded, destructive MCP tools are confirmation-gated, and release artifacts have SHA256 checksums.

The 2026-05-27 remediation pass fixed the main public-distribution audit flags that could be addressed in-repo:

- The high-severity docs-site npm advisory is resolved; `npm audit --omit=dev --audit-level=moderate` now reports 0 vulnerabilities.
- RustSec advisory checks are now gated with `cargo deny check advisories`; the two remaining transitive advisories are explicit `deny.toml` accepts with rationale.
- Optional dev-app `client_secret` reads are redacted by default and config files are written `0600` on Unix.
- `spotuify auth bearer` now requires `--reveal-secret` before printing a live bearer.
- Custom OAuth redirect URIs now reject non-loopback hosts.
- Cover-art fetches now reject oversized bodies before decode.
- The Vercel site has security headers and the inline font `onload` was removed.
- `/site` npm updates and audit/build checks are wired into Dependabot/CI.
- Release archives now get GitHub artifact provenance attestations alongside SHA256 checksums.
- `SECURITY.md` now documents private vulnerability reporting.

Remaining accepted risks:

- macOS binaries are still not notarized/codesigned in this repo pass. The README now states that honestly and points at checksums/provenance.
- `rsa` via `librespot-core` and `instant` via `tantivy` remain transitive upstream issues; both are tracked in `deny.toml` instead of silently failing audits.

## Audit Commands Run

```sh
npm audit --omit=dev --audit-level=moderate
npm audit fix --dry-run --omit=dev
npm ls devalue --all
npm outdated --depth=0
cargo deny check advisories
cargo deny check bans sources licenses
cargo tree -i rsa --locked
cargo tree -i instant --locked
cargo search tantivy --limit 3
cargo search librespot-core --limit 3
rg secret/token/key patterns across source/docs/scripts
rg browser-dangerous sinks across site source/public/scripts
```

Post-fix verification also ran:

```sh
npm audit --omit=dev --audit-level=moderate
npm run build
cargo fmt --all -- --check
cargo deny check advisories
INSTA_UPDATE=always scripts/cargo-test --test cli_help
scripts/cargo-test --test cli_exit_codes
scripts/cargo-test -p spotuify-spotify --lib
scripts/cargo-test -p spotuify-system --lib
```

Notes:

- `cargo audit` is not installed locally.
- `cargo deny check bans sources licenses` was used for initial discovery, but this pass only gates `advisories`; a full license policy would need a separate allowlist review.

## Rubric Assessment

| Rule | Status | Notes |
| --- | --- | --- |
| SEC-01 Secrets and credential handling | Pass | Keychain/token mirror is good; `client_secret` and bearer printing now require explicit reveal intent. |
| SEC-02 Public distribution trust | Partial | SHA256/Homebrew checksums and provenance exist; macOS notarization/signing remains external release-ops work. |
| SEC-03 Dependency and supply chain | Partial | npm advisory fixed; npm/site audit added; RustSec transitive advisories explicitly accepted pending upstream upgrades. |
| SEC-04 Local trust boundaries and IPC | Mostly pass | Unix socket IPC, loopback MCP HTTP, bearer auth, origin checks, bounded frames, and loopback OAuth redirect binding. Socket permissions should still be verified at runtime. |
| SEC-05 Command execution and hooks | Mostly pass | Main hook path uses argv execution and is opt-in; legacy unused shell executor exists but appears not wired. |
| SEC-06 Filesystem, paths, permissions | Mostly pass | Token mirror and config writes use 0600 on Unix; cache reset is explicit. |
| SEC-07 Network and external API safety | Mostly pass | Reqwest clients generally use timeouts and TLS defaults; cover cache has body-size validation. |
| SEC-08 Static site/browser security | Mostly pass | No custom XSS sinks found; Vercel security headers added. CSP still allows inline script/style because Starlight emits inline scripts/styles. |
| SEC-09 Import/export/diagnostics | Mostly pass | Bug reports redact config and require manual sharing; logs are included raw. |
| SEC-10 Abuse resistance/user consent | Mostly pass | Mutations are preview/confirm oriented; transport actions remain intentionally scriptable. |
| SEC-11 Rust memory-safety posture | Mostly pass | Workspace denies unsafe code; one Windows-only FFI `unsafe` block is narrow. |
| SEC-12 Auditability/release readiness | Mostly pass | `SECURITY.md`, advisory gate, npm/site gate, provenance, and rubric are present; notarization remains open. |

## Critical Findings

None found.

## High Findings

### S-001: Resolved high npm advisory in static site dependency tree

- Rule ID: SEC-03, SEC-08
- Severity: High
- Location: `site/package-lock.json:2068`, `site/package-lock.json:2088`
- Evidence:
  - `site/package-lock.json:2068` pins `astro` at `6.3.0`.
  - `site/package-lock.json:2088` pulls `devalue` via `^5.6.3`.
  - `npm ls devalue --all` shows `astro@6.3.0 -> devalue@5.8.0`.
  - `npm audit` reports `GHSA-77vg-94rm-hx3p`, high severity DoS in `devalue 5.6.3 - 5.8.0`.
- Impact: public site builds ship with a dependency advisory that automated distribution/security checks can flag. Runtime exploitability depends on whether vulnerable deserialization paths are reachable in the generated site, but the audit flag is real.
- Fix: run `npm audit fix` in `site/`; dry-run reports `devalue 5.8.0 => 5.8.1`.
- Mitigation: add npm audit to CI and Dependabot npm updates for `/site`.
- False positive notes: current site appears mostly static; this may be a low practical exploit path, but scanner severity remains high.
- Remediation status: fixed in `site/package-lock.json`; `npm audit --omit=dev --audit-level=moderate` now passes.

## Medium Findings

### S-002: Accepted RustSec vulnerability in transitive `rsa` dependency

- Rule ID: SEC-03
- Severity: Medium
- Location: `Cargo.lock:4842`
- Evidence:
  - `Cargo.lock:4842` pins `rsa 0.9.10`.
  - `cargo deny check advisories` reports `RUSTSEC-2023-0071`, Marvin timing side-channel.
  - `cargo tree -i rsa --locked` traces it through `librespot-core v0.8.0`.
- Impact: automated Rust advisory checks fail. The advisory’s own workaround says local non-compromised use is fine; spotuify is not exposing an RSA private-key oracle over a public network in normal use, so practical runtime risk appears limited. Still an audit flag.
- Fix: monitor/upgrade `librespot` when it removes or fixes the vulnerable dependency. `cargo search librespot-core` currently shows `0.8.0` as latest.
- Mitigation: document a temporary advisory exception only if you accept the risk, with rationale tied to local-only use.
- False positive notes: this is a real advisory, but likely not remotely exploitable through spotuify’s public surfaces as currently designed.
- Remediation status: accepted in `deny.toml` with rationale pending upstream `librespot-core` upgrade.

### S-003: Remediated optional Spotify dev-app client secret exposure

- Rule ID: SEC-01, SEC-06
- Severity: Medium
- Location:
  - `crates/spotuify-spotify/src/config.rs:740`
  - `crates/spotuify-spotify/src/config.rs:755`
  - `crates/spotuify-spotify/src/config.rs:784`
  - `crates/spotuify-spotify/src/config.rs:790`
  - `crates/spotuify-spotify/src/config.rs:984`
  - `crates/spotuify-spotify/src/config.rs:990`
  - `src/main.rs:1901`
  - `src/main.rs:1903`
  - `README.md:653`
- Evidence:
  - `get_config_value(ConfigKey::ClientSecret)` returns the raw secret at `config.rs:755`.
  - `spotuify config get <key>` prints returned values directly at `src/main.rs:1901-1903`.
  - `set_config_value(ConfigKey::ClientSecret, ...)` writes into `spotuify.toml` at `config.rs:790`.
  - config writes use `fs::write` at `config.rs:990`, so Unix permissions depend on umask.
  - README states dev-app credentials live in `~/.config/spotuify/spotuify.toml` or env at `README.md:653`.
- Impact: users who opt into a developer Spotify app can leave `client_secret` in a normal config file and reveal it through `spotuify config get client_secret`. This is weaker than the keychain-backed token design and can be flagged in audits.
- Fix: prefer env-only for `client_secret`, or store it in keychain. At minimum: write config files as 0600 on Unix, redact `config get client_secret` unless `--reveal-secret` is provided, and remove `client_secret` from the default template.
- Mitigation: update docs to recommend `SPOTUIFY_CLIENT_SECRET` over config storage.
- False positive notes: default first-party login does not require this secret; impact applies to optional dev-app flow.
- Remediation status: `config get client_secret` now prints `<redacted>` unless `--reveal-secret` is passed; config writes use 0600 on Unix and docs recommend `SPOTUIFY_CLIENT_SECRET`.

### S-004: Remediated default live Web API bearer printing

- Rule ID: SEC-01, SEC-04
- Severity: Medium
- Location:
  - `src/main.rs:479`
  - `src/main.rs:491`
  - `src/main.rs:2484`
  - `src/main.rs:2506`
  - `src/main.rs:2509`
  - `crates/spotuify-protocol/src/lib.rs:272`
  - `crates/spotuify-protocol/src/lib.rs:787`
  - `crates/spotuify-daemon/src/handler.rs:1313`
- Evidence:
  - CLI subcommand is explicitly “Print the daemon's current Spotify Web API bearer token” at `src/main.rs:479`.
  - JSON and plain output print the token at `src/main.rs:2506` and `src/main.rs:2509`.
  - IPC protocol exposes `Request::WebApiToken` at `protocol/src/lib.rs:272`.
  - daemon handler returns `state.web_api_bearer(force)` at `handler.rs:1313`.
- Impact: any process running as the user and able to connect to the local daemon can extract a full Web API bearer and use it outside spotuify. Same-user local compromise already has significant power, but this expands the blast radius from “control my local player” to “exfiltrate a reusable Spotify API token until expiry.”
- Fix: gate `spotuify auth bearer` behind an explicit scary flag such as `--reveal-secret`, add an interactive confirmation when stdout is a terminal, and consider disabling `WebApiToken` over generic IPC except for doctor/internal paths.
- Mitigation: keep bearer TTL short, never expose this through MCP tools/resources, and document it as a developer-only diagnostic.
- False positive notes: this command is intentionally useful for debugging. The concern is public distribution posture and same-user malware behavior, not remote exploitability.
- Remediation status: `spotuify auth bearer` now fails before daemon startup unless `--reveal-secret` is provided.

### S-005: Partially remediated release artifact trust gap

- Rule ID: SEC-02, SEC-12
- Severity: Medium
- Location:
  - `README.md:63`
  - `README.md:66`
  - `.github/workflows/release.yml:170`
  - `.github/workflows/release.yml:184`
  - `packaging/homebrew/spotuify.rb:9`
  - `packaging/homebrew/spotuify.rb:10`
- Evidence:
  - README says “Binaries are unsigned today” at `README.md:63`.
  - README suggests `xattr -d com.apple.quarantine` at `README.md:66`.
  - release workflow generates SHA256 files at `release.yml:170-176` and uploads them at `release.yml:179-184`.
  - Homebrew formula uses HTTPS URLs plus SHA256 at `packaging/homebrew/spotuify.rb:9-20`.
- Impact: distribution sites and users may treat the app as suspicious even if clean. Removing quarantine is a known trust smell on macOS.
- Fix: codesign and notarize macOS binaries, add GitHub artifact attestations/SLSA provenance, and publish signature files for tarballs. Prefer “verify checksum/signature” docs over quarantine removal.
- Mitigation: keep SHA256 checksums and Homebrew formula validation; document unsigned status honestly until fixed.
- False positive notes: lack of signing is not malware. It is a distribution trust gap.
- Remediation status: release workflow now creates GitHub artifact provenance attestations with `actions/attest@v4`; macOS notarization/signing remains open.

### S-006: Remediated static site security headers gap

- Rule ID: SEC-08
- Severity: Medium
- Location:
  - `site/vercel.json:1`
  - `site/vercel.json:7`
  - `site/astro.config.mjs:29`
  - `site/astro.config.mjs:35`
  - `site/astro.config.mjs:37`
- Evidence:
  - `site/vercel.json` only configures framework/install/build/output at lines `1-7`; no `headers`.
  - Astro config loads Google Fonts at `astro.config.mjs:29-35`.
  - Astro config uses inline `onload` at `astro.config.mjs:37`.
  - grep found no CSP, `X-Frame-Options`, `X-Content-Type-Options`, `Referrer-Policy`, or `Permissions-Policy` config outside the new rubric.
- Impact: security header scanners can flag the public docs site. The inline `onload` makes a strict CSP harder without `unsafe-inline` or a hash.
- Fix: add Vercel headers: CSP, `X-Content-Type-Options: nosniff`, `Referrer-Policy`, `Permissions-Policy`, and `frame-ancestors 'none'` or equivalent. Replace the inline `onload` font optimization or account for it with a CSP hash.
- Mitigation: verify runtime headers after deploy because some may be set by Vercel/project settings outside the repo.
- False positive notes: no custom DOM XSS sinks were found in `site/src`, `site/public`, or `site/scripts`.
- Remediation status: `site/vercel.json` now defines CSP, HSTS, referrer, content-type, frame, and permissions headers; the inline font `onload` was removed.

### S-007: Remediated missing docs-site and Rust advisory automation

- Rule ID: SEC-03, SEC-12
- Severity: Medium
- Location:
  - `.github/dependabot.yml:3`
  - `.github/dependabot.yml:9`
  - `.github/workflows/ci.yml:89`
  - `.github/workflows/ci.yml:333`
- Evidence:
  - Dependabot only covers Cargo and GitHub Actions at `.github/dependabot.yml:3-13`.
  - CI runs Rust checks/builds but no `npm audit`, no site build gate, and no `cargo deny check advisories` gate in visible jobs.
- Impact: known advisories can sit unnoticed until a manual audit. This already happened for `devalue`.
- Fix: add Dependabot npm ecosystem for `/site`, add CI jobs for `npm ci && npm audit --omit=dev --audit-level=high`, and add `cargo deny check advisories` with a checked-in `deny.toml`.
- Mitigation: if a RustSec advisory is accepted temporarily, record an explicit ignore with expiry and rationale.
- False positive notes: Vercel may build the site separately, but that does not replace repo-level audit visibility.
- Remediation status: Dependabot now covers `/site`; CI now runs `npm audit`, `npm run build`, and `cargo deny check advisories`.

## Low Findings

### S-008: Accepted unmaintained `instant` dependency through Tantivy

- Rule ID: SEC-03
- Severity: Low
- Location: `Cargo.lock:2466`
- Evidence:
  - `Cargo.lock:2466` pins `instant 0.1.13`.
  - `cargo deny check advisories` reports `RUSTSEC-2024-0384`, unmaintained crate.
  - `cargo tree -i instant --locked` traces it through `measure_time -> tantivy 0.22.1 -> spotuify-search`.
  - `cargo search tantivy --limit 3` shows latest `tantivy = 0.26.1`.
- Impact: automated audits may flag the dependency as unmaintained. This is not an immediate vulnerability by itself.
- Fix: evaluate upgrading Tantivy from `0.22.1` to a current compatible release.
- Mitigation: document a temporary allow if the upgrade is too large before distribution.
- Remediation status: accepted in `deny.toml` with rationale pending a planned Tantivy upgrade.

### S-009: Remediated OAuth redirect listener loopback enforcement

- Rule ID: SEC-07
- Severity: Low
- Location:
  - `crates/spotuify-spotify/src/auth.rs:1042`
  - `crates/spotuify-spotify/src/auth.rs:1050`
  - `crates/spotuify-spotify/src/auth.rs:1099`
- Evidence:
  - `bind_redirect_listener` parses the configured redirect URI and binds to its host/port at `auth.rs:1042-1050`.
  - OAuth state is validated at `auth.rs:1099-1100`.
  - default redirect is loopback at `crates/spotuify-spotify/src/config.rs:1042-1043`.
- Impact: a user-provided redirect URI like `http://0.0.0.0:8888/callback` could expose the short-lived callback listener beyond loopback. State validation reduces risk, but scanners may still flag a configurable network bind.
- Fix: reject non-loopback redirect hosts unless an explicit advanced flag/environment override is set.
- Mitigation: docs already recommend `127.0.0.1`; keep it that way.
- Remediation status: `bind_redirect_listener` now rejects non-loopback hosts before binding.

### S-010: Remediated cover-art response size cap

- Rule ID: SEC-07, SEC-06
- Severity: Low
- Location:
  - `crates/spotuify-system/src/cover_cache.rs:206`
  - `crates/spotuify-system/src/cover_cache.rs:223`
  - `crates/spotuify-system/src/cover_cache.rs:228`
- Evidence:
  - cover cache fetches arbitrary `url` with `http.get(url)` at `cover_cache.rs:206`.
  - it calls `resp.bytes().await` at `cover_cache.rs:223`.
  - it decodes after the full body is in memory at `cover_cache.rs:228`.
- Impact: if a hostile or corrupted image URL reaches this path, it can force memory use before decode validation. Normal Spotify image URLs make this unlikely.
- Fix: enforce a `Content-Length` max and stream with a hard byte cap before decode; optionally restrict accepted hosts to Spotify image/CDN hosts unless explicitly configured.
- Mitigation: existing timeout and cache max help, but they do not cap a single response body.
- Remediation status: cover-art fetch now validates `Content-Length` when available and rejects oversized bodies after read before decode.

### S-011: Remediated missing security contact/policy

- Rule ID: SEC-12
- Severity: Low
- Location: repository root / `.github`
- Evidence:
  - `find . -maxdepth 3 -name '*SECURITY*'` found no `SECURITY.md`.
- Impact: distribution sites and security reporters have no clear private disclosure path.
- Fix: add `SECURITY.md` with supported versions, disclosure contact, expected response window, and note that Spotify tokens should never be pasted into issues.
- Remediation status: `SECURITY.md` added.

## Positive Controls Observed

- Token persistence path documents that first-party Web API bearers are not persisted; only refresh credentials are mirrored, and the disk mirror uses atomic 0600 writes on Unix (`crates/spotuify-spotify/src/auth.rs:323-330`, `auth.rs:384-401`, `auth.rs:645-678`).
- OAuth callback validates `state` (`crates/spotuify-spotify/src/auth.rs:1099-1100`).
- MCP HTTP requires `SPOTUIFY_MCP_TOKEN`, rejects non-loopback binds, validates browser `Origin`, and caps body reads to 1 MiB (`crates/spotuify-mcp/src/http.rs:24-28`, `http.rs:43-50`, `http.rs:72`, `http.rs:114-138`).
- IPC frames are length-delimited with a 16 MiB maximum (`crates/spotuify-protocol/src/lib.rs:1604-1607`).
- Destructive MCP tools require confirmation/preview behavior (`crates/spotuify-mcp/src/tools.rs:163-217`, `crates/spotuify-mcp/src/confirm.rs:50-53`).
- User shell hooks are opt-in by default (`crates/spotuify-spotify/src/config.rs:1095-1098`) and the active dispatcher uses argv-style execution rather than `sh -c` (`crates/spotuify-system/src/hooks.rs:196-203`).
- Browser sink scan found no custom `innerHTML`, `document.write`, `eval`, `new Function`, dangerous `postMessage`, or `dangerouslySetInnerHTML` in site source/public/scripts. The previously found inline font `onload` has been removed.
- Secret regex scan did not find committed private keys, GitHub tokens, OpenAI-style keys, AWS access keys, or JWT-looking literals in source/docs/scripts outside generated/dependency folders.

## Remaining Follow-Up

1. Add macOS codesigning and notarization when Apple Developer credentials are available.
2. Revisit `librespot-core` and Tantivy upgrades periodically to remove the two `deny.toml` advisory exceptions.
3. Consider a stricter future CSP with nonces or hashes if the Starlight inline scripts can be eliminated.
4. Verify daemon socket permissions at runtime on each supported platform.
