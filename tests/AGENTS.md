# TEST KNOWLEDGE BASE

## TEST TIERS
- **Contracts** (`*_contract.rs`): Argument logic (e.g., `-L` position flexibility) and legacy JSON structure.
- **Integration** (`*_integration.rs` / `*_ops.rs`): Logic flows like `BackupOutcome` and `RestoreEngine` replay.
- **Live** (`live_tmux.rs`): Full-cycle tests with real `tmux`. Requires `just test-live` or `--run-ignored`.
- **Shared Helpers**: `tests/support/mod.rs` (fake-tmux log assertions and environment setup).

## ISOLATION RULES
- **Temp HOME**: Every test that touches `~/.remux/` must run in a unique temp directory using `TempHome`.
- **Unique Socket**: Live tests must use `-L <unique-socket-name>` (derived from PID) to avoid host leakage.
- **No Global Config**: Ensure host `.tmux.conf` does not influence base-index or pane-index values.
- **RUSTUP_HOME/CARGO_HOME**: Point these to real toolchain locations when running binary integration with a custom `HOME`.
- **Ignored Live Tests**: Keep live tests under `#[ignore]` to maintain non-Linux CI portability.

## FIXTURES & MOCKS
- **Legacy Fixtures**: `tests/fixtures/legacy/` for `__class__` and `session:window.pane` validation.
- **Snapshots**: Use `snapshot_contract.rs` to verify that frozen fixtures remain decodable with optional fields.
- **Mock Tmux**: Use `MockTmuxAdapter` to assert command ordering (e.g., `list-windows` before `list-panes`).
- **Binary CLI**: Use `env.run_binary()` to test the actual `remux` executable with an isolated environment.

## NAMING CONVENTIONS
- `*_contract.rs`: Boundary/Format validation (Argument parsing, JSON schema).
- `*_integration.rs`: Multi-module logic flows involving `BackupOutcome` or `RestoreOutcome`.
- `*_ops.rs`: Catalog and directory manipulation tests.
- `live_tmux.rs`: Real process execution requiring a running `tmux` server.

## ANTI-PATTERNS
- **Host Contamination**: Modifying the developer's real `~/.remux/` or `tmux` server.
- **Shared Socket Names**: Using "remux-test" as a static name (leads to parallel execution races).
- **Unchecked Build Logs**: Misinterpreting Cargo's "waiting for lock" message as an execution failure.
