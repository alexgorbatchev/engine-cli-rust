---
created_on: 2026-10-14 16:50
last_modified: 2026-10-14 16:50
status: current
---

## `engine-cli playlist list`

Print the Collection hierarchy with `[id:<id> tracks:<count>]`. Agent mode uses indented bullets; human mode uses box branches. An empty collection prints `status: empty` or a human message.

## `engine-cli playlist inspect <playlist>`

Resolve required string `playlist` by ID or name and print tracks in stored linked order. Lines contain artist/title (filename fallback), track ID, and minutes:seconds using the selected duration unit. Empty playlists print `status: empty` or a human message.

## `engine-cli playlist create <name>`

Create the required string `name`. `--parent` / `-p` is a string, default unset (effective parent Collection). A parent accepts a name, ID, `0`, or `Collection` case-insensitively. `--folder` is bool, default false. Print `status: ok`, `id`, `title`, `parent_id`, `is_folder`.

## `engine-cli playlist rm <playlist>`

Delete a playlist by required string ID/name and its memberships. `--force` / `-f` is bool, default false; true also deletes descendants. Children without force cause an error. Print `status: ok`, `deleted_id`, `deleted_title`. Audio and collection tracks remain.

## `engine-cli playlist move <playlist>`

Move a playlist under required string `--to` (no default), accepting parent name, ID, `0`, or `Collection`. Reject cycles. Print `status: ok`, `id`, `title`, `parent_id`.

## `engine-cli playlist track add <playlist> <tracks>...`

Add one or more required string track identifiers to the required playlist ID/name, preserving linked ordering. Print one `status: ok`, `playlist_id`, `track_id` result per item. Duplicate membership is retained once.

## `engine-cli playlist track rm <playlist> <tracks>...`

Remove memberships for one or more required track identifiers from the required playlist ID/name. Print `status: ok`, `playlist_id`, `removed_track_id` per item. Collection tracks and audio remain.

## `engine-cli playlist track move <source> <destination> <tracks>...`

Atomically transfer each track's membership between required string source/destination playlist identifiers. Print `status: ok`, `src_playlist_id`, `dst_playlist_id`, `track_id` per item. Earlier completed transfers survive a later item failure.
