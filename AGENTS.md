---
created_on: 2026-10-14 16:53
last_modified: 2026-10-14 22:40
status: current
---

# engine-cli

Native Rust command-line consumer of the internal Engine DJ collection package and shared audio metadata services. The CLI and `crates/engine-library` belong to one Cargo workspace.

## Commands

- Development build: `just build` writes `bin/engine-cli`.
- Run: `just run <args>`; compact mode: `just run-ai <args>`.
- Tests: `just test`.
- Full gate: `just check` includes scoped formatting, strict Clippy, tests, and 90% line coverage.
- Format: `just fmt`; scope formatters to the affected package because dependencies also include other repositories.

## Runtime and tests

- Use Rust 1.89+ and installed cargo-llvm-cov/llvm-tools for the coverage gate. Native media fixtures require FFmpeg and FFprobe on PATH.
- Native dependencies are internal `crates/engine-library`, `../fetch-track-cli/crates/audio-metadata`, and sibling `cli-command-kit`; integrate their actual APIs directly and keep domain implementations in the libraries.
- Tests create isolated SQLite collections and real audio inside `.tmp`; subprocess tests set their own HOME and XDG_DATA_HOME rather than modifying process-global environment.
- The binary version is 0.1.0. Rust artifacts are unpublished; existing Go release archives are not Rust binaries. The compiled canonical skill URL is a discovery pointer and does not establish a published Rust skill or release.

## Contracts

- Default stored duration units are documented milliseconds. Explicit seconds mode is for verified legacy collections. Never infer units by magnitude or rewrite stored raw values to guess their interpretation.
- Keep automatic snapshots before mutations and typed partial/committed-cleanup outcomes. Snapshots exclude audio and are consistent per database, not across separate databases.
- Route displayed paths and diagnostics through cli-command-kit; retain native paths for filesystem and subprocess calls.
- Preserve subject-first commands, compact AGENT output, raw version bytes, and once-only runtime error reporting. Argument-only validation occurs in Clap before collection access.
- Generate shell completions through clap_complete into memory, then write the resulting script fallibly. Fish's generator in 4.6.11 internally expects successful writes despite its try_generate API; caller output failures must remain returned errors.
- Maintain `skills/engine-cli/SKILL.md` and its references in the same change as command, argument, flag, default, environment, output, or side-effect changes; update last_modified and test against the live command tree.
- Always change a corresponding test file when runtime results change and retain at least 90% line coverage (scripts excluded). Prove behavioral tests fail with the implementation temporarily disabled, restore it, and rerun them.
- Keep task history and progress outside agent instructions; update standing rules in place. Never publish releases, tags, packages, or production deployments without explicit authorization.

## Technical provenance

The source CLI is `alexgorbatchev/engine-cli` (MIT). Reusable schema, transaction, backup, file, and artwork corrections are documented in `crates/engine-library`. Hardware and Engine DJ application/device validation are outside local test evidence.
