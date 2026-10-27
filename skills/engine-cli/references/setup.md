---
created_on: 2026-10-14 16:50
last_modified: 2026-10-14 16:50
status: current
---

## Runtime and global options

Use an existing Engine DJ collection containing `Database2/m.db`. FFprobe and FFmpeg on `PATH` provide audio inspection and native media operations; database-only commands operate without them. Close Engine DJ while modifying a collection to avoid competing application writes.

| Flag | Type | Default | Meaning |
| --- | --- | --- | --- |
| `--lib-dir`, `-l` | path | `~/Music/Engine Library` | Collection directory; a leading whole `~` segment expands through the home directory API. |
| `--no-backup` | bool | false | Skip the automatic pre-mutation snapshot. |
| `--duration-unit` | string | milliseconds | Accepted values: milliseconds, seconds. Select seconds only for a collection whose stored units are verified. |
| `--version` | bool | false | Print the raw version at the root. |
| `--help`, `-h` | bool | false | Print the selected command's help. |

Global collection flags are accepted before or after subcommands. `library` has visible aliases `lib` and `db`. Identifiers are exact numeric IDs or names; track lookup also accepts filenames and paths. Ambiguous matches return an error with candidate IDs.

## Output, cancellation, and safety

`AGENT=1`, `true`, or `yes` selects compact output. Unset `AGENT` or `0` selects human labels and box trees. Results go to stdout; warnings and one returned error go to stderr. Displayed paths fold the home prefix to `~`, including JSON report paths. Filesystem operations use the original paths.

Exit codes: 0 success; 1 argument, I/O, database, or media error; 6 partially completed restoration or committed database change with failed file cleanup; 130 cancellation. SIGINT and SIGTERM cancel active work. A failed batch can retain earlier completed items; inspect the collection before retrying.

Mutations first create a snapshot under the platform user data directory plus `engine-cli/backups` (`$XDG_DATA_HOME/engine-cli/backups` or `~/.local/share/engine-cli/backups` on Linux). They print `backup_snapshot`; restore prints `safety_backup`. `--no-backup` skips this step. Read-only verification, listing, inspection, and explicit backups do not trigger automatic snapshots. Backups contain databases and artwork, not audio files. They are consistent per SQLite database, not one transaction across separate database files. Restoring replaces database contents and matching artwork; it retains unrelated files. Hardware and Engine DJ device round trips remain unverified.

Durations default to documented milliseconds, with no magnitude heuristic or rewrite of existing stored values. `--duration-unit seconds` controls imported/synchronized duration values and displayed playlist durations for verified legacy collections.
