# Recipes (schema v2)

A schema-v2 recipe describes changes to the **decoded application** of a pinned official
release. `patch-cli patch` applies it and writes a complete, verified `.UPD` that the unit's
updater installs. Read [xdj700-flashing.md](./xdj700-flashing.md) before you flash anything.

## What a recipe looks like

```json
{
  "schema_version": 2,
  "recipe_id": "xdj700-v1.15-example",
  "description": "What the recipe changes for the user.",
  "target": {
    "release": "xdj700-v1.15",
    "upd_sha256": "73edec98…f99c",
    "application_sha256": "1875381b…5939"
  },
  "label": "Ver1.16",
  "reported_version": "0.10",
  "replacements": [
    {
      "offset": 2304,
      "bytes_hex": "0900",
      "precondition": {
        "offset": 2288,
        "len": 48,
        "sha256": "<SHA-256 of the stock bytes in 2288..2336>"
      },
      "purpose": "What the original code does, and what the replacement does instead."
    }
  ],
  "expected": {
    "application_sha256": "<SHA-256 of the decoded output application>",
    "upd_sha256": "<SHA-256 of the output .UPD>"
  }
}
```

| Field | Meaning |
| --- | --- |
| `target.release` | A release the engine pins. Today only `xdj700-v1.15`. |
| `target.upd_sha256`, `target.application_sha256` | The official file and its decoded stock application. They must equal the engine's pins, so a wrong `release` cannot select another file. |
| `label` | The MAIN label of the output, `VerX.YY`. It must be **higher** than the release's own version, or the updater skips the file. |
| `reported_version` | The version the modified application reports, `X.YY`. It must be **lower** than the release's own version, so that the official update restores stock. |
| `replacements` | Same-length replacements in the decoded application, in ascending order and not overlapping. May be empty. |
| `precondition` | A window of the stock application that contains the span, at least 32 bytes long, identified by its SHA-256. A recipe never contains vendor bytes. A hash of only a few bytes could be inverted by brute force (four bytes take minutes), which would publish them; a window of 32 or more bytes cannot. Windows are checked on the stock application before anything is replaced, so they may overlap other spans. |
| `bytes_hex` | The project's own replacement bytes. Their length is the span length. |
| `purpose` | Required. A reviewer must be able to tell what each span changes. |
| `expected` | Optional, but every committed recipe pins both identities. |

## What the engine checks

Before the input is read (`check_recipe_v2`):
1. The recipe's static checks: fields, hex, order, no overlaps, precondition windows of at least
   32 bytes that contain their spans.
2. The release is known, and the recipe repeats its id and pins exactly.
3. The label is higher, and the reported version lower, than the release's own version.
4. No replacement overlaps a protected range. For v1.15 that is `[0, 0x800)`: the application
   header and its version block. The version changes only through `reported_version`.

Then, on the official file:
1. The input is pinned by length (checked before reading) and by SHA-256.
2. Each precondition window lies inside the application and matches its SHA-256 on the stock
   application.
3. The modified application differs from stock **only** in the declared spans and the version
   string (a byte-by-byte check).
4. The rebuild runs its full verification, including the release rule: a modified application
   must report a lower version.
5. The output matches `expected`, when declared.

The output is written atomically, is never overwritten, and is read back before it is renamed
into place.

## Rules for every recipe

- **Report a version lower than 1.15** (the engine enforces this). The official v1.15 update then
  restores stock. This was observed on an owner's unit for a version-only change (the
  flashing guide's stages 3 and 4).
- **Stay out of the code that runs early during start-up.** An application that crashes before
  its own update mode starts cannot be recovered by software (flashing guide, section 5). The
  engine protects only the header; **the start-up path is not yet mapped**, so this rule is
  enforced by review and staged hardware testing, not by the tool.
- **Same length only.** Growing the application (for example appending code) is not supported
  until the memory after the application is understood.
- **Test on hardware in stages**, as the flashing guide describes, and record the result.

## The committed recipes

`recipes/<release>/` holds the project's recipes. CI checks that each one passes every check
that needs no firmware and pins its output identities. The owner-input tests apply them to the
official file.

| Recipe | What it does | Output |
| --- | --- | --- |
| `xdj700-v1.15/version-marker-0.10.json` | Only the reported version, `0.10`. The unit shows `0.10` on UTILITY. | The hardware-tested stage-3 file, `84cbd263…` |

```bash
cargo run --release -p patch-cli -- patch \
  --input /path/to/XDJ700.UPD \
  --recipe recipes/xdj700-v1.15/version-marker-0.10.json \
  --output /path/to/new-dir/XDJ700.UPD
```

## Schema v1

`schema_version` 1 manifests write raw byte spans to the input file. They cannot produce an
installable update; do not flash their output. Any other `schema_version` is refused.
