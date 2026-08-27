# SOURCE LAYER KNOWLEDGE BASE

## SOURCE STRUCTURE
- `main.rs`: Binary entrypoint.
- `cli/`: Argument parsing, dispatch, `AppError`, observability, and catalog presentation.
- `actions/`: CLI action implementations such as backup, restore, and interactive flows.
- `lib.rs`: Public API facade for integration testing.
- `model/`: Pure data structures (Session -> Window -> Pane).
- `storage/`: Catalog, snapshot persistence, backup-name rules, and compact fingerprints.
- `tmux_adapter/`: tmux subprocess adapter, including command verbosity.

## IMPLEMENTATION GUIDELINES
- **Tmux Interaction** (`tmux_adapter/`): Mandatory adapter layer. Uses custom format strings (`:=:`) for deterministic output. Handles raw process bytes to avoid encoding-related data loss.
- **Persistence** (`snapshot.rs`): Dual-file JSON storage (`summary.json`, `manifest.json`). Must maintain Python-style `__class__` markers for legacy compatibility and handle optional JSON fields like `Window.name`.
- **Business Logic**:
    - `actions/backup/`: State capture (raw bytes for pane content) and `/proc` command-tree inspection. Groups sessions and captures terminal dimensions.
    - `actions/restore.rs`: Sequential replay (renumbering, base-index probing). Rebuilds layouts using tmux-native strings.
    - `catalog.rs`: Catalog lifecycle and isolation. Sorting by `mtime desc` then `id desc`.
- **Config** (`config/`): Path derivation (`AppState::active_backup_path`). Handles socket-dir sanitization (`[^A-Za-z0-9_.-] -> _`).

## CORE DOMAIN
- `model/`:
    - **Session**: High-level grouping, tracks active window.
    - **Window**: Tracks order, layout, and child panes.
    - **Pane**: Tracks working directory, history content, and TTY state.
- `actions/interactive.rs`: Reloads catalog on every loop iteration to keep state fresh.
- `storage/backup_name.rs`: Centralized regex validation for custom backup IDs.

## CONVENTIONS
- **Path Isolation**: Derive all backup paths through `AppState` to honor `-L` socket isolation.
- **Safe Capture**: Preserve trailing newlines in pane content (legacy parity).
- **Error Handling**: Module-level Error enums; strings only at CLI boundary.
- **Sync Model**: Standard synchronous `std::process::Command` (no async).

## ANTI-PATTERNS
- **Direct Shell Calls**: Never skip `TmuxAdapter` to call `tmux` directly.
- **Manual Paths**: No manual `.join(".remux")` or similar path building; use `ConfigPaths`.
- **Server Assumptions**: Check for running server/active session before querying state.
