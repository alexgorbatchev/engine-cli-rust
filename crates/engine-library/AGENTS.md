---
created_on: 2026-10-14 16:34
last_modified: 2026-10-14 22:40
status: current
---

# Engine collection services

Read `../../AGENTS.md` and the applicable skills before changing this package. This internal workspace package belongs to `engine-cli`; its shared `audio-metadata` dependency resolves from a pinned GitHub revision of `fetch-track-cli-rust`. Run local recipes from this package directory.

## Commands

- `just check`: scoped formatting, strict Clippy, all-target tests with at least 90% line coverage, and documentation.
- `just test`: all-target behavioral tests.
- `just build`: all targets.
- `just doc`: local API documentation.
- `cargo fmt --package engine-library`: format only this package.

Tests require `ffmpeg` and `ffprobe` on PATH. Keep temporary test files and evidence under `.tmp`. Run image tests against real encoded images and audio tests against real media.

## Contracts

- Open native databases without creating, migrating, or inventing schema columns. Inspect optional folder and artwork-source columns.
- Keep explicit database creation separate from opening; publish new databases without replacing an existing destination. Preserve native schema spellings, explicit record identities, and nullable fields during interchange.
- Preserve native beat samples and unknown bytes. Bound Qt decompression by the caller's limit; reject truncated data. Follow sibling links without hanging on disconnected legacy chains, and maintain native sibling uniqueness constraints during mutations.
- Preserve nullable track fields, linked membership order, native SQLite errors, and ambiguous identifier errors. Maintain transactions across related database mutations.
- Explicit playlist insertion appends its ID to native sibling links, rejects missing parents and occupied IDs, and participates in an existing caller transaction. History append links an existing track and preserves unknown play times as NULL.
- Default new durations to documented milliseconds. Require explicit units for known seconds-based collections; never infer units by magnitude.
- Preserve original files until the database update succeeds. Never replace an occupied file-move destination. Retain committed and rollback cleanup errors.
- Generate 256-pixel database thumbnails and 1400-pixel cache images. Preserve exact case in base64url cache names. Keep audio bytes unchanged during artwork normalization.
- Use SQLite's online backup API for live databases. Preserve preflight checks and partial restore paths; separate databases are not one atomic snapshot or restore.
- Keep operations cancellable at their documented record/chunk/backup boundaries. Report per-record normalization failures rather than silently ignoring them.

The inspected source is `../../../.tmp/sources/engine-cli`; its upstream MIT license is retained. Hardware validation requires separate evidence and must not be inferred from local SQLite or audio tests.
