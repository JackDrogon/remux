# retmux

`retmux` is a Rust tmux session backup and restore tool. This repository ships the `retmux` binary,
keeps the legacy CLI surface, and preserves the on-disk backup layout under `~/.retmux/` so existing
backups remain usable.

## Supported baseline

- Linux
- tmux available on `PATH`
- Rust stable toolchain

## Build, run, and install

The supported developer and release-facing entrypoints are intentionally minimal:

```bash
cargo build
cargo run --bin retmux -- -h
cargo install --path . --bin retmux --locked
```

After installation, the binary is available as `retmux`.

## Usage

```bash
retmux -b
retmux -l
retmux -r
retmux -L sockA -b backup_20240101_120000
```

Backups are written to `~/.retmux/backup` by default. When `-L <socket-name>` is active, retmux
isolates data under `~/.retmux/backup-sockets/<sanitized-socket-name>`.

## What retmux stores

- tmux sessions, including session names and terminal sizes
- windows, including order, names, and layouts
- panes, including working directories and captured content history

## What retmux does not restore

- the original running processes inside panes
- shell history for each pane
- tmux buffer stacks

## Development

```bash
just build
just test
just test-live
just check
```

- `just test` runs the Rust unit/integration suite with `cargo test --all-features`.
- `just test-live` runs the real tmux integration suite that is ignored by default under plain
  `cargo test`.
- `just check` is the one-shot local gate: rustfmt, dprint, typos, clippy, the standard Rust test
  suite, and the tmux-backed live integration suite.

Install [`dprint`](https://dprint.dev/installation/), [`typos`](https://github.com/crate-ci/typos),
and `tmux` locally if you want `just check` and CI-equivalent verification to pass on your machine.

## Repository layout

```text
.github/workflows/ci.yml   Linux CI with tmux-backed integration coverage
assets/                    Default config assets bundled into the binary
src/                       retmux library and binary entrypoint
tests/                     Compatibility, CLI, restore, and live tmux integration tests
ref/retmux/                Reference implementation used for compatibility guidance
```
