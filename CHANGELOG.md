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
  Motorola S-record syntax, checksum and layout validation (fail-closed).
- `patch-cli inspect --structure` reports container documents, versions, CRCs, record counts and
  address extents in text or JSON. Without `--structure`, `inspect` output is unchanged.
- `patch_core::read_firmware` reads an input once and returns its identity plus the hashed bytes.
  It refuses non-regular files (directories, FIFOs, devices) before opening, and re-checks the
  open handle.

### Changed
- Local Python validation instructions now use the same entrypoint as CI (`scripts/run_python_quality.sh`).
- `patch-cli patch` now loads and validates recipe manifests, enforces firmware identity compatibility
  (size + SHA-256), and applies deterministic `write_span` / `owner_copy_window` operations.
- Rust tests now live in dedicated `tests/` files per crate instead of inline `#[cfg(test)]` blocks
  in production source files.
- `patch-cli patch` now hashes in-memory input bytes (single-read flow), rejects overlapping destination
  ranges, verifies that all byte mutations stay within declared destination regions, and reports
  output SHA-256.
- `patch-cli patch` reads input through `read_firmware`. A FIFO or device input is now refused
  instead of blocking or being read without limit.
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
