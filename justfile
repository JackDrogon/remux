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
alias tl := test-live
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
    cargo run --bin remux

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
    dprint fmt "README.md" ".github/workflows/ci.yml" "Cargo.toml" ".config/nextest.toml"

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
    cargo nextest run --all-features
    cargo test --doc --all-features

# Run documentation tests that nextest does not execute
[group('test')]
test-doc:
    cargo test --doc --all-features

# Run tmux-backed integration tests against a real tmux server
[group('test')]
test-live:
    cargo nextest run --test live_tmux --run-ignored only --no-capture

# Run the full local verification suite
[group('test')]
check:
    cargo fmt --all --check
    dprint check "README.md" ".github/workflows/ci.yml" "Cargo.toml" ".config/nextest.toml"
    typos .
    cargo clippy --all-targets --all-features -- -D warnings
    cargo nextest run --all-features
    cargo test --doc --all-features
    cargo nextest run --test live_tmux --run-ignored only --no-capture

# Run the same checks before committing
[group('test')]
pre-commit: check

# Build local API documentation
[group('test')]
docs:
    cargo docs
