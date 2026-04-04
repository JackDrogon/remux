# PROJECT KNOWLEDGE BASE

Generated: 2026-04-04 / 8894101 / main

## OVERVIEW
`remux` is a Rust-based tmux session backup and restore tool. It is designed as a high-performance, 
type-safe port of the Python `retmux` utility, maintaining full structural compatibility with legacy 
backup formats while providing a robust CLI for Linux environments.

## PROJECT MAP
- `src/`: Core logic and binary entry point (`main.rs`, `lib.rs`).
- `tests/`: Integration suite (`live_tmux.rs`, `cli_contract.rs`).
- `assets/`: Bundled default configurations.
- `ref/retmux/`: Reference Python implementation for behavior parity.
- `.config/`: Tool-specific configurations (e.g., nextest).

## WHERE TO LOOK
- **Source implementation**: Start with `src/AGENTS.md` for module boundaries, persistence, tmux interaction, and runtime path logic.
- **Test behavior**: Start with `tests/AGENTS.md` for fixture policy, isolation rules, and live-tmux verification.
- **Legacy parity**: Start with `ref/retmux/AGENTS.md` when behavior must match the Python reference implementation.
- **Daily commands**: Use `justfile` for local recipes; CI expectations live in `.github/workflows/ci.yml`.

## DEVELOPMENT GUIDELINES
This project uses `just` as the canonical daily entrypoint so local runs stay aligned with CI flags and recipe ordering.
- **Default**: Use `just <recipe>` for routine workflows (build, test, lint, format, check, run).
- **Exceptions**: Raw toolchain commands (`cargo`, `clippy`, `nextest`, `dprint`, `typos`) are allowed only when debugging a recipe gap or investigating CI-only flag behavior.
- **Reporting**: When using a raw command, explicitly report why `just` was insufficient and which raw command was used.
- **Daily Entrypoints**:
  - `just build`: Standard debug build.
  - `just run -- -h`: View all CLI actions (Backup, List, Restore, Interactive).
  - `just test`: Runs unit tests and doctests.
  - `just test-live`: Runs integration tests against a real tmux server (requires `tmux`).
  - `just check`: Full local gate (fmt, dprint, typos, clippy, test, test-doc, test-live).

## CONVENTIONS
- **Backup Roots**: Default to `~/.remux/`. Use socket-based isolation under `backup-sockets/`.
- **Compatibility**: Must decode legacy JSON markers (`__class__`) and handle optional `Window.name`.
- **Execution flow**: Root AGENTS defines repo-wide rules; implementation and test specifics belong in the child AGENTS files.
- **Section shape**: Child AGENTS should use domain-appropriate sections and do not need to mirror root headings like `DEVELOPMENT GUIDELINES` or `NOTES` unless those sections add real signal.

## ANTI-PATTERNS (THIS PROJECT)
- **Leaking Host State**: Never run live tests without a temporary `HOME`.
- **Duplicate Path Logic**: Always derive paths through `RuntimeConfig` to ensure socket isolation.
- **Routine Raw Toolchain Use**: Do not bypass `just` for normal build/test/lint/format/check workflows.

## UNIQUE STYLES
- **Sync Over Async**: Uses `std::process::Command` for deterministic tmux interaction.
- **Telegraphic CLI**: Minimalist output; errors use stable messages for legacy parity.

## NOTES
- **Runtime**: Linux + tmux on `PATH` + Rust stable (1.85+).
- **CI**: Enforces strict formatting (rustfmt + dprint) and full test coverage.
- **Legacy Path**: Ported from `ref/retmux`; behavior must match Python implementation exactly.
