# Learnings

- Task 1 freezes compatibility with committed fixture trees under
  `tests/fixtures/legacy/default_socket` and `tests/fixtures/legacy/named_socket_sockA` instead of
  relying on generated data.
- Legacy fixture validation is intentionally structural only in this task: backup directory name,
  `<backup_id>.json` naming, pane content files, and Python serializer markers `__class__` /
  `__module__`.
- The legacy pane content filenames follow `session:window.pane` naming, matching
  `tmuxbk.backup.backup_tmux()` and `Pane.idstr()` in the Python reference.
- Legacy CLI parsing is position-flexible for `-L <socket>`: the socket option can appear before or
  after the action payload, as long as the overall shape still collapses to exactly one action and
  at most one action argument.
- Task 5 showed that the frozen Python-style fixture can omit `Window.name`; matching
  `tmuxbk.util.dict2object()`, the Rust decoder needs to construct `Window` first and then overlay
  optional JSON fields so the default `win{win_id}` name survives.
- Legacy size fields are best modeled as either an empty tuple or a `(width, height)` pair; in JSON
  this means accepting `[]` for defaults and `[w, h]` for populated terminal sizes.
- Task 3 confirms the legacy startup order is "bootstrap/load config first, then parse CLI socket,
  then derive the active backup root"; keeping those steps separate lets `retmux -v` create
  `~/.retmux/retmux.conf` on a fresh HOME while still switching `backup` vs
  `backup-sockets/<sanitized>` after `-L` is parsed.
- The legacy socket directory sanitization really is a simple character replacement
  (`[^A-Za-z0-9_.-] -> _`), so names like `custom/socket name` must resolve to `custom_socket_name`
  instead of using URL encoding, hashing, or path nesting.
- Task 6 confirmed the legacy tmux command flow is "render the command arguments first, then replace
  the leading `tmux` token with the active prefix"; mirroring that order in Rust reproduces
  `tmux -L <socket> ...` exactly without duplicating socket logic in call sites.
- Python `exec_cmd()` removes only one trailing newline before downstream `split("\n")` calls, so
  the Rust adapter normalizes process output the same way and preserves legacy line-splitting
  semantics for list-style tmux responses.
- Task 7 confirmed that the legacy `-b` fallback name from `controller.tmux_id_4_backup()` is the
  bare timestamp shape `YYYYMMDD_HHMMSS`, not the fixture-style `backup_YYYYMMDD_HHMMSS` prefix.
- Legacy pane-content files should preserve the raw `capture-pane` stdout bytes on disk; if the Rust
  path reuses normalized command text instead of raw output, it drops the final newline that Python
  keeps when redirecting stdout directly to the pane file.
- Task 4 confirmed the catalog boundary should stay at `RuntimeConfig::active_backup_path()`:
  summary listing, named lookup, latest-backup resolution, and deletion all become socket-isolated
  automatically when they share that one root.
- The latest-backup rule needs one stable ordering source for both UI listing and restore fallback;
  sorting by `mtime desc` and then `backup id desc` makes the Rust path deterministic even when
  filesystem timestamps are close together.
- Task 8 confirmed that latest-backup fallback should follow the Python helper semantics and pick
  the active backup directory with the newest modification time, not the lexicographically greatest
  backup id.
- Restore compatibility depends on reversing window replay before recreating the base-index
  placeholder window; this allows a session restored from an initial tmux window at `base-index` to
  be renumbered upward first and then backfill lower-numbered windows without overwriting earlier
  work.
- Deterministic malformed-backup handling is easier if the Rust restore path validates legacy JSON
  markers and pane-content file existence before mutating tmux, while session-name conflicts remain
  the one intentional non-fatal branch that should be skipped and left untouched.
- Task 4 verification is easiest to preserve as plain command transcripts under
  `.sisyphus/evidence/`; capturing the exact `cargo test --test catalog_ops` output proves both the
  socket-isolation happy path and the missing-name failure path without needing extra harness code.
- Task 7 showed the backup capture core can stay in `src/backup.rs` while CLI-facing success
  behavior lives in `src/cli.rs`; mapping `BackupOutcome::Created` and `BackupOutcome::NoServer` to
  stable messages keeps the legacy no-server path observable without coupling I/O to the capture
  engine.
- The Task 7 evidence split is useful in practice: keep `creates_legacy_backup_tree` in
  `.sisyphus/evidence/task-7-backup-capture.txt`, and group `duplicate_backup_id_fails` plus
  `no_server_exits_cleanly` in `.sisyphus/evidence/task-7-backup-capture-error.txt` so reviewers can
  verify success-path and safety-path behavior independently.
