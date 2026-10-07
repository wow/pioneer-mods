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
- Compatibility-gated recipe execution with deterministic byte-span patch operations
- CI checks for format/lint/test

Container-aware section codecs/repacking are still in progress.

## Quick start (developer)

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo install cargo-audit --version 0.22.2 --locked
cargo audit
python scripts/check_file_size_caps.py
python -m pip install --upgrade pip -r .github/requirements/python-quality.txt
bash scripts/run_python_quality.sh
```

Inspect owner-supplied firmware identity:

```bash
cargo run -p patch-cli -- inspect --input /path/to/XDJ700.UPD --format json
```

Apply a compatibility-gated recipe manifest:

```bash
cargo run -p patch-cli -- patch \
  --input /path/to/XDJ700.UPD \
  --recipe /path/to/recipe.json \
  --output /path/to/XDJ700-patched.UPD
```

Notes:
- Existing output files are refused by default; pass `--force` to allow overwrite.
- Patch output is written atomically and reports both input/output SHA-256 identities.

## Code quality baseline

- Rust:
  - `rustfmt`
  - `clippy`
  - tests (`cargo test`)
  - advisory checks (`cargo audit`, pinned installer version)
- Python (when present):
  - pinned toolchain versions via `.github/requirements/python-quality.txt`
  - unified checker entrypoint: `bash scripts/run_python_quality.sh`
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
