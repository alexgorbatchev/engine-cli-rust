---
created_on: 2026-10-14 16:50
last_modified: 2026-10-14 16:50
status: current
---

## `engine-cli track add <paths>...`

Import one or more required audio filesystem paths and native tags. `--to-playlist` is a string, default unset; create the named playlist when absent and assign imported tracks. `--move` is bool, default false; true moves files into `Music` under the collection without replacing existing files. Without move, retain source files and store relative collection references. Probe warnings go to stderr while preserved available metadata can still be imported. Print per item `status: ok`, `track_id`, `title`, `path`, `moved_file`, and optional `playlist_id`, `playlist`.

## `engine-cli track rm <tracks>...`

Remove one or more required string track identifiers and their memberships. `--delete-file` is bool, default false; true removes the physical audio after the database change. Print `status: ok`, `deleted_track_id`, `title`, `deleted_file`. A committed database deletion with failed physical cleanup exits 6.

## `engine-cli track move <track> <destination>`

Move the audio for required string track ID/title/path to a required destination path (file or existing directory) and update the database. Existing destinations are preserved and cause an error. Print `status: ok`, `track_id`, `title`, `new_path`.

## `engine-cli track relocate`

Replace matching stored directory prefixes using required string `--from` and required string `--to` (no defaults). Match whole path components. Audio files remain in place. Print `status: ok`, `relocated_count`, `from_prefix`, `to_prefix`.

## `engine-cli track artwork normalize`

Crop embedded nonsquare artwork to square covers and normalize cached artwork with content-hash references updated. Keep audio data intact. Embedded covers use 1400×1400 and collection thumbnails use 256×256. Print `status: complete`, `total_tracks`, `normalized`, `already_square`, `no_art`, `errors`, `cache_normalized`. Individual file errors are warnings and increment `errors`; completed items remain.
