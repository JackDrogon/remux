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

The `just test` and `just check` recipes run `cargo test --all-features` so local validation matches CI.

## Pull requests

- Keep changes focused and well tested.
- Update documentation when behavior changes.
- Prefer small, reviewable commits.
