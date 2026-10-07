# Releasing Guide

Public release checklist for this project.

## 1) Choose release type

- prerelease (`alpha`, `beta`, `rc`) for experimental milestones
- stable (`vX.Y.Z`) only when release gates are met

## 2) Preconditions

Before cutting a release:

1. Determinism checks pass.
2. Refusal-path checks pass (incompatible inputs are rejected explicitly).
3. Compatibility contracts are up to date.
4. Risk statement is reviewed for public release.
5. [README.md](./README.md) disclaimer remains present and accurate.

## 3) Update release docs

1. Update [CHANGELOG.md](./CHANGELOG.md).
2. Confirm [VERSIONING.md](./VERSIONING.md) policy is still accurate.
3. Prepare release notes including:
   - scope,
   - known limitations,
   - compatibility table,
   - migration notes (if any),
   - safety warning for experimental channels.

## 4) Tag and publish

1. Create release tag in version format from [VERSIONING.md](./VERSIONING.md).
2. Publish release artifacts and checksums.
3. Mark prerelease status correctly for `alpha/beta/rc`.

## 5) Post-release

1. Record any discovered regressions quickly in changelog/unreleased section.
2. If a severe issue is found, publish a patch release and annotate advisory notes.

---

For experimental firmware patch tooling, conservative release policy is preferred over fast release cadence.
