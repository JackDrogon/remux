# Decisions

- Renamed the Cargo package identity to `retmux` so `cargo build --bin retmux` works without adding
  extra binary wiring beyond the standard `src/main.rs` layout.
- Kept `src/main.rs` as a no-output baseline entrypoint for Task 1 so the starter greeting is gone,
  but no premature CLI parsing contract from Task 2 is introduced.
- Added fixture tests that validate committed legacy assets without depending on future
  backup/restore modules; this freezes the external on-disk contract early and reduces drift risk
  for later tasks.
- Centralized Task 2 CLI behavior in `src/cli.rs` and kept `main` as a single
  `std::process::exit(retmux::run(...))` bridge so later tasks can replace use-case stubs without
  reworking the external flag contract.
- Implemented the legacy snapshot domain in `src/model.rs` with constructor-carried defaults and
  helper methods mirroring the Python reference (`windows_in_reverse`, `min_pane_id`, and `idstr`)
  so later backup/restore tasks can reuse the compatibility behavior directly.
- Implemented `src/serde_legacy.rs` as an explicit legacy JSON compatibility layer that validates
  `__class__` / `__module__`, returns typed `LegacySnapshotError` variants on malformed input, and
  writes Python-compatible snapshot JSON for future backup/restore consumption.
- Implemented Task 3 config behavior in a dedicated `src/config.rs` module so `~/.retmux`,
  `retmux.conf`, `backup`, `backup-sockets/<sanitized>`, `content.with.escape`, and tmux
  command-prefix activation all come from one reusable source of truth.
- Kept the runtime shape as "loaded config + later socket activation" rather than parsing socket
  state inside the config file loader; this mirrors the Python flow (`load_config()` then
  `set_tmux_socket_name(...)`) and keeps backup-root derivation centralized for Tasks 4/6/7/8.
- Treated malformed config as a fatal startup error instead of silently falling back to defaults,
  because the Rust port's acceptance criteria require deterministic failure reporting for bad
  `retmux.conf` content.
- Implemented Task 6 as a dedicated `src/tmux.rs` adapter around a typed `TmuxCommand` surface so
  later backup/restore work can reuse one socket-aware, shell-free tmux seam instead of inlining
  `Command` invocations throughout the crate.
- Modeled subprocess failures in `src/error.rs` with explicit missing-binary, spawn, wait, timeout,
  and nonzero-exit variants that retain the rendered command plus status/stdout/stderr context,
  giving later tasks deterministic failure handling without string parsing.
- Implemented Task 4 in a dedicated `src/catalog.rs` module so backup enumeration, named lookup,
  latest selection, detail rendering, and deletion all reuse the same active-root query layer
  instead of scattering path logic across CLI actions.
- Kept Task 4 strictly non-interactive in `src/cli.rs`: `-l` without an argument now prints a
  deterministic summary list, while `-l <name>` and `-d <name>` execute the real named flows without
  prematurely pulling in Task 9's interactive prompts.
- Implemented Task 8 restore behavior in a dedicated `src/restore.rs` module so CLI dispatch stays
  thin while the restore engine can own backup-name resolution, snapshot loading, dummy-session
  bootstrap, reverse window replay, and fail-fast error handling.
- Chose explicit preflight validation for restore inputs: backup directories are resolved from the
  active socket root using directory mtimes for latest-backup fallback, and pane-content files are
  checked before `send-keys cat ...` is issued so malformed or incomplete backups fail
  deterministically instead of depending on tmux-shell side effects.
- Implemented Task 7 in `src/backup.rs` as a standalone capture use-case that resolves the backup id
  before probing tmux, rejects duplicate tree roots without overwrite, returns a clean success when
  no tmux server exists, and writes the legacy `<backup_id>/<backup_id>.json` plus
  `session:window.pane` content files under the active socket root.
- Kept CLI integration for Task 7 scoped to `-b` only by wiring `src/cli.rs` directly to
  `backup::capture_backup(...)`, so backup behavior is real without expanding restore or interactive
  flows beyond their task boundaries.
- Wrote pane content files from the raw `capture-pane` subprocess output instead of
  `TmuxAdapter::capture_pane()`'s normalized string path, because legacy compatibility requires
  preserving the trailing newline and any ANSI escape bytes exactly as tmux emitted them.
