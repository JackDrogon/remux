# retmux Rust Port Plan

## TL;DR

> **Summary**: Port `ref/retmux` into this repository as a single-crate Rust implementation that
> preserves the legacy CLI contract, backup layout, socket semantics, and restore behavior while
> restructuring internals into testable Rust modules. **Deliverables**:
>
> - A Rust binary exposed as `retmux`
> - Legacy-compatible config, backup-path, JSON, and pane-file handling
> - Non-interactive and interactive backup/list/delete/restore flows
> - Fixture-based compatibility tests plus live tmux integration coverage in CI **Effort**: Large
>   **Parallel**: YES - 2 waves **Critical Path**: 1 → 2 → (3, 5, 6) → (4, 7, 8) → 9 → 10

## Context

### Original Request

`@ref/retmux 将retmux实现转换成rust实现到当前的repo`

### Interview Summary

- Compatibility target: preserve external behavior, allow internal Rust-idiomatic restructuring.
- Delivery shape: land the Rust port in parallel inside the current repository first; do not do a
  risky big-bang replacement.
- First-release scope: core paths first, but interactive flows must still be completed before the
  port is considered done.
- Test strategy: tests-after implementation, with agent-executed QA scenarios required for every
  task.

### Metis Review (gaps addressed)

- Freeze the compatibility boundary before broad implementation: CLI semantics, backup layout, JSON
  shape expectations, socket handling, restore sequencing, and config loading.
- Treat legacy fixture compatibility and live tmux orchestration as separate verification layers;
  both are mandatory.
- Keep v1 as one Rust crate with internal modules; do not split into workspace/multi-crate
  structure.
- Do not redefine the on-disk format, backup roots, or tmux process model during the port.

## Work Objectives

### Core Objective

Recreate the reference `retmux` behavior in Rust inside this repo, using a synchronous tmux
subprocess model and legacy-compatible backup storage so that existing retmux users can move to the
Rust binary without migrating their data or changing their operational workflow.

### Deliverables

- Replace the starter crate behavior in `src/main.rs` / `src/lib.rs` with Rust modules for CLI,
  config, domain model, tmux adapter, backup, restore, and interactive flows.
- Ship the executable name as `retmux`; repository directory naming remains unchanged and out of
  scope.
- Support reading Python-generated backup trees under `~/.retmux/backup` and
  `~/.retmux/backup-sockets/<sanitized-socket>`.
- Write Rust-generated backups using the same directory and file naming scheme, including
  `<backup_id>.json` plus pane-content files.
- Add automated tests for CLI/socket semantics, config/path behavior, fixture decoding/encoding,
  backup generation, restore sequencing, interactive flows, and corruption/failure handling.
- Extend CI/automation so tmux-backed integration tests run non-interactively in the repo’s
  verification pipeline.

### Definition of Done (verifiable conditions with commands)

- `cargo fmt --all --check`
- `dprint check`
- `typos .`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cargo test --test compat_fixtures`
- `cargo test --test restore_integration -- --nocapture`
- `cargo run --quiet --bin retmux -- -h`
- `HOME="$TMPDIR/retmux-home" cargo run --quiet --bin retmux -- -L sockA -b backup_20240101_120000`

### Must Have

- External contract compatibility for `-h`, `-v`, `-l`, `-d`, `-b`, `-r`, `-ri`, and `-L`, including
  `-L` appearing before or after the action.
- Legacy backup root behavior from `~/.retmux/backup` and isolated named-socket roots under
  `~/.retmux/backup-sockets/<sanitized-socket>`.
- Legacy config bootstrap and loading from `~/.retmux/retmux.conf`, including `content.with.escape`.
- Rust restore must consume Python-generated fixture backups without manual conversion.
- Restore must preserve key legacy semantics: latest-backup fallback, dummy-session bootstrap when
  no tmux server exists, `base-index` handling, reverse window restore order, and session-collision
  skip behavior.
- Supported baseline: Linux on `ubuntu-latest` CI with the tmux version installed in CI; broader
  OS/version support is explicitly out of scope for this first port.
- Failure policy: fail fast with deterministic error reporting, leave already-created tmux state in
  place, and do **not** attempt best-effort rollback of partially restored sessions.

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)

- No new backup format, alternate backup root, or incompatible JSON schema.
- No async runtime, daemon/service mode, or background job orchestration.
- No workspace split or multi-crate refactor in v1.
- No human-only verification steps; every acceptance criterion must be executable by agents.
- No exact-text parity requirement for ANSI art, spacing, or prompt copy beyond preserving the same
  action semantics and success/failure meaning.
- No repo-folder rename; only the shipped binary/interface is renamed to `retmux`.

## Verification Strategy

> ZERO HUMAN INTERVENTION — all verification is agent-executed.

- Test decision: tests-after + Rust unit/integration tests (`cargo test`) with tmux-backed
  integration coverage.
- QA policy: Every task includes a happy path and a failure/edge scenario with concrete commands,
  paths, session names, and evidence files.
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`
- Verification layers:
  1. Fixture compatibility against Python-style backup artifacts
  2. Live tmux orchestration tests against isolated sockets
  3. Full repo quality gates (`fmt`, `dprint`, `typos`, `clippy`, `test`)