- Task 8 showed that restore must use the caller-selected backup directory name for pane-content
  lookup, not the embedded snapshot `tid`; Python passes the external `tmux_id` through the whole
  restore path, so Rust should do the same to keep legacy directory semantics stable.
- The `src/` hierarchy enforces a strict separation between the subprocess adapter (`tmux.rs`), the serialization format (`snapshot.rs`), and the business engines (`backup.rs`, `restore.rs`), ensuring that platform-specific tmux quirks don't leak into the persistence logic.
- For fail-fast compatibility, pane-content existence checks should run after filtering out
  conflicting session names but before reading `base-index` or creating a dummy tmux session; this
  preserves the non-fatal collision branch while still avoiding tmux mutations for malformed/missing
  backup assets whenever possible.
- Task 9 confirmed the interactive layer can stay thin by reloading the active backup catalog on
  each loop iteration, rendering summary/detail through `catalog`, and delegating destructive work
  to `catalog::delete_backup` / `restore::restore_from_config` instead of inventing a second
  interactive-only code path.
- Deterministic interactive safety is easiest to preserve when empty/non-numeric/out-of-range
  selections are reported inline on stdout, invalid confirmations stay inside the yes/no prompt
  loop, and EOF aborts with a stable error before any delete or restore action is executed.
- When a CLI entrypoint intentionally changes from non-interactive to interactive, older catalog
  tests should assert socket-isolated listing via `catalog::list_backups` and
  `catalog::render_summary` instead of depending on EOF-sensitive binary behavior.
- Task 10 keeps the real tmux coverage in `tests/live_tmux.rs` as ignored-by-default tests and wires
  them into `just check` / CI with `cargo test --test live_tmux -- --ignored --nocapture`; this
  preserves a portable `cargo test --all-features` path while still making the supported Linux+tmux
  baseline explicit.
- Live tmux integration tests must launch direct `tmux` commands with the same temporary `HOME` used
  by the `retmux` binary, otherwise host-level `.tmux.conf` settings like pane/base index values can
  leak into the server state and create false restore failures.
- Task 10 scope-fix is simplest when incidental repo-formatting changes are reverted and only CI/just/README/tests wiring is kept.
- F4 scope review approved the current change set because it stays inside the plan's declared Rust-port surface: single-crate Cargo package, synchronous `std::process::Command` tmux adapter, legacy `~/.retmux/{backup,backup-sockets/<sanitized>}` roots, Python-compatible JSON markers, and task-10-limited CI/docs automation updates.
- For scope fidelity, untracked `.sisyphus/evidence/**` and notepad files are execution artifacts rather than release-surface expansion, while `src/config.rs`, `src/catalog.rs`, `src/backup.rs`, `src/restore.rs`, and their tests reinforce socket isolation by routing all backup access through `RuntimeConfig::active_backup_path()`.
- F2 code-quality review approved the delivered Rust port as release-ready for the planned scope: `lsp_diagnostics` reported zero Rust diagnostics across `src/` and `tests/`, `cargo test --all-features` passed, and `just check` also passed including the ignored live tmux suite.
- For maintainability, the current code keeps CLI and interactive layers thin by delegating backup/catalog/restore work into dedicated modules; the only notable debt seen during F2 is duplicated latest-backup resolution logic across `src/catalog.rs` and `src/restore.rs`, which is non-blocking today because both quality gates and restore coverage pass.
- F2 blocker follow-up is simplest when backup-name handling becomes a single shared normalization step before any filesystem lookup: creating with `-b "  name  "` now persists `name`, while catalog lookup/delete and named restore all reuse the same trimmed validation rules without touching `RuntimeConfig::active_backup_path()` semantics.
- CI dprint reliability is easiest to preserve by installing a pinned binary explicitly in the workflow before `dprint check`; pinning `taiki-e/install-action` plus `dprint@0.53.2` removes dependence on whatever tooling happens to exist on the runner image.
- F2 re-review approved the current workspace for the planned release scope: `src/backup_name.rs` now centralizes explicit-name validation, `backup` / `catalog` / `restore_from_config` all reuse it, `.github/workflows/ci.yml` installs pinned `dprint`, and both `cargo test --all-features` plus `just check` pass after the fix.
- The remaining quality debt is non-blocking for the documented binary release surface: latest-backup selection is still duplicated across `src/catalog.rs` and `src/restore.rs`, and `restore::restore_from_path_with_adapter` still trusts its already-resolved `backup_name` argument, but current CLI and CI-covered flows route through validated names.
- Created root AGENTS.md for remux project knowledge base.
- Created tests/AGENTS.md to document test isolation patterns, fixture usage, and live-tmux verification rules.
