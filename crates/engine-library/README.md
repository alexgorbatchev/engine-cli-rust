`engine-library` lets Rust applications inspect and maintain an Engine DJ music collection, including playlists, audio metadata, artwork, and recoverable database snapshots.

# What It Does

- **Collection access:** Reads nullable track metadata, playlist trees, native membership order, and artwork without migrating the database.
- **Music management:** Imports, moves, removes, relocates, and synchronizes tracks while preserving unrelated metadata.
- **Artwork maintenance:** Generates square covers and thumbnails, updates collection references, and prunes unreferenced images.
- **Snapshots:** Backs up committed SQLite data, including live WAL contents, and restores checked databases.
- **Verification:** Reports missing audio, byte-count differences, broken references, and empty playlists.
- **Collection interchange:** Creates explicitly requested databases and preserves complete track records, database identity, smartlists, history, and native performance data.

# How It Works

1. Open an existing `Database2/m.db` collection with `Database::open`.
2. Read tracks and playlists, or pass the collection directory to the music, verification, artwork, or backup operations.
3. Inspect the returned data and typed errors before performing another operation.

# How it Really Works

1. Opening a collection checks its required columns and leaves its schema intact. Missing optional track fields read as null and remain absent during writes. Collections without `Playlist.isFolder` support playlist containers; creating a separately marked folder returns a schema error. Explicit creation publishes a new database without replacing an occupied destination and generates a unique database UUID.
2. Playlist mutations use transactions and maintain membership and sibling ordering. Name resolution reports ambiguous matches. Directory relocation matches complete path components. Imports preserve existing ratings, artwork references, and tags that the audio file does not supply.
3. New durations use documented milliseconds by default. `DurationUnit::Seconds` is an explicit choice for collections with verified seconds-based values. Stored values remain unchanged during ordinary reads; the library never guesses units from their magnitude.
4. Audio inspection launches `ffprobe` through the supplied `Tools` configuration. A failed probe produces a warning and preserves existing metadata. Audio filenames must be UTF-8 because collection paths are SQLite text; an invalid path returns an error without replacement characters.
5. File moves publish a copy without overwriting an existing destination, commit metadata, then remove the source. A failed database update removes the staged destination and retains the source. `CommittedCleanup` and `Cleanup` retain native filesystem errors when cleanup fails.
6. Artwork normalization reads embedded covers without rewriting audio. It publishes 1400×1400 JPEG cache images under SHA-1 hexadecimal and case-sensitive base64url names, and stores 256×256 database thumbnails. Database and track-reference updates are transactional. A failed update can leave orphan cache files for pruning; warnings identify individual failures. Normalizing a hash-named cache image publishes its new content hash and updates collection references before removing the old cache entry. Non-square cache images with ordinary filenames retain their names.
7. Snapshots contain `Database2`, `Root`, and optional `Artwork` sections. Each database contains a consistent committed snapshot; separate databases and artwork files do not share a transaction. Restore preflights every source database before writing and returns `PartialRestore` with completed paths after a later failure. Unrelated destination files remain intact.
8. Database, image, filesystem, and backup calls block their calling thread. Music inspection is asynchronous. Use one database connection per execution context. Cancellation is checked between records, copy chunks, and SQLite backup steps; image decoding and encoding finish before the next check. No work runs at import time, and these services do not contact the network.
9. Performance decoding retains sample positions, floating-point precision, beat-marker fields, and trailing bytes. Empty slots remain distinct from unnamed cues at sample zero. Qt decompression requires an explicit allocation bound and rejects truncated or inconsistent streams. Raw performance records preserve opaque caches independently of decoded edits. History groups origin identity by database UUID and track ID, and compares parsed dates across native numeric and text representations.

# Prerequisites