## Execution Strategy

### Parallel Execution Waves

> Target: 5-8 tasks per wave. <3 per wave (except final) = under-splitting. Extract shared
> dependencies as Wave-1 tasks for max parallelism.

Wave 1: compatibility freeze and core plumbing

- Task 1: Freeze compatibility fixtures and binary identity defaults
- Task 2: Implement CLI parser/dispatch contract
- Task 3: Implement config bootstrap and active backup path resolution
- Task 5: Implement legacy data model and JSON compatibility layer
- Task 6: Implement tmux adapter and subprocess error model

Wave 2: behavior completion and pipeline hardening

- Task 4: Implement backup catalog/list/delete behavior on top of legacy data model
- Task 7: Implement backup capture and legacy on-disk output
- Task 8: Implement non-interactive restore sequencing
- Task 9: Implement interactive flows and user-facing logging/help output
- Task 10: Wire tmux integration tests, CI, and release-facing polish

### Dependency Matrix (full, all tasks)

| Task | Depends On                | Blocks            |
| ---- | ------------------------- | ----------------- |
| 1    | —                         | 2, 3, 5, 6, 10    |
| 2    | 1                         | 4, 7, 8, 9, 10    |
| 3    | 1, 2                      | 4, 6, 7, 8, 9, 10 |
| 4    | 2, 3, 5                   | 9, 10             |
| 5    | 1, 2                      | 4, 7, 8, 10       |
| 6    | 2, 3                      | 7, 8, 10          |
| 7    | 2, 3, 5, 6                | 10                |
| 8    | 2, 3, 5, 6                | 9, 10             |
| 9    | 2, 3, 4, 8                | 10                |
| 10   | 1, 2, 3, 4, 5, 6, 7, 8, 9 | F1-F4             |

### Agent Dispatch Summary (wave → task count → categories)

| Wave               | Task Count | Recommended Categories                  |
| ------------------ | ---------: | --------------------------------------- |
| Wave 1             |          5 | unspecified-high ×4, deep ×1            |
| Wave 2             |          5 | unspecified-high ×4, deep ×1            |
| Final Verification |          4 | oracle ×1, unspecified-high ×2, deep ×1 |

## TODOs

> Implementation + Test = ONE task. Never separate. EVERY task MUST have: Agent Profile +
> Parallelization + QA Scenarios.

