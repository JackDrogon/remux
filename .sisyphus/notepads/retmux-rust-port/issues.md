# Issues

- No blocking implementation issues were encountered in Task 1 after aligning the corrupt-fixture
  test to validate against a caller-supplied fixture root.
- Verification commands run under `zsh`, where `status` is a read-only shell variable; use a
  different variable name like `exit_code` when scripting nonzero-exit assertions.
- The committed default-socket fixture omits `Window.name` even though the Python serializer would
  normally emit it from `Window.__dict__`; the Rust compatibility layer now treats that field as
  optional so frozen fixtures remain decodable without relaxing marker/type validation.
- In this container, `cargo` is managed by `rustup`, so a literal `HOME="$TMP_HOME" cargo ...`
  verification command fails before the binary starts unless `RUSTUP_HOME` and `CARGO_HOME` are
  pointed at the real toolchain directories; this is an environment constraint, not a retmux
  bootstrap bug.
- No blocking implementation issues were encountered in Task 6 after keeping subprocess execution on
  `std::process::Command` argument vectors and testing the missing-binary path with a unique
  nonexistent executable path instead of relying on PATH mutation.
- Task 7 test harnesses must scan tmux arguments for `-t...` instead of assuming the target is
  always the second positional argument; the Rust adapter keeps the legacy order where
  `list-windows` and `list-panes` place `-F...` before `-t...`.
- No blocking implementation issues were encountered in Task 4 after keeping directory discovery
  rooted in `active_backup_path()` and exercising CLI integration through the compiled `retmux`
  binary with a temp `HOME` instead of trying to spoof cargo-level HOME changes.
- During Task 8 verification, the first restore integration failures came from the fake-tmux log
  assertion format rather than restore behavior; the helper logs raw command lines separated by
  spaces, so command-order checks should assert against those literal strings instead of introducing
  an alternate delimiter.
- Parallel `cargo` verification commands can briefly block on the shared build-directory lock in
  this environment; the wait is harmless, but evidence logs should record it so a future reviewer
  does not mistake the pause for a Task 4 failure.
- No blocking implementation issues were encountered in Task 7 after re-checking the legacy
  controller order: duplicate backup-name validation still belongs before the tmux-server probe, so
  the only missing parity gap was a deterministic CLI success message for the no-server branch.
- Task 8 verification can hit the shared Cargo target lock when multiple acceptance commands are
  launched close together; the evidence logs now show harmless
  `Blocking waiting for file lock on build directory` lines, which are environmental and not restore
  failures.
- A snapshot JSON may carry a `tid` that differs from the directory chosen for restore tests; if the
  implementation uses `tmux.tid` for pane-file paths, restore fails against the wrong directory even
  though the selected backup name is valid.
- Task 9 scripted `cargo run -- -ri` verification still needs a prepared temp HOME plus a fake
  `tmux` on `PATH`; when HOME is overridden in this environment, keep `RUSTUP_HOME` and `CARGO_HOME`
  pointed at the real toolchain so the acceptance command exercises retmux itself instead of failing
  inside rustup bootstrap.
- No blocking implementation issues were encountered in Task 9 after extracting a shared
  selection/confirmation loop; the main adjustment was removing a test-only unused import so
  evidence logs stayed free of avoidable warnings.
- Task 9 changed `retmux -l` without an argument into an EOF-sensitive interactive flow, so any
  older integration test that shells out to bare `-l` must be updated to validate catalog semantics
  through named lookups or lower-level APIs instead of assuming summary-only stdout.
- This environment did not have `dprint` installed initially, so `just check` could not execute
  until the CLI was installed locally; the repo automation changes themselves were valid once
  `dprint` was present on `PATH`.
- Task 10 Phase 1 scope review flagged incidental `CONTRIBUTING.md` and `typos.toml` formatting as out-of-scope and required explicit reversion.
- No blocking implementation issues were encountered in the F2 blocker fix once backup-name normalization was centralized; the key compatibility constraint was to reject invalid explicit names while still preserving the legacy timestamp fallback only for omitted `-b` values.
