# Pioneer Mods

Unofficial open-source tooling for firmware patch research and controlled patch generation workflows.

## Important disclaimer

This is an **unofficial, experimental** project.  
If you use anything from this repository, you do so **at your own risk and responsibility**.

- Not affiliated with or endorsed by Pioneer DJ / AlphaTheta.
- No official firmware is redistributed here.
- Owner-input workflows only.
- Do **not** rely on experimental builds for live performances.

## Project status

Early stage. APIs, formats, and behavior may change quickly.

## Current implementation status

This repository currently includes:

- Rust workspace scaffolding (`core/patch-core`, `core/patch-schema`, `core/patch-cli`)
- Deterministic firmware identity inspection (`inspect`) command
- Recipe schema baseline and validation primitives
- CI checks for format/lint/test

The patch-application command path is intentionally not released yet.

## Quick start (developer)

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo audit
python scripts/check_file_size_caps.py
```

Inspect owner-supplied firmware identity:

```bash
cargo run -p patch-cli -- inspect --input /path/to/XDJ700.UPD --format json
```

## Code quality baseline

- Rust:
  - `rustfmt` (`rustfmt.toml`)
  - `clippy` (`clippy.toml`)
  - tests (`cargo test`)
  - advisory checks (`cargo audit`)
- Python (when present):
  - `ruff` format + lint
  - `mypy` static typing
  - `pytest`
- Repository hygiene:
  - file-size cap check via `scripts/check_file_size_caps.py`

## Versioning and release docs

- [VERSIONING.md](./VERSIONING.md)
- [RELEASING.md](./RELEASING.md)
- [CHANGELOG.md](./CHANGELOG.md)

## Project governance docs

- [LICENSE](./LICENSE)
- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md)
- [SECURITY.md](./SECURITY.md)
