# Test Patterns and Conventions in Remux

## Test Framework and Runner
- **Primary Runner**: `cargo nextest` is used for unit and integration tests (via `just test`).
- **Doc Tests**: `cargo test --doc` is used to run documentation tests, as `nextest` does not support them.
- **Live Integration**: `cargo nextest run --test live_tmux --run-ignored only --no-capture` is used for tests that interact with a real tmux server.

## Directory Structure
- `src/`: Contains unit tests (usually within the same file or in a `tests` module).
- `tests/`: Integration tests.
    - `tests/*.rs`: Individual integration test suites (e.g., `backup_capture.rs`, `cli_contract.rs`).
    - `tests/support/mod.rs`: Shared test utilities and helper functions.
    - `tests/fixtures/`: Static data for tests, including legacy backup formats for compatibility testing.

## Naming Conventions
- Integration test files are named based on the functionality they test (e.g., `restore_integration.rs`, `snapshot_contract.rs`).
- Support module is at `tests/support/mod.rs`.

## Unique Patterns
- **Live Environment Management**: `live_tmux.rs` uses a `LiveTmuxEnv` struct to create isolated `HOME` and workspace directories, and manages a dedicated tmux socket for tests.
- **Ignored Tests**: Tests that require a real tmux server are marked with `#[ignore = "..."]` to prevent failure in environments without tmux.
- **Binary Integration**: Tests use `env!("CARGO_BIN_EXE_remux")` to invoke the compiled binary.
- **Fixture Support**: `tests/support/mod.rs` provides builders like `single_window_tmux` to programmatically create `Tmux` model instances for testing.
