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
- Recipe schema v2: same-length changes to the decoded application of a pinned release, with
  hash preconditions over at least 32 stock bytes around each span (recipes hold hashes, and
  stock bytes only in short unchanged gaps inside a span), a protected header and version block,
  and a bounded diff, written as a complete, verified update by `patch`.
  "Verified" means checked against the input and the recipe, not safe to flash. The start-up
  and update-path code is mapped only in emulation, as a protected set kept outside this
  repository: given it, `patch` and `precondition` refuse a recipe that overlaps it, and without
  it they run only when `--no-protected-set` skips that check on purpose; emulator rehearsals
  and staged hardware tests do the rest. See
  [docs/recipes.md](./docs/recipes.md) and the flashing guide.
- `precondition` command: prints a draft recipe's precondition hashes, computed on the official
  file only after the recipe, committed-recipe and leak checks, so authors never compute a hash
  themselves and never see one for a window those rules refuse

## Quick start (developer)

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items
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

Apply a schema-v2 recipe (see [docs/recipes.md](./docs/recipes.md)), for example the committed
version marker, which reproduces the hardware-tested stage-3 file:

```bash
cargo run --release -p patch-cli -- patch \
  --input /path/to/XDJ700.UPD \
  --recipe recipes/xdj700-v1.15/version-marker-0.10.json \
  --output /path/to/new-dir/XDJ700.UPD \
  --no-protected-set
```

`--no-protected-set` skips the start-up and update-path check deliberately: the maintainer has
checked the committed recipes against the protected set, which is not published (see "The
protected set" in [docs/recipes.md](./docs/recipes.md)).

To write a recipe, `precondition` prints its precondition hashes, computed on the official file
after the recipe, committed-recipe and leak checks, so a draft's placeholders can be filled in;
`--check` confirms a completed recipe (see "Writing a recipe" in
[docs/recipes.md](./docs/recipes.md)):

```bash
cargo run --release -p patch-cli -- precondition \
  --input /path/to/XDJ700.UPD \
  --recipe /path/to/draft.json \
  --committed-recipes recipes \
  --protected-set /path/to/xdj700-v1.15-protected-set.tsv   # or --no-protected-set
```

Notes:
- A v2 output is a complete update, verified before it is written, written atomically and never
  overwritten (`--force` is refused).
- Schema v1 manifests still work, but they write raw byte spans to the input file and cannot
  produce an installable update. Do not flash their output. For v1 only, `--force` allows
  overwriting an existing output.

Rebuild the official XDJ-700 v1.15 update around its own application: unchanged (a no-op
rebuild), or with only its reported version changed (`--report-version`):

```bash
cargo run --release -p patch-cli -- rebuild \
  --input /path/to/XDJ700.UPD \
  --application stock \
  --label Ver1.16 \
  --output /path/to/new-dir/XDJ700.UPD
```

Notes:
- Only the official v1.15 file is accepted (checked by SHA-256). A rebuilt file is never
  accepted as input.
- `--label` is required, with no default (see the note on versions below). A label not higher
  than `Ver1.15` gets a warning.
- `--report-version X.YY` (optional) sets the version the application reports about itself;
  only its version string changes. It must be lower than 1.15, so that the official v1.15
  update restores the stock application (observed on an owner's unit with `0.10`; see the
  guide's stages 3 and 4). `inspect --structure` shows a file's `reported_version`.
- The output must not exist; it is never overwritten. The rebuild is verified against the input
  before it is written. It is then written atomically, after its temporary file has been read
  back through the file system. Write to a local disk, then copy the file to a FAT32 stick
  (on macOS the writer refuses exFAT).
- The updater only writes a document whose version is higher than the installed one; equal and
  lower versions are skipped, as observed on an owner's unit. A unit on v1.15 therefore needs
  `--label Ver1.16`, the smallest higher label. Without `--report-version` its output is always
  17,368,545 bytes with SHA-256
  `9e1ac10e09c701cb6863b8667131e03452156a0bd7702823bc5f0502a88b6a08`; with `--report-version 0.10`
  it is the guide's stage-3 file (17,368,543 bytes, `84cbd263…`). It was flashed and booted on an
  owner's unit. Labels do not stick: the unit reports the application's own version string
  (observed), so future official releases are expected to install normally; see the guide.
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
