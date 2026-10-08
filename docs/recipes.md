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
        "before": 16,
        "after": 30,
        "sha256": "<SHA-256 of the stock bytes in 2288..2336: 16 before, the span, 30 after>"
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
| `precondition` | The stock bytes around the span, `before` it and `after` it (`before + after` at least 32, the window at most 4096 bytes), identified by their SHA-256. The window moves with `offset`, so a mistyped offset fails the check. See [Precondition windows](#precondition-windows). |
| `bytes_hex` | The project's own replacement bytes. Their length is the span length. The first and last must differ from stock, and at most half may equal it, fewer than 32 in a row: split the span around unchanged bytes. |
| `purpose` | Required. A reviewer must be able to tell what each span changes. |
| `expected` | Optional, but every committed recipe pins both identities. |

## Precondition windows

A recipe holds hashes of stock bytes, not the bytes themselves, except where a span keeps stock
bytes between nearby edits. A hash over a few unknown bytes can be inverted by brute force (four
bytes take minutes), which would publish them. Over 32 or more unknown bytes that is impractical,
unless the bytes are predictable or other hashes overlap them. So:

- Windows of one recipe may not overlap (overlapping windows share all but a few bytes, and each
  hash would reveal the difference). CI checks that the windows of **all committed recipes** of a
  release are disjoint, too. For nearby spans, extend the first window before its span and the
  second after its own.
- A window may not reach into a protected range (it holds known strings).
- At least 32 window bytes must lie outside the span (`before + after`). The span's own stock
  bytes do not count: they may follow from the replacement (a flipped bit, a changed condition).
- Among the bytes outside the span, the four most common values may fill at most half (this
  refuses padding, fill, and two-valued or 16-bit data).
- A span changes its first and last bytes and keeps at most half of its stock bytes, fewer than
  32 in a row, because `bytes_hex` publishes them. Split a longer unchanged stretch into two
  spans, where it can be window bytes instead.
- A precondition mismatch does not print the window's actual hash, which might be one of the
  windows these rules refuse.

These checks are heuristics against accidental leaks, not a proof. Choose windows over code, not
over strings or tables: text passes the value check but is easy to guess. Windows are compared
per release; a future release that shares code with this one needs the same care. Review is the
backstop.

## Writing a recipe

Write recipes against your own copy of the official file, and commit only the recipe. The CLI
does not yet print a window's SHA-256; a helper that does, after the same leak checks, is
planned. Until then, compute it with `patch-core`: decode the stock application with
`xdj700::decode_application`, take `sha256_hex` of the window, and check the recipe with
`xdj700::apply_recipe_v2` before you commit it. Never paste stock bytes into an issue or a
commit, only their hash.

## What the engine checks

Before the input is read (`check_recipe_v2`):
1. The recipe's static checks: fields, hex, order, no overlapping spans, precondition windows of
   at most 4096 bytes, with at least 32 outside the span, that start inside the application and
   do not overlap each other.
2. The release is known, and the recipe repeats its id and pins exactly.
3. The label is higher, and the reported version lower, than the release's own version.
4. Every precondition window ends inside the application (its length is pinned).
5. No replacement or precondition window overlaps a protected range. For v1.15 that is
   `[0, 0x800)`: the application header and its version block. The version changes only through
   `reported_version`.

Then, on the official file:
1. The input is pinned by length (checked before reading) and by SHA-256.
2. On the stock application, before anything is replaced, each precondition window matches its
   SHA-256 and passes the [window rules](#precondition-windows), and each span changes its first
   and last bytes and keeps at most half of its stock bytes, fewer than 32 in a row.
3. The modified application differs from stock **only** in the declared spans and the version
   string (a byte-by-byte check that the rebuild entry point runs for every edit).
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
that needs no firmware and pins its output identities, and that their precondition windows are
disjoint. The owner-input tests apply every one of them to the official file (which runs the
window rules) and check those identities.

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
