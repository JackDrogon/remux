# PROJECT KNOWLEDGE BASE

Generated: 2026-04-04 / 8894101 / main

## OVERVIEW
`remux` is a Rust-based tmux session backup and restore tool. It is designed as a high-performance, 
type-safe port of the Python `retmux` utility, maintaining full structural compatibility with legacy 
backup formats while providing a robust CLI for Linux environments.

## STRUCTURE
- `src/`: Core logic and binary entry point.
- `tests/`: Extensive integration suite (CLI, Compatibility, Live Tmux).
- `assets/`: Bundled default configurations.
- `ref/retmux/`: Reference Python implementation for behavior parity.
- `.config/`: Tool-specific configurations (e.g., nextest).

## WHERE TO LOOK
- **Tmux Interaction**: `src/tmux.rs` (adapter layer) and `tests/live_tmux.rs` (real integration).
- **Configuration**: `src/config/` (schema, paths, loading).
- **Data Persistence**: `src/backup.rs` and `src/snapshot.rs`.
- **Catalog Management**: `src/catalog.rs` (listing, latest-resolution, deletion).
- **Restoration Logic**: `src/restore.rs`.

## CODE MAP
- `main.rs` -> `cli.rs`: Entry point and command routing.
- `lib.rs`: Library exports for modular testing.
- `backup_name.rs`: Centralized validation for backup identifiers.
- `model.rs`: Core domain entities (Session, Window, Pane).
- `interactive.rs`: Thin layer for the `-i` / interactive loop.

## CONVENTIONS
- **Backup Roots**: Default to `~/.remux/`. Use socket-based isolation under `backup-sockets/`.
- **Determinism**: Backups are sorted by `mtime desc` then `id desc`.
- **Error Handling**: Use `src/error.rs` types; avoid `panic!` in library code.
- **Compatibility**: Must decode legacy JSON markers (`__class__`) and handle optional `Window.name`.
- **Testing**: Use `nextest` for unit/integration; `live_tmux` tests must be ignored by default.

## ANTI-PATTERNS (THIS PROJECT)
- **Leaking Host State**: Never run live tests without a temporary `HOME`.
- **Direct I/O in Capture**: Keep the capture core in `backup.rs` decoupled from CLI printing.
- **Duplicate Path Logic**: Always derive paths through `RuntimeConfig` to ensure socket isolation.
- **Mutable Tmux during Validation**: Validate backup integrity before mutating the real tmux server.

## UNIQUE STYLES
- **Sync Over Async**: Uses `std::process::Command` for deterministic tmux interaction.
- **Telegraphic CLI**: Minimalist output; errors use stable messages for legacy parity.
- **Structural Decoding**: Rust structs overlay optional JSON fields to preserve legacy defaults.

## COMMANDS
- `just build`: Standard debug build.
- `just test`: Runs unit tests and doctests.
- `just test-live`: Runs integration tests against a real tmux server (requires `tmux`).
- `just check`: Full gate (fmt, dprint, typos, clippy, test, test-doc, test-live).
- `cargo run -- -h`: View all CLI actions (Backup, List, Restore, Interactive).

## NOTES
- **Runtime**: Linux + tmux on `PATH` + Rust stable (1.85+).
- **CI**: Enforces strict formatting (rustfmt + dprint) and full test coverage.
- **Legacy Path**: Ported from `ref/retmux`; behavior must match Python implementation exactly.
