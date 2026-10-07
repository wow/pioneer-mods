# Contributing

Thanks for your interest in contributing.

This project is unofficial and experimental. Please read [README.md](./README.md), [VERSIONING.md](./VERSIONING.md), and [RELEASING.md](./RELEASING.md) before opening significant changes.

## Ground rules

1. **No vendor firmware redistribution** in issues, PRs, or repository files.
2. Follow owner-input patching and compatibility-gated behavior.
3. Keep changes focused and easy to review.
4. Add or update tests for behavior changes.
5. Update documentation when behavior, compatibility, or release process changes.

## Development flow

1. Open an issue (or discussion) for non-trivial work.
2. Create a feature branch.
3. Implement the change with tests.
4. Run relevant checks locally.
5. Update [CHANGELOG.md](./CHANGELOG.md) under `Unreleased` when appropriate.
6. Open a pull request with:
   - what changed,
   - why it changed,
   - how it was validated,
   - known limitations and risks.

## Local validation commands

Run from repository root:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo audit
python scripts/check_file_size_caps.py
```

If Python files are present:

```bash
python -m pip install --upgrade ruff mypy pytest
ruff format --check .
ruff check .
mypy scripts
pytest -q
```

## Coding standards

- Rust and Python line-length target is 100 columns.
- Prefer small modules; file-size caps are enforced by `scripts/check_file_size_caps.py`.
- Keep parser/patch behavior fail-closed (explicit errors, no silent fallback).
- Keep compatibility contracts strict (hash/version/size gated).

## Pull request checklist

- [ ] Scope is clear and bounded.
- [ ] Backward-compatibility impact is explained.
- [ ] Determinism and refusal paths are considered where relevant.
- [ ] Tests and docs are updated.
- [ ] Changelog entry added when relevant.

## Safety and release expectations

Contributors should treat release quality conservatively:
- avoid risky shortcuts around compatibility gates,
- call out uncertainty explicitly,
- prefer prerelease channels for experimental features.
