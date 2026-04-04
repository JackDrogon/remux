# SOURCE LAYER KNOWLEDGE BASE

## OVERVIEW
The `src/` directory contains the core logic of `remux`. It is structured as a library (`lib.rs`) with a thin binary wrapper (`main.rs`). The design emphasizes synchronous I/O, type-safe tmux interaction, and legacy compatibility.

## MODULE MAP
- **Tmux Adapter** (`tmux.rs`): The exclusive gate for `tmux` subprocess calls. Uses `std::process::Command` with custom format strings (`:=:`) for deterministic output parsing.
- **Persistence** (`snapshot.rs`): Handles the dual-file JSON storage (`summary.json`, `manifest.json`) and pane content hashing (SHA256).
- **Core Logic**:
    - `backup.rs`: Orchestrates state capture and disk persistence.
    - `restore.rs`: Implements the `RestoreEngine` which handles renumbering, layout replay, and dummy-session probing.
    - `catalog.rs`: Manages the backup index, sorting, and lifecycle.
- **Configuration** (`config/`): Hierarchical config loading and runtime path derivation (supporting socket isolation).
- **Domain Model** (`model.rs`): Pure data structures representing the tmux hierarchy (Session -> Window -> Pane).

## CONVENTIONS
- **Path Isolation**: All backup paths MUST be derived through `AppState::active_backup_path()` to ensure `-L` socket isolation works correctly.
- **Error Propagation**: Use module-specific Error enums. Prefer `String` errors only at the CLI boundary (`cli.rs`).
- **Sync Only**: No `async/await`. Tmux interaction is naturally sequential and binary-driven.
- **Safe Capture**: Pane content is captured as raw bytes to preserve trailing newlines for legacy parity.

## ANTI-PATTERNS
- **Direct Command Call**: Never use `std::process::Command` directly for tmux; always use `TmuxAdapter`.
- **Path String Building**: Avoid manual path concatenation; use `PathBuf` and central config methods.
- **State Leakage**: Don't query host tmux options (like `base-index`) without considering if a server is actually running (see `RestoreEngine::ensure_base_index_ready`).

## NOTES
- **Critical Files**: `restore.rs` and `tmux.rs` are high-impact. Changes here require `test-live` verification.
- **Deterministic Ordering**: Catalog listings must sort by `mtime desc` then `id desc`.
- **Legacy Compatibility**: `snapshot.rs` must remain compatible with Python-style `__class__` markers.
