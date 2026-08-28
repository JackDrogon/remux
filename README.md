# remux

`remux` is a Rust tmux session backup and restore tool. This repository ships the `remux` binary and
stores backups under `~/.remux/` by default.

## Supported baseline

- Linux
- tmux available on `PATH`
- Rust stable toolchain

## Build, run, and install

The supported developer and release-facing entrypoints are intentionally minimal:

```bash
cargo build
cargo run --bin remux -- -h
cargo install --path . --bin remux --locked
```

After installation, the binary is available as `remux`.

## Usage

```bash
remux backup
remux list
remux restore
remux restore --interactive
remux compact
remux -L sockA backup backup_20240101_120000
```

`remux compact` compares the two newest backups in the active catalog and deletes the older
automatic timestamp backup when it is covered by the newer. The coverage rules and field-by-field
rationale are in [docs/compact.md](docs/compact.md).

Backups are written to `~/.remux/backup` by default. When `-L <socket-name>` is active, remux
isolates data under `~/.remux/backup-sockets/<sanitized-socket-name>`.

Operational logs are written to `~/.remux/remux.log`. By default remux keeps console logging off so
CLI stdout/stderr stay stable; adjust the `[logging]` section in `~/.remux/config.toml` when you
need more verbosity during debugging. Console color is `auto` (ANSI when stderr is a TTY); set
`color = "always"` or `color = "never"` to override. File logs are never colored.

## What remux stores

- tmux sessions, including session names and terminal sizes
- windows, including order, names, and layouts
- panes, including working directories and captured content history

## What remux does not restore

- the original running processes inside panes
- shell history for each pane
- tmux buffer stacks

## How remux talks to tmux

remux runs tmux as a synchronous subprocess and waits until that process exits. It does not impose
its own command deadline: a hung tmux hangs remux. The adapter has no timeout API and no timed-out
error. A wait loop that only polls exit status would stall on a full stdout pipe and mis-report
large `capture-pane` output as a timeout.

## Errors

Argument parsing is clap's job: it prints and exits. After that, remux is a small CLI whose errors
are not a second control plane: construct a typed code for the fault, add context so a human can see
where it happened, print one report, and stop. Do not reclassify failures by errno, source, or
context in order to pick a "better" code. Fail clearly and leave the machine consistent. remux does
not publish a partial snapshot. Staging directories are created with exclusive `mkdir` among
cooperating remux instances (a name collision retries; remux never deletes a path it did not just
create). The tree is landed with `renameat2(RENAME_NOREPLACE)` so an occupied backup id is not
replaced. An unpublished staging directory is removed on a handled failure before land; after a
successful land, later fsync errors do not delete the published backup, and the error report says
the snapshot was already published. A crash or power loss may still leave temps. The operator reads
the report and acts.

## Development

```bash
just build
just test
just test-doc
just test-live
just check
```

- `just test` runs the Rust unit/integration suite with `cargo nextest run --all-features` and
  follows up with `cargo test --doc --all-features` so doctests keep matching the old `cargo
  test`
  coverage.
- `just test-doc` runs only the documentation tests that nextest does not execute.
- `just test-live` runs the real tmux integration suite with
  `cargo nextest run --test live_tmux
  --run-ignored only --no-capture`.
- `just check` is the one-shot local gate: rustfmt, dprint, typos, clippy, the standard Rust test
  suite, and the tmux-backed live integration suite.
- Repository-level nextest behavior lives in `.config/nextest.toml`; CI uses the explicit `ci`
  profile from that file.

Install [`cargo-nextest`](https://nexte.st/), [`dprint`](https://dprint.dev/installation/),
[`typos`](https://github.com/crate-ci/typos), and `tmux` locally if you want `just check` and
CI-equivalent verification to pass on your machine.

## Repository layout

```text
.github/workflows/ci.yml   Linux CI with tmux-backed integration coverage
.config/nextest.toml       Repository-level nextest profiles
assets/                    Default config assets bundled into the binary
docs/                      Design notes, including compact coverage
src/                       remux library and binary entrypoint
tests/                     Compatibility, CLI, restore, and live tmux integration tests
ref/retmux/                Reference implementation used for compatibility guidance
```
