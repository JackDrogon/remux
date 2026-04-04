# TEST KNOWLEDGE BASE

## OVERVIEW
The `tests/` directory contains a tiered suite of verification tools ranging from pure 
serialization contracts to live, side-effect-heavy integration with `tmux`.

## WHERE TO LOOK
- **CLI/Parsing**: `tests/cli_contract.rs` (argument logic) and `tests/help_output.rs`.
- **Serialization**: `tests/snapshot_contract.rs` (JSON/Legacy compatibility).
- **Core Logic**: `tests/backup_capture.rs`, `tests/restore_integration.rs`, `tests/catalog_ops.rs`.
- **Live Integration**: `tests/live_tmux.rs` (real server interactions).
- **Shared Helpers**: `tests/support/mod.rs` (mock generators).

## CONVENTIONS
- **Runner**: Use `cargo nextest` for parallel execution of unit and standard integration tests.
- **Doctests**: Must be run separately via `cargo test --doc` (or `just test-doc`).
- **Isolation**: Live tests MUST use a temporary `HOME` and unique `-L <socket>` to avoid 
  leakage from the developer's host environment (e.g., custom `.tmux.conf`).
- **Naming**: 
  - `*_contract.rs`: Boundary/Format validation.
  - `*_integration.rs`: Multi-module logic flows.
  - `live_tmux.rs`: Real process execution against `tmux`.
- **Fixtures**: Committed under `tests/fixtures/` for deterministic legacy validation.

## ANTI-PATTERNS
- **Global HOME**: Never run integration tests that touch `~/.remux/` on the host.
- **Shared Sockets**: Avoid using the default tmux socket in tests; always generate a 
  unique name per test run.
- **Live in Default Suite**: Do not remove `#[ignore]` from live tests; they must remain 
  opt-in to keep `cargo test` portable.

## NOTES
- **nextest config**: Repository behavior is tuned in `.config/nextest.toml`.
- **CI Flow**: CI runs the full suite including `live_tmux`.
- **Environment**: If `cargo` freezes, it may be waiting for a build-directory lock.
