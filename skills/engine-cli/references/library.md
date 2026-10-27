---
created_on: 2026-10-14 16:50
last_modified: 2026-10-14 16:50
status: current
---

## `engine-cli library verify`

Audit audio existence and sizes, artwork references, orphaned playlist links, and empty playlists without writing. `--json` is a bool, default false; true prints the complete camelCase JSON report including `issues` and `isHealthy`. Agent summary fields are `total_tracks`, `missing_files`, `corrupt_sizes`, `broken_entities`, `missing_artwork`, `empty_playlists`, and `status` (`healthy` or `issues_detected`). Findings are a successful audit; runtime failures return an error.

## `engine-cli library sync`

Update existing tracks from current audio metadata, report missing files, and scan for unindexed audio without importing it. Preserve stored duration units using the global option. Print `status: complete`, `total_tracks`, `updated_records`, `missing_files`, `unindexed_files`, and repeated `unindexed` paths. Probe warnings go to stderr.

## `engine-cli library backup [path]`

Create a unique SQLite-aware snapshot. Optional `path` is a filesystem path; it overrides `--out-dir` / `-o` (path, default unset). With neither, use the platform data directory's `engine-cli/backups`. `--list` is bool, default false; true lists snapshots at the selected destination without creating one. Creation prints `status: success`, `path`, `size_bytes`, `db_files`, `art_files`. Listing prints `count` and repeated `backup: <path> | timestamp: <RFC3339> | size_bytes: <bytes>` lines.

## `engine-cli library restore <path>`

Restore a snapshot directory after source database validation. Create the default safety snapshot first unless `--no-backup` is set. Print `status: success`, `restored_from`, and `target_dir`. A failure after earlier files were restored returns a typed partial outcome with completed paths and exit 6.

## `engine-cli library prune`

Delete artwork rows not referenced by tracks and files not referenced by surviving artwork rows. Print `status: complete`, `deleted_db_rows`, and `deleted_disk_files`. Audio files are retained.
