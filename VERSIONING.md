# Versioning Policy

This project uses **Semantic Versioning** (`MAJOR.MINOR.PATCH`) with explicit prerelease channels.

## Version format

- Stable: `X.Y.Z`
- Prerelease: `X.Y.Z-alpha.N`, `X.Y.Z-beta.N`, `X.Y.Z-rc.N`

## Current maturity policy

Until `1.0.0`:
- The project is experimental.
- Breaking changes may occur in minor releases (`0.Y.Z`).
- Release notes must explicitly call out any breaking behavior.

After `1.0.0`:
- `MAJOR`: breaking changes
- `MINOR`: backward-compatible new functionality
- `PATCH`: backward-compatible bug fixes

## Compatibility signaling

Every release must document:
- supported firmware identity set (hash/version/size contracts),
- newly added feature flags/recipes,
- removed/deprecated support paths.

## Tagging

Git tag format:
- `vX.Y.Z` for stable
- `vX.Y.Z-alpha.N`, `vX.Y.Z-beta.N`, `vX.Y.Z-rc.N` for prereleases

## Changelog requirement

Each release must update [CHANGELOG.md](./CHANGELOG.md) with:
- Added
- Changed
- Fixed
- Removed
- Security/Safety notes (when relevant)