- 1. [x] Freeze legacy compatibility fixtures and ship binary identity as `retmux`

  **What to do**: Replace the starter crate identity with the port baseline: expose the executable
  as `retmux`, remove starter-only assumptions from `Cargo.toml` / `src/main.rs` / `src/lib.rs`, and
  add committed fixture assets under `tests/fixtures/legacy/` for both default-socket and
  named-socket backup trees. Add a fixture validation test that proves the committed assets match
  the chosen legacy contract: same directory structure, same `<backup_id>.json` naming, pane-content
  files present, and Python-style `__class__` / `__module__` markers available where required.
  **Must NOT do**: Do not implement backup/restore logic yet; do not invent a Rust-native fixture
  format; do not leave the binary name as `remux`.

  **Recommended Agent Profile**:
  - Category: `deep` — Reason: this task freezes the compatibility baseline every later task depends
    on.
  - Skills: `[]` — No extra skill is needed; the work is repo-local and fixture-driven.
  - Omitted: [`playwright`, `git-master`] — No browser work and no git-history analysis are needed.

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: 2, 3, 5, 6, 10 | Blocked By: —

  **References** (executor has NO interview context — be exhaustive):
  - Pattern: `Cargo.toml:1-18` — Current crate is still a starter package named `remux`; this is the
    identity to replace.
  - Pattern: `src/main.rs:1-7` — Starter binary currently prints `Hello, remux!`; must be removed as
    part of the port baseline.
  - Pattern: `src/lib.rs:1-13` — Starter library currently contains only demo code; use it as the
    reset point.
  - Pattern: `ref/retmux/retmux:14-54` — Legacy CLI branding, option inventory, and config-file path
    shown to users.
  - Test: `ref/retmux/tests/test_cli_socket.py:40-98` — Existing legacy compatibility assertions for
    `-L` parsing and socket-specific backup directories.
  - Pattern: `ref/retmux/tmuxbk/config.py:17-23` — Canonical `.retmux` root, backup root,
    named-socket root, and config-file location.
  - Pattern: `ref/retmux/tmuxbk/util.py:12-18` — Python serializer markers that appear in legacy
    JSON fixtures.

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo build --bin retmux`
  - [ ] `cargo test --test compat_fixtures`
  - [ ] `test -d tests/fixtures/legacy/default_socket && test -d tests/fixtures/legacy/named_socket_sockA`

  **QA Scenarios** (MANDATORY — task incomplete without these):
  ```
  Scenario: Legacy fixture assets are complete and binary name is correct
    Tool: Bash
    Steps: cargo build --bin retmux && cargo test --test compat_fixtures
    Expected: build succeeds; fixture test confirms default-socket and named-socket fixture trees exist and match expected legacy file names.
    Evidence: .sisyphus/evidence/task-1-compat-fixtures.txt

  Scenario: Corrupt fixture shape is rejected
    Tool: Bash
    Steps: cargo test --test compat_fixtures corrupt_fixture_shape_is_rejected -- --exact --nocapture
    Expected: the dedicated test passes by proving malformed fixture content is detected and reported as invalid rather than silently accepted.
    Evidence: .sisyphus/evidence/task-1-compat-fixtures-error.txt
  ```

  **Commit**: YES | Message: `feat(retmux): freeze legacy fixtures and binary identity` | Files:
  `Cargo.toml`, `src/main.rs`, `src/lib.rs`, `tests/compat_fixtures.rs`, `tests/fixtures/legacy/**`

- 2. [x] Port the CLI parser and action dispatch contract

  **What to do**: Implement a Rust CLI layer that preserves the legacy action model: exactly one
  action from `-h/-v/-l/-d/-b/-r/-ri`, one optional action argument, and `-L <socket-name>` allowed
  before or after the action. Route parsed actions into explicit Rust use-case entrypoints instead
  of ad hoc branching in `main`, and make invalid combinations fail deterministically with a nonzero
  exit code. Keep `-h` and `-v` as direct terminal actions; keep the action vocabulary unchanged
  even if the internal parser uses a modern crate such as `clap`. **Must NOT do**: Do not convert
  the interface into subcommands; do not drop support for `-L` after the action; do not implement
  hidden aliases.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Reason: exact CLI semantics matter, but the code change is still
    localized.
  - Skills: `[]` — Standard Rust CLI work; no specialty skill required.
  - Omitted: [`playwright`, `git-master`] — No browser or git workflow work is involved.

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: 3, 4, 5, 6, 7, 8, 9, 10 | Blocked By: 1

  **References** (executor has NO interview context — be exhaustive):
  - Pattern: `ref/retmux/retmux:67-93` — Source-of-truth parsing semantics for
    socket/action/action_arg.
  - Pattern: `ref/retmux/retmux:96-131` — Action registry and exit behavior for valid/invalid
    invocation paths.
  - Test: `ref/retmux/tests/test_cli_socket.py:40-67` — Existing tests for `-L` before/after action
    and socket prefix usage.
  - Pattern: `src/main.rs:1-7` — Replace starter main with a thin CLI entrypoint only.
  - Pattern: `src/lib.rs:1-13` — Replace starter demo exports with real application modules.

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test --test cli_contract`
  - [ ] `cargo run --quiet --bin retmux -- -h >/tmp/retmux-help.txt && test -s /tmp/retmux-help.txt`
  - [ ] `cargo run --quiet --bin retmux -- -L >/tmp/retmux-l-missing.txt 2>&1; test $? -ne 0`

  **QA Scenarios** (MANDATORY — task incomplete without these):
  ```
  Scenario: -L works before and after the action
    Tool: Bash
    Steps: cargo test --test cli_contract socket_can_appear_before_or_after_action -- --exact --nocapture
    Expected: parser returns the same socket/action/action_arg tuple for `retmux -L sockA -b backup_20240101_120000` and `retmux -b backup_20240101_120000 -L sockA`.
    Evidence: .sisyphus/evidence/task-2-cli-contract.txt

  Scenario: Missing or over-specified arguments fail deterministically
    Tool: Bash
    Steps: cargo test --test cli_contract invalid_argument_shapes_exit_nonzero -- --exact --nocapture
    Expected: tests cover missing `-L` value, too many arguments, and unknown action combinations, each producing a nonzero exit path.
    Evidence: .sisyphus/evidence/task-2-cli-contract-error.txt
  ```

  **Commit**: YES | Message: `feat(cli): port legacy flag parsing and dispatch` | Files:
  `src/main.rs`, `src/lib.rs`, `src/cli.rs`, `tests/cli_contract.rs`

- 3. [x] Port legacy config bootstrap and active backup-path resolution

  **What to do**: Implement the `.retmux` config subsystem in Rust: `~/.retmux`, `retmux.conf`,
  default backup root, named-socket backup root with sanitization, config bootstrap when the file is
  missing, and `content.with.escape` parsing. Load config before action execution so behavior
  mirrors the reference entrypoint. Keep backup path derivation centralized so every
  backup/list/delete/restore flow reads the same active root from one module. **Must NOT do**: Do
  not move config under XDG paths; do not use different directory names; do not ignore malformed
  config silently.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Reason: path and config compatibility are central to migration
    safety.
  - Skills: `[]` — Repo-local config/path work only.
  - Omitted: [`playwright`, `git-master`] — No browser or git workflow work is involved.

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 4, 6, 7, 8, 9, 10 | Blocked By: 1, 2

  **References** (executor has NO interview context — be exhaustive):
  - Pattern: `ref/retmux/tmuxbk/config.py:17-23` — User path, backup root, named-socket root, and
    config-file path.
  - Pattern: `ref/retmux/tmuxbk/config.py:32-56` — Socket-name activation, command-prefix updates,
    sanitization, and active backup-path selection.
  - Pattern: `ref/retmux/tmuxbk/config.py:59-105` — Config bootstrap and `content.with.escape`
    loading.
  - Pattern: `ref/retmux/tmuxbk/conf/default.conf:1-21` — Default config file contents and option
    names.
  - Test: `ref/retmux/tests/test_cli_socket.py:69-98` — Default vs named-socket backup root
    expectations.

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test --test config_paths`
  - [ ] `TMP_HOME="$(mktemp -d)" && HOME="$TMP_HOME" cargo run --quiet --bin retmux -- -v >/tmp/retmux-version.txt && test -f "$TMP_HOME/.retmux/retmux.conf"`
  - [ ] `cargo test --test config_paths named_socket_uses_sanitized_backup_root -- --exact --nocapture`

  **QA Scenarios** (MANDATORY — task incomplete without these):
  ```
  Scenario: Missing config is bootstrapped into ~/.retmux
    Tool: Bash
    Steps: TMP_HOME="$(mktemp -d)" && HOME="$TMP_HOME" cargo run --quiet --bin retmux -- -v && test -f "$TMP_HOME/.retmux/retmux.conf"
    Expected: the command succeeds, `.retmux` is created, and `retmux.conf` is copied from the Rust-side default template.
    Evidence: .sisyphus/evidence/task-3-config-bootstrap.txt

  Scenario: Malformed config produces a deterministic failure path
    Tool: Bash
    Steps: cargo test --test config_paths malformed_config_is_reported -- --exact --nocapture
    Expected: malformed config content is surfaced as an explicit error instead of silently falling back to unrelated defaults.
    Evidence: .sisyphus/evidence/task-3-config-bootstrap-error.txt
  ```

  **Commit**: YES | Message: `feat(config): add legacy config and socket-aware paths` | Files:
  `src/config.rs`, `src/lib.rs`, `src/main.rs`, `assets/retmux.default.conf` or equivalent in-crate
  default asset, `tests/config_paths.rs`

- 4. [x] Port backup catalog, latest-backup selection, named list, and named delete flows

  **What to do**: Implement the backup catalog helpers and the non-interactive parts of `-l` / `-d`:
  enumerate only the active socket directory, sort/show backups deterministically, resolve the
  latest backup when needed, render detailed information for `-l <name>`, and delete `-d <name>`
  safely. Structure the code so the later interactive layer can reuse the same catalog/query
  primitives instead of duplicating logic. **Must NOT do**: Do not make `-l` without an argument
  interactive in this task; do not scan all socket roots together; do not delete across socket
  boundaries.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Reason: this is user-visible behavior that depends on the new
    config/model layers.
  - Skills: `[]` — Standard repo-local implementation.
  - Omitted: [`playwright`, `git-master`] — No browser or git-history work applies.

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: 9, 10 | Blocked By: 2, 3, 5

  **References** (executor has NO interview context — be exhaustive):
  - Pattern: `ref/retmux/tmuxbk/controller.py:18-50` — Summary listing and latest-backup marker
    behavior.
  - Pattern: `ref/retmux/tmuxbk/controller.py:52-90` — Named detail rendering path used by
    `-l <name>`.
  - Pattern: `ref/retmux/tmuxbk/controller.py:93-110` — Delete flow for a named backup.
  - Pattern: `ref/retmux/tmuxbk/controller.py:153-188` — Validation rules for missing/nonexistent
    backups and latest-backup selection.
  - Pattern: `ref/retmux/tmuxbk/util.py:56-63` — Backup deletion helper.
  - Pattern: `ref/retmux/tmuxbk/util.py:116-129` — Active-socket backup enumeration and
    latest-backup selection.
  - Pattern: `ref/retmux/tmuxbk/tmux_obj.py:8-53` — Short/long info formatting data model that named
    listing depends on.

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test --test catalog_ops`
  - [ ] `cargo test --test catalog_ops named_socket_listing_is_isolated -- --exact --nocapture`
  - [ ] `cargo test --test catalog_ops delete_missing_backup_fails -- --exact --nocapture`

  **QA Scenarios** (MANDATORY — task incomplete without these):
  ```
  Scenario: Named listing and delete stay inside the active socket root
    Tool: Bash
    Steps: cargo test --test catalog_ops named_socket_listing_is_isolated -- --exact --nocapture && cargo test --test catalog_ops delete_named_backup_succeeds -- --exact --nocapture
    Expected: only backups from the active socket root are shown/deleted; default-root backups remain untouched.
    Evidence: .sisyphus/evidence/task-4-catalog-ops.txt

  Scenario: Missing named backup reports an error
    Tool: Bash
    Steps: cargo test --test catalog_ops delete_missing_backup_fails -- --exact --nocapture
    Expected: the command path returns a deterministic error for a nonexistent backup name and does not mutate the filesystem.
    Evidence: .sisyphus/evidence/task-4-catalog-ops-error.txt
  ```

  **Commit**: YES | Message: `feat(catalog): port named list and delete flows` | Files:
  `src/catalog.rs`, `src/cli.rs`, `src/lib.rs`, `tests/catalog_ops.rs`

- 5. [x] Implement the legacy tmux snapshot model and Python-compatible JSON layer

  **What to do**: Define Rust data structures for `Tmux`, `Session`, `Window`, and `Pane`,
  preserving the legacy identity fields, reverse-window ordering helper, pane-id string, and JSON
  compatibility contract needed to load reference backups. Provide encode/decode helpers that can
  read Python-generated snapshots and write Rust snapshots in the same on-disk shape expected by
  later backup/restore tasks. **Must NOT do**: Do not flatten away legacy fields that the restore
  flow needs; do not replace class/module markers with a new schema; do not use lossy parsing for
  pane/session identifiers.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Reason: the data model is foundational and directly affects both
    backup and restore behavior.
  - Skills: `[]` — Standard Rust serialization/modeling work.
  - Omitted: [`playwright`, `git-master`] — No browser or git-history work is involved.

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 4, 7, 8, 10 | Blocked By: 1, 2

  **References** (executor has NO interview context — be exhaustive):
  - Pattern: `ref/retmux/tmuxbk/tmux_obj.py:8-53` — `Tmux` summary/detail formatting and
    session/window/pane structure.
  - Pattern: `ref/retmux/tmuxbk/tmux_obj.py:56-104` — `Session.windows_in_reverse()`,
    `Window.min_pane_id()`, and `Pane.idstr()` helpers.
  - Pattern: `ref/retmux/tmuxbk/util.py:12-45` — Python object serialization/deserialization
    contract with `__class__` and `__module__`.
  - Pattern: `ref/retmux/tmuxbk/util.py:90-109` — JSON save/load helpers and file naming.
  - Pattern: `ref/retmux/tmuxbk/backup.py:21-35` — Snapshot shape written during backup.

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test --test compat_model`
  - [ ] `cargo test --test compat_fixtures rust_decodes_python_snapshot_fixture -- --exact --nocapture`
  - [ ] `cargo test --test compat_model rust_round_trip_preserves_legacy_keys -- --exact --nocapture`

  **QA Scenarios** (MANDATORY — task incomplete without these):
  ```
  Scenario: Rust decodes Python-generated snapshot fixtures
    Tool: Bash
    Steps: cargo test --test compat_fixtures rust_decodes_python_snapshot_fixture -- --exact --nocapture
    Expected: a Python-shaped fixture containing sessions, windows, panes, and legacy metadata decodes into the Rust model without manual migration.
    Evidence: .sisyphus/evidence/task-5-compat-model.txt

  Scenario: Unknown or malformed legacy fields are rejected cleanly
    Tool: Bash
    Steps: cargo test --test compat_model malformed_legacy_snapshot_is_rejected -- --exact --nocapture
    Expected: malformed JSON or invalid pane/session identifiers return typed errors instead of panicking or silently dropping state.
    Evidence: .sisyphus/evidence/task-5-compat-model-error.txt
  ```

  **Commit**: YES | Message: `feat(model): add legacy snapshot schema compatibility` | Files:
  `src/model.rs`, `src/serde_legacy.rs`, `src/lib.rs`, `tests/compat_model.rs`,
  `tests/compat_fixtures.rs`

- 6. [x] Implement the synchronous tmux command adapter and typed subprocess errors

  **What to do**: Port the tmux command layer into Rust as a single adapter module that owns command
  rendering, optional `-L <socket>` prefix insertion, stdout capture, stderr capture, exit-status
  handling, and timeout-aware failures. Recreate the legacy command surface needed by later tasks:
  list sessions/windows/panes, create/kill sessions, create/split/rename/move/select windows, select
  layout, capture panes, clear pane history, send `cd`/`cat` content, and query options such as
  `base-index`. Keep the implementation synchronous and command-list based; every call must avoid
  shell interpolation. **Must NOT do**: Do not call `sh -c`; do not introduce async process
  execution; do not inline tmux commands throughout use-case code.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Reason: this is the critical integration seam between Rust and
    the live tmux server.
  - Skills: `[]` — Repo-local systems work only.
  - Omitted: [`playwright`, `git-master`] — No browser or git-history work is needed.

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: 7, 8, 10 | Blocked By: 2, 3

  **References** (executor has NO interview context — be exhaustive):
  - Pattern: `ref/retmux/tmuxbk/cmd.py:14-60` — Canonical tmux command templates and parameter
    ordering.
  - Pattern: `ref/retmux/tmuxbk/cmd.py:63-69` — Prefix insertion for optional socket-aware tmux
    invocations.
  - Pattern: `ref/retmux/tmuxbk/cmd.py:71-216` — Required helper surface for has-server,
    create/split/layout, pane capture, and content restore.
  - Test: `ref/retmux/tests/test_cli_socket.py:62-67` — Expected command shape when `-L custom` is
    active.
  - Pattern: `ref/retmux/tmuxbk/config.py:32-42` — Source-of-truth for active tmux command prefix.

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test --test tmux_adapter`
  - [ ] `cargo test --test tmux_adapter socket_prefix_is_inserted -- --exact --nocapture`
  - [ ] `cargo test --test tmux_adapter missing_tmux_binary_returns_typed_error -- --exact --nocapture`

  **QA Scenarios** (MANDATORY — task incomplete without these):
  ```
  Scenario: Socket-aware command rendering matches legacy prefix semantics
    Tool: Bash
    Steps: cargo test --test tmux_adapter socket_prefix_is_inserted -- --exact --nocapture
    Expected: rendered commands begin with `tmux -L sockA ...` whenever a named socket is active and remain plain `tmux ...` otherwise.
    Evidence: .sisyphus/evidence/task-6-tmux-adapter.txt

  Scenario: Missing tmux binary or command failure yields a typed error
    Tool: Bash
    Steps: cargo test --test tmux_adapter missing_tmux_binary_returns_typed_error -- --exact --nocapture
    Expected: the adapter returns a deterministic error variant with exit/status context instead of panicking or swallowing stderr.
    Evidence: .sisyphus/evidence/task-6-tmux-adapter-error.txt
  ```

  **Commit**: YES | Message: `feat(tmux): add synchronous tmux adapter` | Files: `src/tmux.rs`,
  `src/error.rs`, `src/lib.rs`, `tests/tmux_adapter.rs`

- 7. [x] Port backup capture and legacy backup-tree writing

  **What to do**: Implement `-b [name]` in Rust: choose the provided backup id or generate the
  legacy timestamp format, reject duplicate backup ids, query the live tmux server for
  sessions/windows/panes, serialize a legacy-compatible snapshot to `<backup_id>.json`, and capture
  pane contents into sibling files named by `session:window.pane`. Respect `content.with.escape` by
  mapping it to the `capture-pane` flag behavior, and preserve the legacy no-server behavior of
  logging that nothing was backed up and exiting successfully. **Must NOT do**: Do not skip pane
  content files; do not write outside the active backup root; do not auto-overwrite an existing
  backup id.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Reason: this is the first live end-to-end behavior slice using
    config, model, and tmux integration together.
  - Skills: `[]` — Standard repo-local implementation.
  - Omitted: [`playwright`, `git-master`] — No browser or git-history work applies.

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: 10 | Blocked By: 2, 3, 5, 6

  **References** (executor has NO interview context — be exhaustive):
  - Pattern: `ref/retmux/tmuxbk/backup.py:14-37` — Backup entrypoint, timestamp handling, JSON
    write, and pane capture loop.
  - Pattern: `ref/retmux/tmuxbk/backup.py:39-95` — Session/window/pane enumeration logic.
  - Pattern: `ref/retmux/tmuxbk/cmd.py:35-37` — `capture-pane -S-100000` legacy content capture
    behavior.
  - Pattern: `ref/retmux/tmuxbk/config.py:52-56` — Active backup-path selection for default vs named
    socket roots.
  - Pattern: `ref/retmux/tmuxbk/util.py:90-99` — JSON file creation and parent-directory bootstrap.

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test --test backup_capture`
  - [ ] `cargo test --test backup_capture creates_legacy_backup_tree -- --exact --nocapture`
  - [ ] `cargo test --test backup_capture no_server_exits_cleanly -- --exact --nocapture`

  **QA Scenarios** (MANDATORY — task incomplete without these):
  ```
  Scenario: Backup writes legacy JSON and pane files into the active socket root
    Tool: Bash
    Steps: cargo test --test backup_capture creates_legacy_backup_tree -- --exact --nocapture
    Expected: with socket `sockA` and backup id `backup_20240101_120000`, the test finds `.retmux/backup-sockets/sockA/backup_20240101_120000/backup_20240101_120000.json` plus pane-content files next to it.
    Evidence: .sisyphus/evidence/task-7-backup-capture.txt

  Scenario: Duplicate backup id or missing tmux server is handled safely
    Tool: Bash
    Steps: cargo test --test backup_capture duplicate_backup_id_fails -- --exact --nocapture && cargo test --test backup_capture no_server_exits_cleanly -- --exact --nocapture
    Expected: duplicate ids fail without overwriting files, and no-server backup returns the planned clean no-op behavior.
    Evidence: .sisyphus/evidence/task-7-backup-capture-error.txt
  ```

  **Commit**: YES | Message: `feat(backup): port tmux snapshot capture` | Files: `src/backup.rs`,
  `src/tmux.rs`, `src/model.rs`, `src/cli.rs`, `tests/backup_capture.rs`

- 8. [x] Port non-interactive restore, latest-backup fallback, and conflict handling

  **What to do**: Implement the Rust restore engine for `-r [name]`: validate the requested backup
  id, default to the latest backup when the name is omitted, load the legacy snapshot, detect
  whether a tmux server exists, create a dummy session when needed to read `base-index`, restore
  windows in reverse order, rebuild pane layouts, restore pane paths and contents, skip sessions
  that already exist in the target tmux server, and remove the dummy session at the end. Preserve
  the decided failure policy: fail fast, keep any already-created tmux state, and report the failure
  clearly. **Must NOT do**: Do not overwrite existing sessions automatically; do not attempt
  rollback of partially restored state; do not ignore missing pane files or malformed JSON.

  **Recommended Agent Profile**:
  - Category: `deep` — Reason: restore sequencing is the highest-risk behavior in the port and needs
    careful ordering.
  - Skills: `[]` — Repo-local implementation only.
  - Omitted: [`playwright`, `git-master`] — No browser or git workflow work is involved.

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: 9, 10 | Blocked By: 2, 3, 5, 6

  **References** (executor has NO interview context — be exhaustive):
  - Pattern: `ref/retmux/tmuxbk/controller.py:123-174` — Restore-name validation and latest-backup
    fallback semantics.
  - Pattern: `ref/retmux/tmuxbk/restore.py:21-33` — Dummy-session bootstrap to read `base-index`
    when no tmux server exists.
  - Pattern: `ref/retmux/tmuxbk/restore.py:36-69` — Top-level restore flow, conflict skip behavior,
    dummy-session cleanup.
  - Pattern: `ref/retmux/tmuxbk/restore.py:72-120` — Reverse window ordering, pane path restore,
    pane content restore, and layout selection.
  - Pattern: `ref/retmux/tmuxbk/cmd.py:40-60` — Pane path restoration and content loading commands.
  - Pattern: `ref/retmux/tmuxbk/tmux_obj.py:63-68` — `windows_in_reverse()` behavior that restore
    depends on.

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test --test restore_integration`
  - [ ] `cargo test --test restore_integration restores_latest_backup_when_name_missing -- --exact --nocapture`
  - [ ] `cargo test --test restore_integration skips_conflicting_session_names -- --exact --nocapture`
  - [ ] `cargo test --test restore_integration malformed_backup_fails_fast -- --exact --nocapture`

  **QA Scenarios** (MANDATORY — task incomplete without these):
  ```
  Scenario: Restore replays a legacy fixture into an isolated tmux socket
    Tool: Bash
    Steps: cargo test --test restore_integration restores_latest_backup_when_name_missing -- --exact --nocapture
    Expected: the test boots an isolated socket, restores the latest fixture backup, and verifies session/window/pane topology plus expected layout restoration.
    Evidence: .sisyphus/evidence/task-8-restore-integration.txt

  Scenario: Conflict and corruption cases fail the planned way
    Tool: Bash
    Steps: cargo test --test restore_integration skips_conflicting_session_names -- --exact --nocapture && cargo test --test restore_integration malformed_backup_fails_fast -- --exact --nocapture
    Expected: existing target sessions are skipped without overwrite, malformed backups fail immediately, and any already-created tmux state is left intact for inspection.
    Evidence: .sisyphus/evidence/task-8-restore-integration-error.txt
  ```

  **Commit**: YES | Message: `feat(restore): port legacy restore sequencing` | Files:
  `src/restore.rs`, `src/cli.rs`, `src/tmux.rs`, `src/model.rs`, `tests/restore_integration.rs`

- 9. [x] Port interactive list/delete/restore flows and stabilize user-facing output

  **What to do**: Implement the interactive behaviors deferred from earlier tasks: `-l` without an
  argument, `-d` without an argument, and `-ri`. Reuse the shared catalog/restore/delete primitives
  so interactive mode is only a thin stdin/stdout layer. Port the help/version output to cover the
  same option inventory and config-path messaging as the reference tool, and keep log/highlight
  output readable in terminals without requiring exact legacy ANSI-art byte parity. Handle invalid
  input and EOF deterministically. **Must NOT do**: Do not fork a second code path for
  delete/restore logic; do not require manual confirmation outside stdin-driven prompts; do not
  recreate the legacy typo `retumx>` as a required behavior.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Reason: this is user-facing behavior on top of the completed core
    flows.
  - Skills: `[]` — Repo-local CLI/stdin/stdout work only.
  - Omitted: [`playwright`, `git-master`] — No browser or git-history work is involved.

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: 10 | Blocked By: 2, 3, 4, 8

  **References** (executor has NO interview context — be exhaustive):
  - Pattern: `ref/retmux/retmux:14-54` — Help output option inventory and config-file messaging.
  - Pattern: `ref/retmux/tmuxbk/controller.py:52-90` — Interactive list/detail loop.
  - Pattern: `ref/retmux/tmuxbk/controller.py:101-110` — Interactive delete confirmation flow.
  - Pattern: `ref/retmux/tmuxbk/controller.py:129-140` — Interactive restore confirmation flow.
  - Pattern: `ref/retmux/tmuxbk/log.py:13-35` — Highlight/color helpers used in user-facing text.
  - Pattern: `ref/retmux/tmuxbk/tmux_obj.py:22-53` — Detail rendering shape for
    backup/session/window/pane output.

  **Acceptance Criteria** (agent-executable only):
  - [ ] `cargo test --test interactive_flows`
  - [ ] `cargo test --test help_output`
  - [ ] `printf '1\nyes\n' | cargo run --quiet --bin retmux -- -ri >/tmp/retmux-ri.txt`

  **QA Scenarios** (MANDATORY — task incomplete without these):
  ```
  Scenario: Interactive restore completes from scripted stdin
    Tool: Bash
    Steps: cargo test --test interactive_flows interactive_restore_accepts_scripted_input -- --exact --nocapture
    Expected: the no-arg interactive restore flow selects backup `backup_20240101_120000`, accepts `yes`, and reuses the same restore engine verified in Task 8.
    Evidence: .sisyphus/evidence/task-9-interactive-flows.txt

  Scenario: Invalid input and EOF are handled gracefully
    Tool: Bash
    Steps: cargo test --test interactive_flows invalid_input_and_eof_are_reported -- --exact --nocapture
    Expected: empty, non-numeric, and EOF input paths produce deterministic errors/messages and never panic or perform the wrong destructive action.
    Evidence: .sisyphus/evidence/task-9-interactive-flows-error.txt
  ```

  **Commit**: YES | Message: `feat(interactive): port list delete and restore prompts` | Files:
  `src/interactive.rs`, `src/cli.rs`, `src/logging.rs`, `src/lib.rs`, `tests/interactive_flows.rs`,
  `tests/help_output.rs`

- 10. [x] Wire tmux-backed integration tests into repo automation and finalize release-facing polish

  **What to do**: Update the repo automation so the port is verifiable end to end: add tmux
  installation/setup to CI, make `just`/local workflows run the new compatibility and live-tmux
  suites, ensure the binary/install flow clearly targets `retmux`, and remove starter-project
  wording from repo-facing developer documentation that would confuse executors. Keep the release
  surface minimal: `cargo build`, `cargo run --bin retmux`, and
  `cargo install --path . --bin retmux` should be the supported developer paths. **Must NOT do**: Do
  not add a multi-platform release matrix; do not introduce packaging beyond Cargo/local CI in this
  first port; do not keep docs claiming the project is a generic starter crate.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` — Reason: this task hardens the pipeline and delivery surface after
    all behavior is present.
  - Skills: `[]` — Standard repo-local CI/documentation work.
  - Omitted: [`playwright`, `git-master`] — No browser or git-history work is involved.

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: F1-F4 | Blocked By: 1, 2, 3, 4, 5, 6, 7,
  8, 9

  **References** (executor has NO interview context — be exhaustive):
  - Pattern: `justfile:30-38` — Existing build/run entrypoints to keep aligned with the new binary
    name.
  - Pattern: `justfile:73-85` — Current repo verification commands that must remain green and absorb
    the new tests.
  - Pattern: `.github/workflows/ci.yml:10-38` — Current CI job to extend with tmux-backed
    integration coverage.
  - Pattern: `Cargo.toml:1-18` — Package/binary metadata surface consumed by Cargo commands.
  - Pattern: `README.md:1-40` — Starter wording to replace with retmux-specific development
    guidance.
  - Pattern: `ref/retmux/README.md:21-66` — Legacy usage/features reference for release-facing
    wording.

  **Acceptance Criteria** (agent-executable only):
  - [ ] `just check`
  - [ ] `cargo test --all-features`
  - [ ] `cargo install --path . --bin retmux --locked --root /tmp/retmux-install-root`

  **QA Scenarios** (MANDATORY — task incomplete without these):
  ```
  Scenario: Full local verification suite passes with tmux-backed tests enabled
    Tool: Bash
    Steps: just check
    Expected: fmt, dprint, typos, clippy, unit tests, fixture tests, and live tmux integration tests all pass in one local verification run.
    Evidence: .sisyphus/evidence/task-10-ci-polish.txt

  Scenario: Installation flow produces a runnable retmux binary
    Tool: Bash
    Steps: cargo install --path . --bin retmux --locked --root /tmp/retmux-install-root && /tmp/retmux-install-root/bin/retmux -h >/tmp/retmux-installed-help.txt && test -s /tmp/retmux-installed-help.txt
    Expected: the installed binary is named `retmux` and prints non-empty help output.
    Evidence: .sisyphus/evidence/task-10-ci-polish-error.txt
  ```

  **Commit**: YES | Message: `chore(ci): add tmux integration coverage and release polish` | Files:
  `.github/workflows/ci.yml`, `justfile`, `README.md`, `Cargo.toml`, `tests/**`

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get
> explicit "okay" before completing. **Do NOT auto-proceed after verification. Wait for user's
> explicit approval before marking work complete.** **Never mark F1-F4 as checked before getting
> user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.

- [x] F1. Plan Compliance Audit — oracle
- [x] F2. Code Quality Review — unspecified-high
- [x] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [x] F4. Scope Fidelity Check — deep

## Commit Strategy

- Keep commits vertical and revertable; each commit must deliver one compatibility-visible slice
  plus its tests.
- Recommended commit order:
  1. `feat(retmux): freeze legacy fixtures and binary identity`
  2. `feat(cli): port flag parsing and dispatch semantics`
  3. `feat(config): add legacy config and socket-aware backup paths`
  4. `feat(model): add legacy tmux snapshot schema`
  5. `feat(tmux): add synchronous tmux command adapter`
  6. `feat(catalog): port list and delete backup flows`
  7. `feat(backup): port session backup generation`
  8. `feat(restore): port restore sequencing and conflict handling`
  9. `feat(interactive): port interactive list/delete/restore flows`
  10. `chore(ci): add tmux-backed integration coverage and release polish`
- Every commit must end with
  `cargo fmt --all --check && dprint check && typos . && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features`
  passing.

## Success Criteria

- Running the Rust binary as `retmux` covers the same operational workflow as the reference
  implementation for backup, list, delete, restore, and interactive restore.
- Existing legacy backups under `~/.retmux` are consumable by Rust without migration.
- Named socket backups remain isolated and do not leak across sockets.
- tmux restore behavior is deterministic under the supported CI environment and covered by automated
  tests.
- The repository no longer contains starter "Hello, remux!" behavior; the crate is fully repurposed
  for retmux.
