---
name: engine-cli
description: Use when operating engine-cli to inspect, back up, restore, or modify Engine DJ collections, playlists, audio files, and artwork.
author: alexgorbatchev
metadata:
  created_on: 2026-10-14 16:50
  last_modified: 2026-10-14 17:01
  status: current
---

Use `AGENT=1 engine-cli` for compact key-value output and bullet trees. Resolve the collection directory and inspect it before selecting a mutation. Keep automatic safety snapshots enabled for writes.

Read [setup](references/setup.md) for runtime requirements, global flags, output, errors, duration units, and backup safety. Read [library](references/library.md) for verification, sync, backups, restoration, and pruning. Read [playlists](references/playlists.md) for hierarchy and memberships. Read [tracks](references/tracks.md) for audio import, removal, movement, relocation, and artwork. Read [completion](references/completion.md) for shell scripts. Read [help](references/help.md) for every generated help query path.

## Workflow

1. Run `AGENT=1 engine-cli --lib-dir "~/Music/Engine Library" library verify --json`.
2. Run `AGENT=1 engine-cli playlist list` and inspect the selected playlist with `AGENT=1 engine-cli playlist inspect "Warm Up"`.
3. Import with `AGENT=1 engine-cli track add ./song.flac --to-playlist "Warm Up"`; retain the printed `backup_snapshot` and `track_id`.
4. Verify the collection again. Use `engine-cli library restore <path>` with a confirmed snapshot when restoration is intended.

## Metadata and help

- `engine-cli` prints root help. `engine-cli library`, `engine-cli playlist`, `engine-cli playlist track`, `engine-cli track`, and `engine-cli track artwork` print the selected group's subtree without accessing the collection.
- `engine-cli skill` accepts no arguments or command-specific flags and prints the canonical Skills CLI install command and GitHub skill URL, identically in both modes. The local Rust artifacts and skill are unpublished; the printed canonical URL is a discovery pointer, not evidence of a published Rust release.
- `engine-cli --version` prints `0.1.0` followed by one newline.
- `engine-cli help [command]...` prints help for a command path. `--help` / `-h` is a bool help flag on every command. Agent help starts with the instruction to read `AGENT=1 engine-cli skill`.
- Use explicit `--help` for any command path. `engine-cli completion` prints the completion group's help without accessing the collection.
