# Contributing to remux

Thanks for your interest in contributing.

## Local checks

Run the same checks locally that CI expects:

```bash
just fmt
just fmt-repo
just lint
just spellcheck
just pre-commit
just test
```

The `just test` and `just check` recipes run `cargo nextest run --all-features` and
`cargo test --doc --all-features`, keeping doctest coverage aligned with CI.

Repository-level nextest configuration lives in `.config/nextest.toml`. CI explicitly uses the `ci`
profile from that file.

## Pull requests

- Keep changes focused and well tested.
- Update documentation when behavior changes.
- Prefer small, reviewable commits.
