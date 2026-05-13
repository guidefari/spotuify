# Agent Playlist Recipes

spotuify is designed so agents use the same CLI as humans. Playlist creation is a preview-and-commit workflow: plan, resolve, dry-run, then commit only after approval.

## Safety Rules

1. Read or generate a plan first.
2. Resolve tracks into JSONL candidates.
3. Always show `playlist create --dry-run` before mutating Spotify.
4. Run `playlist create --yes` only after explicit user approval.
5. Keep `plan.json` and `candidates.jsonl` together so the preview and commit are repeatable.

## Basic Flow

Situation: the user asks for a playlist about exile and returning home.

```bash
spotuify playlist plan "exile and returning home" --format json > plan.json
spotuify resolve-tracks --from plan.json --format jsonl > candidates.jsonl
spotuify playlist create "Exile and Return" --from candidates.jsonl --dry-run
```

After the user approves:

```bash
spotuify playlist create "Exile and Return" --from candidates.jsonl --yes --format json
```

What you get: a mutation receipt with `playlist_id`, `playlist_uri`, and `added_item_count`.

## Edit The Plan Before Resolving

`playlist plan` creates a deterministic scaffold, not an LLM-generated music essay. Agents should edit `candidate_searches` after research.

```json
{
  "title": "Exile and Returning Home",
  "description": "A playlist about exile, distance, and return.",
  "target_length": 12,
  "mood": "longing, resilient, cathartic",
  "theme_notes": ["songs about leaving", "songs about homecoming"],
  "candidate_searches": [
    "homecoming kanye west",
    "the boxer simon garfunkel",
    "california joni mitchell"
  ],
  "sequencing_notes": ["start sparse", "build toward return"],
  "exclusions": ["live versions unless requested"]
}
```

## Filter With jq

Situation: inspect low-confidence or unresolved candidates before previewing.

```bash
jq -r 'select(.status != "resolved" or .confidence < 0.7) | [.status, .query, .reason] | @tsv' candidates.jsonl
```

Situation: preview only the selected track URIs.

```bash
spotuify playlist create "Exile and Return" --from candidates.jsonl --dry-run --format ids
```

## Agent Prompt

```text
Make a playlist from this brief. First generate or edit a plan JSON, resolve tracks with `spotuify resolve-tracks`, show me `playlist create --dry-run`, and wait for approval before running `--yes`.
```

## Guarantees

- Resolved candidates are JSONL, one query per line.
- Exact duplicate track URIs are not added twice.
- Unresolved items are explicit records, not silent drops.
- Dry-run and commit both read the same `candidates.jsonl` file.
- Commit returns a receipt with the created playlist URI and added item count.
