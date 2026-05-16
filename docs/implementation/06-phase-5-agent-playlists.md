# Phase 5 - Agent Playlists

> **Note on agent semantics (added Phase 13, 2026-05-14)** —
> `build_playlist_plan` ships as a heuristic scaffold for the
> `spotuify playlist plan` CLI surface; it is *not* a real LLM planner.
> Genuine LLM-driven playlist research happens in the upstream
> agent / MCP-client model:
>
> - Phase 8 ships the spotuify MCP server, exposing tools an LLM can
>   call directly: `search`, `playlist_add`, `playlist_create`,
>   `library_save`, etc., gated by destructive-confirm.
> - Phase 12 ships `undo_last` so a misfired LLM tool call is
>   recoverable.
>
> The local `build_playlist_plan` heuristic is kept for Unix-shell
> composition (`spotuify playlist plan | jq` recipes) and for tests
> that need a deterministic plan generator. Treat it as a stub: do not
> extend it to compete with the MCP client model.

## Goal

Let agents research a theme, resolve tracks, preview a playlist, and create it safely through spotuify CLI.

## Deliverables

- [x] Playlist plan JSON schema: `PlaylistPlan` in `crates/spotuify-protocol/src/agent_playlists.rs`.
- [x] Candidate track resolution command: `spotuify resolve-tracks --from plan.json --format jsonl`.
- [x] Playlist dry-run preview: `spotuify playlist create ... --dry-run`.
- [x] Playlist commit command: `spotuify playlist create ... --yes`.
- [x] Mutation receipts: commit path uses daemon `PlaylistCreate` and returns `PlaylistCreate { receipt }`.
- [x] Recipes for agents: MCP tools expose plan/resolve plus normal playlist mutation tools; local heuristic remains a deterministic shell scaffold.

## Commands

```text
spotuify playlist plan "brief" --format json
spotuify resolve-tracks --from plan.json --format jsonl
spotuify playlist create "Name" --from candidates.jsonl --dry-run
spotuify playlist create "Name" --from candidates.jsonl --yes
```

## Plan schema fields

- title
- description
- target length
- mood
- theme notes
- candidate searches
- sequencing notes
- exclusions

## Resolution requirements

- Deduplicate exact tracks.
- Prefer playable tracks.
- Preserve alternatives.
- Explain confidence.
- Return unresolved items explicitly.

## Safety requirements

- [x] No playlist creation without dry-run unless `--yes` is passed.
- [x] Dry-run and commit use the same resolved candidate set.
- [x] Receipt includes playlist ID/URI and added item count through the daemon mutation receipt path.

## Verification

- `plan_schema_contains_required_agent_playlist_fields`
- `resolution_deduplicates_tracks_prefers_playable_and_marks_unresolved`
- `playlist_create_preview_lists_tracks_unresolved_duplicates_and_mutation`
- `playlist_create_requires_preview_or_explicit_yes`
- `agent_playlist_workflow_commands_parse_from_phase_five_spec`
- MCP tool routing tests for `playlist_plan` and `playlist_resolve_tracks`.

## Definition of done

An agent can create a playlist from a user brief with a previewable, repeatable CLI workflow.
