# Spotuify album-reactive TUI worker context

You are a Pi worker launched by PE Tasker. Implement only your assigned task.

## Product destination

Spotuify should keep playback reliable first, then make the terminal UI feel more alive by reacting to album art colors. Playlists remain the user's playlist library; recommendations must be probed separately before becoming product UI.

## Non-negotiables

- Stay within your task scope and allowed paths.
- Do not commit, tag, release, or merge from inside the worker.
- Do not mutate live Spotify state unless the task explicitly allows it.
- For task-001, live next/previous playback checks are allowed because the user reported that exact playback regression.
- Keep daemon-owned state as the source of truth. Do not make the TUI locally fake playback state as a shortcut.
- Use `spotuify` CLI/logs to verify runtime behavior where useful.
- Run the validation commands listed in the task spec before reporting completion when feasible.
