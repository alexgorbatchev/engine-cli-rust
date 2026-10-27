`engine-cli` helps DJs inspect, organize, repair, and back up Engine DJ collections from a terminal. It manages playlists and audio references with safety snapshots before mutations.

# What It Does

- **Audit collections:** Report missing audio, size mismatches, broken playlist links, missing artwork, and empty playlists.
- **Organize playlists:** Create folders, move playlists, and transfer track memberships in their stored order.
- **Manage audio:** Import tags, synchronize metadata, move files, and relocate stored directory prefixes.
- **Protect changes:** Create SQLite-aware database and artwork snapshots, restore them, and retain partial-outcome errors.
- **Normalize artwork:** Square embedded covers and cached thumbnails while preserving audio data.
- **Complete commands:** Generate Bash, Fish, PowerShell, and Zsh completion scripts.

# How It Works

- Select an existing collection with `--lib-dir`, or use `~/Music/Engine Library`.
- Inspect the collection with `library verify` and browse it with `playlist list`.
- Run the desired mutation; retain the printed safety-snapshot path.
- Verify the result or restore a confirmed snapshot with `library restore`.

# How it Really Works

- Results go to stdout. Warnings and one returned error go to stderr. `AGENT=1` selects compact key-value output and bullet trees; displayed home paths use `~`.
- Automatic snapshots live in the platform data directory under `engine-cli/backups`; on Linux this is `$XDG_DATA_HOME/engine-cli/backups` or `~/.local/share/engine-cli/backups`. Snapshots include databases and artwork, not audio files, and are consistent per database rather than across separate database files.
- Import retains source files unless `--move` is set. Removal retains audio unless `--delete-file` is set. Relocation changes database paths without moving audio.
- Duration values use documented milliseconds by default. Choose `--duration-unit seconds` only for a collection whose stored units are verified; existing raw values are retained without a magnitude heuristic.
- Embedded covers are 1400×1400; collection thumbnails are 256×256. Normalized cache files receive updated content-hash references.
- Exit 0 means command success, including an audit that finds issues. Exit 1 means an argument or runtime error, 6 means partial restoration or committed changes with failed file cleanup, and 130 means cancellation. Earlier items in a failed batch may remain completed.
- Hardware and Engine DJ device round trips remain unverified. Close Engine DJ while editing a collection.

# Prerequisites

An existing Engine DJ collection with `Database2/m.db` is required for collection operations. [FFmpeg and FFprobe](https://ffmpeg.org/) on `PATH` provide media inspection and audio operations; database-only commands operate without them.

# Installation

Rust binaries are unpublished. Existing Go release archives do not contain this Rust implementation. Local development builds are documented in [AGENTS.md](AGENTS.md).

# Quick Start

```sh
engine-cli --lib-dir "~/Music/Engine Library" library verify
AGENT=1 engine-cli playlist list
engine-cli track add ./song.flac --to-playlist "Warm Up"
engine-cli library backup ./snapshots
```

Sample Output from `AGENT=1 engine-cli playlist list` for an empty collection:

```text
status: empty
```

# Options & Flags

| Flag | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `--lib-dir <path>` | `-l` | `~/Music/Engine Library` | Existing collection directory |
| `--no-backup` | | `false` | Skip the pre-mutation snapshot |
| `--duration-unit <string>` | | `milliseconds` | Stored units: `milliseconds` or `seconds` |
| `--version` | | `false` | Print the raw version |
| `--help` | `-h` | `false` | Print help for any command |

`engine-cli library verify`

| Flag | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `--json` | | `false` | Print the complete report as JSON |

`engine-cli library backup [path]`

| Flag | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `--list` | | `false` | List snapshots at the chosen destination |
| `--out-dir <path>` | `-o` | User data directory plus `engine-cli/backups` | Snapshot destination; positional `path` takes precedence |

`engine-cli playlist create <name>`

| Flag | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `--parent <string>` | `-p` | `Collection` | Parent name or ID |
| `--folder` | | `false` | Create a folder |

`engine-cli playlist rm <playlist>`

| Flag | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `--force` | `-f` | `false` | Delete descendants recursively |

`engine-cli playlist move <playlist>`

| Flag | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `--to <string>` | | Required | Parent name, ID, or `Collection` |

`engine-cli track add <paths>...`

| Flag | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `--to-playlist <string>` | | Unset | Assign imported tracks, creating the playlist if absent |
| `--move` | | `false` | Move audio into the collection's `Music` directory |

`engine-cli track rm <tracks>...`

| Flag | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `--delete-file` | | `false` | Also remove physical audio |

`engine-cli track relocate`

| Flag | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `--from <string>` | | Required | Old stored directory prefix |
| `--to <string>` | | Required | New stored directory prefix |

`engine-cli completion <bash|fish|powershell|zsh>`

| Flag | Short | Default | Description |
| :--- | :--- | :--- | :--- |
| `--no-descriptions` | | `false` | Omit descriptions from the generated shell script |

Completion scripts go to stdout without accessing the collection. For example, save Zsh output with `engine-cli completion zsh > _engine-cli` and load it through Zsh's completion system.

# License

[MIT](LICENSE), copyright 2026 Alex Gorbatchev.
