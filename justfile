#!/usr/bin/env -S just --justfile

# Justfile for remux
# Run `just` or `just --list` to see available recipes

# ─────────────────────────────────────────────────────────────────────
# Aliases
# ─────────────────────────────────────────────────────────────────────

alias b := build
alias r := run
alias t := test
alias f := fmt
alias fr := fmt-repo
alias l := lint
alias c := check
alias d := docs
alias s := spellcheck
alias pc := pre-commit

# Show all available recipes
[private]
default:
    @just --list --unsorted

# ═════════════════════════════════════════════════════════════════════
#  Build
# ═════════════════════════════════════════════════════════════════════

# Build the binary in debug mode
[group('build')]
build:
    cargo build

# Build and run the application
[group('build')]
run:
    cargo run

# Remove Cargo build artifacts
[group('build')]
clean:
    cargo clean

# ═════════════════════════════════════════════════════════════════════
#  Quality
# ═════════════════════════════════════════════════════════════════════

# Format Rust sources
[group('quality')]
fmt:
    cargo fmt --all

# Format repository-level TOML, JSON, Markdown, and YAML files
[group('quality')]
fmt-repo:
    dprint fmt

# Run Clippy lints
[group('quality')]
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Run spelling checks across the repository
[group('quality')]
spellcheck:
    typos .

# ═════════════════════════════════════════════════════════════════════
#  Test
# ═════════════════════════════════════════════════════════════════════

# Run unit and integration tests
[group('test')]
test:
    cargo test --all-features

# Run the full local verification suite
[group('test')]
check:
    cargo fmt --all --check
    dprint check
    typos .
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-features

# Run the same checks before committing
[group('test')]
pre-commit: check

# Build local API documentation
[group('test')]
docs:
    cargo docs
