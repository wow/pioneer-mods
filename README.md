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
  (I/O-free engine in `patch-core` via `apply_recipe`; `patch-cli` handles files only)
- CI checks for format/lint/test
- Read-only `.UPD` container parser (`inspect --structure`)
- Canonical `.UPD` serializer with self-check, byte-exact no-op roundtrip, and memory image
  reconstruction
- XDJ-700 MAIN application section verification, LZSS decoding, and self-checked LZSS section
  encoding (byte-identical to the reference encoder)
- XDJ-700 `.UPD` rebuild around a modified application (`xdj700::rebuild_with_application`),
  verified against its input by `xdj700::verify_rebuild`. It reproduces the reference alpha.2
  update byte-for-byte.
- `rebuild` command: writes a verified no-op rebuild of the official v1.15 update (stock
  application re-encoded) under a declared version label

Recipes that target the decoded application are still in progress.

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

Inspect the `.UPD` container structure (documents, CRC-16 trailers, S-record layout, address
extents, reconstructed image identities). This is read-only and refuses malformed containers.
It also proves that re-serializing the parsed container reproduces the input byte-for-byte
(`roundtrip: byte-identical`). For XDJ-700 updates it verifies and decompresses the MAIN
application section at image offset `0x40000` and reports its decoded size and SHA-256:

```bash
cargo run -p patch-cli -- inspect --input /path/to/XDJ700.UPD --structure
```

Validate the parser and serializer against your own official XDJ-700 v1.15 update (local only;
never commit firmware):

```bash
PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
  cargo test -p patch-core --test official_firmware -- --ignored
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
- `patch` currently writes byte spans to the raw input file. It does not yet rebuild `.UPD`
  CRCs, S-record checksums, or compressed-section checksums, so it cannot produce an installable
  update yet. Do not flash its output.

Rebuild the official XDJ-700 v1.15 update around its own, unchanged application (a no-op
rebuild, the first hardware test candidate):

```bash
cargo run --release -p patch-cli -- rebuild \
  --input /path/to/XDJ700.UPD \
  --application stock \
  --label Ver1.15 \
  --output /path/to/new-dir/XDJ700.UPD
```

Notes:
- Only the official v1.15 file is accepted (checked by SHA-256). A rebuilt file is never
  accepted as input.
- `--label` is required. Which labels the device's updater accepts is not yet confirmed.
- The output must not exist; it is never overwritten. The rebuild is verified against the input
  before it is written. It is then written atomically, after its temporary file has been read
  back through the file system. Write to a local disk; exFAT is refused.
- With `--label Ver1.15` the output is always 17,368,545 bytes with SHA-256
  `f2dd19d47b8253fbea189009166f958b2d9f29a0bb8a5d7d258f98144134d06c`.
- Flashing any rebuilt file is at your own risk. Read
  [docs/xdj700-flashing.md](./docs/xdj700-flashing.md) first: it covers the stages, how to check
  the file on the USB stick itself, and recovery.

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

## Acknowledgements

The `.UPD` container model, the LZSS codec semantics and the published reference identities
used in tests come from the MIT-licensed
[DeckVolve xdj-700-mods](https://github.com/DeckVolve/xdj-700-mods) project. The LZSS encoder in
`patch-core` is a decision-identical port of its encoder. Elsewhere in this repository it is
called "the reference implementation".

## Versioning and release docs

- [VERSIONING.md](./VERSIONING.md)
- [RELEASING.md](./RELEASING.md)
- [CHANGELOG.md](./CHANGELOG.md)

## Project governance docs

- [LICENSE](./LICENSE)
- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md)
- [SECURITY.md](./SECURITY.md)
