# SOURCE LAYER KNOWLEDGE BASE

## SOURCE STRUCTURE
- `main.rs`: Binary entrypoint.
- `cli/`: Argument parsing, dispatch, observability, and catalog presentation.
- `actions/`: CLI action implementations such as backup, restore, and interactive flows.
- `lib.rs`: Public API facade for integration testing.
- `model/`: Pure data structures (Session -> Window -> Pane).
- `storage/`: Catalog, snapshot persistence, backup-name rules, and compact fingerprints.
- `tmux_adapter/`: tmux subprocess adapter, including command verbosity.

## IMPLEMENTATION GUIDELINES
- **Tmux Interaction** (`tmux_adapter/`): Mandatory adapter layer. Uses custom format strings (`:=:`) for deterministic output. Handles raw process bytes to avoid encoding-related data loss. Subprocesses are waited to completion; remux does not impose its own deadline.
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
- **Error Handling**: Keep compact catalogs and generated `Code`/`Category` together in `src/error.rs`; do not split them into per-catalog files. Callers construct inner catalog errors and convert them at `Result` boundaries. The shared public `Result<T>` is `Result<T, xerror::Error<Code>>`; use xerror `Context` on that exact `Result`, and have the CLI render the outer report once. clap is the argv edge only: `print()` plus clap's exit code; it never becomes `Error` and never goes through remux stdout helpers.
  Philosophy (whole program): remux is a small CLI, not an error-routing engine. Name the fault with a catalog variant at the site it happens; use context only so a human can see which step/path/socket failed; never branch on context, `ErrorKind`, or `source()` to convert one code into another. Fail visibly; do not publish a partial snapshot; best-effort-remove temp dirs on handled failures (crash or power loss may leave temps). After the report, a human handles it. CLI `emit_line` maps BrokenPipe to success; interactive stdin/stdout failures, including BrokenPipe, stay `Interactive::InteractiveIo`. Catalog root is observed as Missing / Directory / NotDirectory (follow symlink-to-dir; file, symlink-to-file, and dangling symlink are NotDirectory). A race after Directory stays `ReadCatalog`. Backup-id slot occupancy uses lstat.
  Backup-id occupancy is a catalog slot (lstat: any directory entry, including a dangling symlink). Occupied → `Backup::DuplicateBackupId` before capture (policy, not a lock). Inspect IO → `Catalog::ReadMetadata`. Persist/commit failure stays `Snapshot`.   Tmux catalog variants are `BinaryNotFound`, `SpawnFailed`, `WaitFailed`, `TmuxFailed` (no accessors on `Tmux`/`Code`; match fields on the variant). Presence probes classify raw completed output before constructing a code: status 0 is present; absence requires status 1 plus an allowlist on `stderr.trim()` compared with ASCII lowercase `contains`. Server (`list-sessions` as has_server): empty stderr is hard failure; absence only if stderr contains `no server running` or `no such file or directory`. Session (`has-session` / `kill-session`): empty stderr is absence; also those two server-down strings plus `can't find session`, `cannot find session`, `session not found`, `unknown session`, `no current`. Anything else unsuccessful is `TmuxFailed` and is never rewritten to `Ok(false)`.   Snapshot publish is exclusive among cooperating remux instances: unpublished staging is created with `mkdir` (retry on collision, never `rm` a path this call did not create) and landed with `renameat2(RENAME_NOREPLACE)`. Occupied dest fails as `SnapshotIo`, not Duplicate. Unpublished payload/sync/land failure discards that staging best-effort. After a successful land the dest is published: parent fsync failure stays `SnapshotIo` plus a human context entry that the snapshot was published; do not delete dest or staging. Staging ownership is not a dirfd defense against a hostile same-UID process. Cleanup failure is one extra context entry on the primary code. Use `Result.context` at the failing step; `attach_context` only when two `Error`s already exist (restore/unpublished-snapshot cleanup).
- **Sync Model**: Standard synchronous `std::process::Command` (no async).

## ANTI-PATTERNS
- **Direct Shell Calls**: Never skip `TmuxAdapter` to call `tmux` directly.
- **Manual Paths**: No manual `.join(".remux")` or similar path building; use `ConfigPaths`.
- **Server Assumptions**: Check for running server/active session before querying state.
