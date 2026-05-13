# Phase 12 - Operation Log and Undo

## Goal

Every mutation the daemon performs is a recorded operation with a reversal plan, inspectable and rollbackable from the CLI. jj-style. The safety net that makes broad agent / MCP use survivable.

## Evidence base

- **None** of ncspot, spotify-player, or spotatui have an operation log or undo concept. spotify-player just added confirmation popups (commit #966) but no undo.
- ncspot's per-playlist `snapshot_id` (`model/playlist.rs:25`) is the key concurrency token — every reversible operation should record the pre-mutation snapshot_id alongside its reversal plan.
- jj's `op log` + `op undo` is the canonical model in 2026. We adopt it whole.

## Why

- Phase 8 (MCP server) lets an LLM execute mutations directly. Without undo, a misfired tool call is unrecoverable without manual SQL or Spotify-app intervention.
- Phase 6's two-stage receipts already capture mutation intent; this phase persists that captured intent and adds a reversal plan.
- Agent confidence: "I can let it run because I can roll back."

## Deliverables

- `operations` table backed by SQLite.
- `spotuify ops log [--limit] [--since] [--source cli|tui|mcp|agent] [--format]`.
- `spotuify ops show <id> [--format]`.
- `spotuify ops undo [<id>] [--dry-run] [--yes]` (defaults to last reversible operation).
- `spotuify ops redo [<id>]` for re-applying an undone op.
- MCP tool `undo_last` (Phase 8 integration).
- TUI Diagnostics panel: last 20 operations with status + undo affordance bound to `u`.

## Schema

```text
operations
- id                  TEXT PRIMARY KEY    -- uuid v7 for time-orderability
- kind                TEXT                -- queue_add | playlist_add | playlist_create | library_save | transfer | like | unlike | play | pause | seek | volume | ...
- occurred_at_ms      INTEGER
- source              TEXT                -- cli | tui | mcp | agent | daemon-internal
- requester           TEXT                -- optional: agent id, MCP session id, hostname
- subject_uris        JSON                -- list of URIs affected
- reversible          INTEGER             -- 0 | 1
- reversal_plan_json  JSON                -- empty if not reversible
- pre_state_json      JSON                -- snapshot_id, prior values needed for rollback
- status              TEXT                -- pending | succeeded | failed | undone | redone
- receipt_id          TEXT                -- foreign key to Phase 6 receipts table
- error_message       TEXT
- undone_by           TEXT                -- operation id of the undo op
- redone_by           TEXT                -- operation id of redo
```

### `pre_state_json`
Critical for correctness — captures the state immediately before the mutation so undo can be exact. Examples:
- `playlist_add LIST [URIs]`: stores `pre_snapshot_id`, `LIST`. Undo = `playlist_remove LIST [URIs]` with `If-Match: pre_snapshot_id` (if Spotify supports it) or best-effort remove.
- `library_save URI`: stores nothing (idempotent). Undo = `library_unsave URI`.
- `transfer DEVICE_X`: stores `prior_device_id`. Undo = transfer to prior_device_id.
- `playlist_create NAME`: stores `playlist_id` from receipt. Undo = unfollow / delete.
- `like URI`: stores nothing. Undo = unlike.

## Reversibility classification

| Kind | Reversible | Reversal plan |
|---|---|---|
| `queue_add` | yes | remove URI from queue (best-effort; queue is ephemeral) |
| `playlist_add` | yes | playlist_remove items by URI; use stored snapshot_id for concurrency |
| `playlist_create` | yes | unfollow / delete playlist |
| `playlist_remove` | yes | playlist_add items at original positions (stored in pre_state) |
| `playlist_reorder` | yes | reorder back |
| `library_save` track/album/episode | yes | library_unsave |
| `library_unsave` | yes | library_save (re-saves; loses original `added_at` timestamp) |
| `transfer` | yes | transfer back to prior device |
| `like` / `unlike` | yes | inverse |
| `play` (transport) | no | recorded for log only |
| `pause` / `resume` / `next` / `previous` / `seek` / `volume` / `shuffle` / `repeat` | no | transient |

## Concurrency & idempotency

- Every undo records a NEW operation (kind = `undo`, `subject = original op id`).
- Idempotent: undoing twice returns "already undone" error, does not double-revert.
- Conflict detection: if Spotify's current snapshot_id differs from pre_state's, undo errors with a clear message and shows the diff (e.g., "Playlist has 5 new tracks added since this op; undo would remove your additions"). Force-undo via `--force` flag.
- Bulk undo: `spotuify ops undo --since 1h --yes` undoes in reverse chronological order, stops on first failure.

## Work items

1. Migration for `operations` table (uuid v7 PKs).
2. `RecordOperation` trait/wrapper around every mutating Request handler — records pending → updates on completion.
3. Define `ReversalPlan` enum with one variant per reversible kind; serialise to JSON column.
4. Implement `ops undo`: load operation, validate still reversible, execute reversal, record a new operation marked `kind=undo`.
5. Implement `ops redo`: re-execute the original by capturing its forward plan.
6. Conflict detection via stored snapshot_id and pre-state.
7. Hook Phase 6 `MutationAccepted`/`MutationFinished` events to update operation status.
8. TUI "Operations" panel under Diagnostics tab; render last 20; bind `u` to undo last.
9. MCP: expose `undo_last` and `ops_log` tools.
10. Retention: operations log keeps 90d by default; configurable.
11. Add `spotuify ops show --diff` to render a human-readable diff of what undo would do.
12. Document patterns in README and `09-agent-workflows.md`.

## Verification

- `spotuify playlist add LIST URI` → `spotuify ops undo` removes the URI from the playlist; second undo errors clearly with "operation already undone".
- `spotuify library save URI` → undo unsaves; redo re-saves (but loses original `added_at`).
- `spotuify playlist create "Test" --from c.jsonl --yes` → undo unfollows / deletes the playlist.
- Playlist modified externally after our op: undo detects `snapshot_id` mismatch and refuses without `--force`.
- `spotuify ops log --source mcp --since 1h` shows only MCP-originated mutations.
- Undoing an op whose target no longer exists (playlist deleted manually in Spotify) produces a clear error; operation log unchanged.
- TUI `u` keypress undoes last visible op, updates the panel.
- MCP `undo_last` reverts the last destructive op.
- After 100 random mutations and 100 undos: spotuify state matches "no operations performed" baseline.

## Definition of done

After an unattended agent run that misbehaves, `spotuify ops log --source agent --since 1h | xargs -I{} spotuify ops undo {} --yes` fully reverts the session, and the operation log records the reversals. Conflict detection prevents accidentally overwriting external changes. Spotuify ships the only Spotify TUI with a real undo button.
