# Changelog

All notable changes to this project should be documented in this file.

The format is inspired by Keep a Changelog and follows [VERSIONING.md](./VERSIONING.md).

## [Unreleased]

### Added
- Initial project documentation for versioning, releasing, and safety disclaimers.
- Open-source baseline docs: `LICENSE` and `CONTRIBUTING.md`.
- Rust workspace scaffolding with:
  - `core/patch-core` (firmware identity hashing/size inspection),
  - `core/patch-schema` (recipe manifest and validation model),
  - `core/patch-cli` (initial `inspect` command and explicit `patch` placeholder error).
- CI workflow for rustfmt, clippy, and tests.
- Quality-hardening baseline:
  - `rustfmt.toml`, `clippy.toml`, `.editorconfig`,
  - `pyproject.toml` for Ruff/mypy/pytest standards,
  - repository file-size cap script (`scripts/check_file_size_caps.py`),
  - expanded CI jobs for Rust advisory audit, Python quality, and repository hygiene.
- Community health files: `CODE_OF_CONDUCT.md`, `SECURITY.md`, issue templates, and pull request template.

### Changed

### Fixed

### Removed

### Safety
- Project is unofficial and experimental; use at your own risk.
