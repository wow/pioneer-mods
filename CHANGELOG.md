# Changelog

All notable changes to this project should be documented in this file.

The format is inspired by Keep a Changelog and follows [VERSIONING.md](./VERSIONING.md).

## [Unreleased]

### Added
- Initial project documentation for versioning, releasing, and safety disclaimers.
- Open-source baseline docs: `LICENSE` and `CONTRIBUTING.md`.
- Rust workspace scaffolding with:
  - `core/patch-core` (firmware identity hashing/size inspection),
  - `core/patch-schema` (recipe manifest, target matching, and validation model),
  - `core/patch-cli` (`inspect` plus compatibility-gated `patch` command baseline).
- CI workflow for rustfmt, clippy, and tests.
- Quality-hardening baseline:
  - `.editorconfig`,
  - `pyproject.toml` for Ruff/mypy/pytest standards,
  - pinned Python tool versions in `.github/requirements/python-quality.txt`,
  - shared Python quality runner (`scripts/run_python_quality.sh`) used by CI and local docs,
  - repository file-size cap script (`scripts/check_file_size_caps.py`),
  - expanded CI jobs for Rust advisory audit, Python quality, and repository hygiene,
  - concurrency adjusted to avoid canceling `main`/scheduled runs,
  - pinned `cargo-audit` install version and cached Rust security job dependencies.
- Community health files: `CODE_OF_CONDUCT.md`, `SECURITY.md`, issue templates, and pull request template.
- Read-only `.UPD` container parser in `patch-core` (`patch_core::upd::parse_upd`): decimal
  document-length header, 32-byte document descriptors, CRC-16/XMODEM document trailers, and
  Motorola S-record syntax, checksum and layout validation (fail-closed). Parsed types have
  private fields and can only be built by the parser.
- `patch-cli inspect --structure` reports container documents, versions, CRCs, record counts and
  address extents in text or JSON. Without `--structure`, `inspect` output is unchanged.
- Canonical `.UPD` serializer (`UpdContainer::to_bytes`). It re-parses its own output and refuses
  unless the result equals the source container (`verify_serialized`). If the output does not
  parse, the error keeps the underlying parse error. `verify_roundtrip` is the
  Phase 1 no-op gate: parse, serialize, and require a byte-identical result
  (`UpdContainer::verify_reproduces`). The output is encoded into a single buffer and only
  re-parsed to diagnose a mismatch.
- Memory image reconstruction per document (`UpdDocument::image`): spans from the first to the last
  data byte, gaps filled with `0xFF`, capped at 64 MiB.
- `inspect --structure` now verifies the byte-exact roundtrip (`roundtrip_verified: true`) and
  reports each document's `image_span` and `image` with a `status`:
  - `reconstructed`, with an 8-digit base, length and SHA-256;
  - `span_exceeds_cap` (per-document 64 MiB cap);
  - `budget_exhausted` (128 MiB total across documents, which bounds the work a small crafted
    file can cause).
- Roundtrip and summary failures after a successful parse are reported as internal errors to
  report, not as an invalid input file.
- Ignored-by-default owner-input test (`official_firmware`) that checks an official XDJ-700 v1.15
  update against public pinned identities when `PIONEER_XDJ700_V115_UPD` is set.
- LZSS decoder for the XDJ-700 MAIN section format (`patch_core::lzss::decode`): 4096-byte ring
  pre-filled with spaces, absolute ring positions, 3..=18-byte matches, and a bounded output.
- XDJ-700 application section support (`patch_core::xdj700`). It verifies the size field, the
  16-bit additive checksum and the zero-prefix tag at MAIN image offset `0x40000`, then decodes
  the section with a 64 MiB output cap.
- `inspect --structure` reports the decoded application (offset, compressed length, checksum,
  decoded length and SHA-256) for XDJ-700 updates. Decoding is limited to MAIN versions whose
  layout has been verified (currently `Ver1.15`); other versions are reported as `unsupported`.
  A container with more than one XDJ-700 MAIN document is refused as ambiguous. The MAIN image
  is built once, during the budgeted summary (`UpdContainer::summary_with_images`). An invalid
  section is reported with its reason instead of failing the whole report.
- Deterministic LZSS encoder (`patch_core::lzss::encode`, `encode_section_stream`). It is
  decision-identical to DeckVolve's reference encoder: verified byte-identical on 6,800 random
  inputs and on the official v1.15 application, and pinned in CI by golden vectors.
- `patch_core::xdj700::encode_section` builds complete section bytes (size field, stream,
  checksum). It is self-checked: the result must decode back to the input through
  `decode_section`.
- `patch_core::read_firmware` reads an input once and returns its identity plus the hashed
  bytes; `read_regular_file` reads without hashing. Both refuse non-regular files (directories,
  FIFOs, devices): the path is checked first, the file is opened non-blocking on Unix so a path
  swapped to a FIFO cannot hang `open()`, and the open handle is checked again before reading.

### Changed
- Local Python validation instructions now use the same entrypoint as CI (`scripts/run_python_quality.sh`).
- `patch-cli patch` now loads and validates recipe manifests, enforces firmware identity compatibility
  (size + SHA-256), and applies deterministic `write_span` / `owner_copy_window` operations.
- Rust tests now live in dedicated `tests/` files per crate instead of inline `#[cfg(test)]` blocks
  in production source files.
- `patch-cli patch` now hashes in-memory input bytes (single-read flow), rejects overlapping destination
  ranges, verifies that all byte mutations stay within declared destination regions, and reports
  output SHA-256.
- `patch-cli patch` reads input through `read_regular_file`. A FIFO or device input is now
  refused instead of blocking or being read without limit. The input is hashed once, by the
  engine.
- The patch engine moved from `patch-cli` into `patch-core` as an I/O-free `apply_recipe` API
  (manifest validation, size + SHA-256 gating, bounds/overlap checks, bounded-diff verification,
  expected output hash) with typed `PatchEngineError` refusals. `patch-cli patch` now only reads
  input, writes output, and prints results. Engine refusals (identity, bounds, overlap, output hash)
  now print a "refusing to patch input firmware '<input>' with recipe '<recipe>'" line followed by
  the reason; the incompatible-identity reason no longer repeats the input file name.

### Fixed
- `scripts/check_file_size_caps.py` now reads tracked files with `git ls-files -z` from repository root,
  preventing silent skips for non-ASCII paths and subdirectory execution.
- Output safety hardening for `patch-cli patch`:
  - refuses output paths that resolve to the input firmware path,
  - refuses overwriting existing output files unless `--force` is provided,
  - writes patched output via atomic temp-file persist in the destination directory.
- Recipe schema hardening:
  - rejects unsupported schema versions (currently only `schema_version=1`),
  - denies unknown JSON fields on manifest/target/operation structs,
  - validates optional `expected_output_sha256` and uses portable length checks.

### Removed
- Redundant `rustfmt.toml` and `clippy.toml` files (settings matched tool defaults or duplicated
  existing workspace metadata).

### Safety
- Project is unofficial and experimental; use at your own risk.