- An existing Engine DJ collection with the required native tables and columns.
- Rust 1.89 or newer.
- [FFmpeg](https://ffmpeg.org/) with `ffprobe` for music inspection; native audio tests also use `ffmpeg`.

# Installation

This unpublished implementation crate belongs to the `engine-cli` repository. From a consumer beside that repository:

```sh
cargo add engine-library --path ../engine-cli/crates/engine-library
```

Keep the adjacent `fetch-track-cli` repository available: its internal `crates/audio-metadata` package supplies the local audio dependency.

# Quick Start

This reads the collection relative to the current directory:

```rust
use engine_library::{Database, Result};

fn main() -> Result<()> {
    let database = Database::open("EngineLibrary/Database2/m.db", true)?;
    for track in database.tracks()? {
        println!("{}\t{}", track.id, track.title.as_deref().unwrap_or(""));
    }
    Ok(())
}
```

# API

| Entry point | Behavior |
| :--- | :--- |
| `Database::open`, `from_connection`, `connection` | Open or supply a native SQLite connection; inspect the connection directly when needed. |
| `Database::create`, `create_history` | Create a new collection or history database without overwriting a destination. |
| `tracks`, `track`, `find_track`, `insert_track`, `update_track`, `delete_track` | Access nullable metadata and maintain memberships when deleting tracks. |
| `track_record`, `track_records`, `put_track_record` | Preserve complete native metadata and explicit identities during interchange. |
| `playlists`, `find_playlist`, `require_playlist`, `create_playlist`, `insert_playlist_record`, `move_playlist`, `delete_playlist` | Inspect and mutate the hierarchy with cycle checks and explicit recursive deletion; append explicit interchange IDs under the same native ordering constraints. |
| `playlist_siblings`, `reorder_playlists`, `playlist_metadata`, `put_playlist_metadata` | Follow native sibling links and replace a complete order under native uniqueness constraints; inspect and write optional metadata. |
| `entities`, `playlist_tracks`, `add_membership`, `remove_membership`, `move_membership` | Read linked ordering and transactionally maintain playlist membership. |
| `relocate_tracks` | Replace a complete directory prefix in stored track paths. |
| `artwork`, `upsert_art`, `put_artwork`, `update_art`, `delete_art` | Access database artwork records; retain explicit artwork identities on import. |
| `information`, `put_information`, `smartlists`, `put_smartlist` | Preserve database identity, schema version, and smartlist rules. |
| `performance`, `put_performance`, `performance::PerformanceData::decode` | Read, write, and decode native beat, cue, and loop data with an explicit decompression limit. |
| `history::read`, `history::append`, `records::parse_timestamp` | Read namespaced play history, append an event for an existing track with an optional timestamp, and parse native dates. |
| `transaction` | Run multiple database operations in one immediate transaction; an error rolls it back. |
| `collection::hierarchy`, `inspect_playlist`, `verify` | Read nested playlists, explicit-unit duration totals, and collection health reports. |
| `music::import`, `move_file`, `remove`, `synchronize` | Manage files and metadata with a caller-supplied cancellation token. |
| `artwork::normalize`, `prune` | Normalize embedded and existing images, or delete unreferenced records and regular cache files. |
| `backup::create`, `automatic`, `list`, `restore` | Create, enumerate, and restore snapshots. `automatic` supports a caller-controlled disable option. |

All fallible entry points return `engine_library::Result`; report types serialize with camelCase fields. Generate local API documentation with the contributor commands in [AGENTS.md](AGENTS.md).

# Configuration

| Option | Default | Meaning |
| :--- | :--- | :--- |
| `Database::open` read-only argument | Required | Selects read-only or read-write access; neither creates a database. |
| `ImportOptions::duration_unit` | `Milliseconds` | Units for imported durations; synchronization also requires an explicit unit. |
| `ImportOptions::playlist` | `None` | Existing playlist identifier or a name to create. |
| `ImportOptions::move_file` | `false` | Move the imported audio into the collection's `Music` directory. |
| Backup output | User data directory + `engine-cli/backups` | XDG data location on supported Unix systems; pass an explicit directory to override it. |
| Artwork cache path | Required | An empty path disables cache writes; database artwork is still updated. |
| `Tools` executable path and working directory | Required | Control native `ffprobe` lookup and execution. |

# Collection Compatibility

The database contract follows the documented Engine Library schema and inspected Go services. Local tests verify actual SQLite transactions, WAL snapshots, audio files, image bytes, cancellation, and filesystem failures. Engine DJ hardware behavior has not been verified. In particular, millisecond imports and separate thumbnail/cache sizes follow the documented format rather than the Go implementation's conflicting duration and image handling.

# License

[MIT](LICENSE), copyright 2026 Alex Gorbatchev; collection schema and performance fixtures retain Nate Randa's 2025 notice.
