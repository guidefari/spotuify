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

- Playlist plan JSON schema.
- Candidate track resolution command.
- Playlist dry-run preview.
- Playlist commit command.
- Mutation receipts.
- Recipes for agents.

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

- No playlist creation without dry-run unless `--yes` is passed.
- Dry-run and commit use same resolved candidate set.
- Receipt includes playlist ID/URI and added item count.

## Definition of done

An agent can create a playlist from a user brief with a previewable, repeatable CLI workflow.
