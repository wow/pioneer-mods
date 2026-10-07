# Security Policy

This is an **unofficial, experimental** project. See [README.md](./README.md) for the full disclaimer.

## Supported versions

The project is pre-`1.0.0`. Only the latest release (and `main`) receives fixes. See [VERSIONING.md](./VERSIONING.md).

## Reporting a vulnerability

**Do not open a public issue for security problems.**

Report privately via GitHub: go to the repository's **Security** tab → **Report a vulnerability**
([direct link](https://github.com/wow/pioneer-mods/security/advisories/new)).

Please include:
- affected version/commit,
- device model and firmware version involved (if relevant),
- steps to reproduce and observed impact.

**Never attach vendor firmware files** to a report. Refer to firmware by model, version, and hash instead.

## What counts as a security issue here

In addition to typical software vulnerabilities, we treat these as security/safety issues:
- a patch or tool path that can produce output for an **incompatible** firmware instead of refusing,
- non-deterministic output that could ship an unintended binary,
- mutation of bytes **outside declared patch regions**,
- anything that could plausibly brick a device or bypass documented safety checks.

## Response

This is a volunteer project; responses are best-effort. We aim to acknowledge reports within 7 days and will coordinate disclosure through a GitHub Security Advisory.
