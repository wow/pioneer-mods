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
| `bytes_hex` | The project's own replacement bytes. Their length is the span length. `--` in place of a byte keeps the stock byte there without publishing it, for changes a few bytes apart, such as fields of one table: fewer than 32 in a row may be kept. The first and last bytes must be written and differ from stock, and of the written bytes at most half may equal stock, fewer than 32 in a row: keep them with `--` or split the span around them. |
| `purpose` | Required. A reviewer must be able to tell what each span changes. |
| `expected` | The output's identities: `application_sha256` (the decoded application) and `upd_sha256` (the update). Optional for a recipe without image edits; a recipe with [image edits](#image-edits) must pin `application_sha256` (a draft uses a placeholder and copies the value `precondition` prints). Every committed recipe pins both. |

## Precondition windows

A recipe holds hashes of stock bytes, not the bytes themselves, except for written bytes in a
span that equal stock (see the span rule below). A hash over a few unknown bytes can be inverted
by brute force (four bytes take minutes), which would publish them. Over 32 or more unknown bytes
that is impractical, unless the bytes are predictable or other hashes overlap them. So:

- Windows of one recipe may not overlap (overlapping windows share all but a few bytes, and each
  hash would reveal the difference). CI checks that the windows of **all committed recipes** of a
  release are disjoint, too. For nearby spans, extend the first window before its span and the
  second after its own.
- One exception across recipes: a recipe may repeat another committed recipe's replacement
  **exactly** (offset, `bytes_hex`, `before`, `after` and the hash; only `purpose` may differ), so
  that it can build on that change. It publishes nothing the first did not. Copy the replacement
  with its hash: a placeholder makes it differ, and `precondition` then refuses the overlap.
- A window may not reach into a protected range (it holds known strings).
- At least 32 window bytes must lie outside the span (`before + after`). The span's own stock
  bytes do not count: they may follow from the replacement (a flipped bit, a changed condition).
- Among the bytes outside the span, the four most common values may fill at most half (this
  refuses padding, fill, and two-valued or 16-bit data).
- A span changes its first and last bytes, and of the bytes it writes at most half may equal
  stock, fewer than 32 in a row, because `bytes_hex` publishes them. A kept byte (`--`) is not
  published and does not count, and it ends such a run. Mark unchanged bytes inside a span with
  `--`, fewer than 32 in a row, or split a longer unchanged stretch into two spans, where it can
  be window bytes instead. Kept bytes are still covered by the window's hash, but like the span's
  other bytes they do not count towards the 32 window bytes outside the span, and a protected
  range that overlaps them overlaps the span.
- A precondition mismatch does not print the window's actual hash, which might be one of the
  windows these rules refuse.

These checks are heuristics against accidental leaks, not a proof. Choose windows over code, not
over strings, tables or images: text passes the value check but is easy to guess, and so can an
image be (a label rendered in a known font). Windows are compared per release; a future release
that shares code with this one needs the same care. Review is the backstop.

### Known exceptions

Windows over data that review accepted, and why. No later recipe can use these ranges, except by
repeating the replacement exactly.

- **`xdj700-v1.15/beat-loop-16-plays-32.json`:** decoded `0xD6234..0xD66F5`, the span and the
  1,216 bytes after it.
  - The span is a table entry, so any window around it lies over data. The next group of the
    same handler's table starts at `0xD6238`. Windows that stop short of it pass the value check
    only from 2,130 bytes before the span: the nearest kilobyte is 65 to 70 percent zero bytes,
    and text follows. After the span, only 49 of the sizes below 636 bytes pass, all close to the
    limit of half; every size from 636 bytes passes, and the share falls to 40 percent at about
    1,160 bytes. At 1,216 bytes it is 38 percent.
  - So the window covers the next group, and that group is reserved; leaving it free would take
    one of the long windows over zero bytes and text. A later change in the range cannot add a
    recipe of its own: it would replace this one, and review would weigh its window against this
    published hash.
  - The first commit of #19 published a 96-byte window for the same span. It lies inside this
    one. On its own it holds 13 distinct 32-bit values that are neither small numbers, pointers
    nor round floats, far beyond brute force. The 1,120 bytes that only this window covers pass
    the value check on their own (38 percent), so the pair does not leave the short stretch
    covered by one hash alone that the rule against overlapping windows guards against.
- **`xdj700-v1.15/beat-loop-1-to-32.json`:** decoded `0xD5668..0xD6234`: the 3,000 bytes before
  the BEAT LOOP button table, the span over its first five entries (`--` keeps the bytes between
  their first bytes), and the rest of the fifth entry.
  - The span is table data, and stage 5's window starts where this one ends, so the window lies
    before the table. The nearest kilobyte there is 64 percent zero bytes: windows pass the value
    check only from 2,091 bytes before the table, and every size from there passes. At 3,000
    bytes the four most common values fill 39 percent, and the window holds 129 distinct 32-bit
    values that are neither zero, small numbers, text, pointers nor round floats.
  - Of the 3,003 bytes outside the span, 32 percent are zero and 27 percent printable text (36
    runs of 4 or more characters, the longest 45). Text is easy to guess, but guessing it and
    the zeros still leaves the other 41 percent (1,244 bytes), which hold those 129 values.
  - The table's sixth entry is stage 5's replacement, repeated exactly, so it publishes nothing
    new and keeps stage 5's window.

## Writing a recipe

Write recipes against your own copy of the official file, and commit only the recipe. Never
paste stock bytes into an issue or a commit, only their hash.

1. Choose each span and its window in your own analysis of the decoded application. The CLI
   does not extract it; `patch-core` decodes it (`xdj700::decode_application` on the parsed
   official update), and it stays on your machine. Write the recipe with every field filled in,
   and use any 64 hex digits (for example all zeros) as the placeholder for each
   `precondition.sha256`. Leave `expected` out for now, unless the recipe has
   [image edits](#image-edits): then use a placeholder for `expected.application_sha256` too.
2. Compute the hashes:

   ```bash
   cargo run --release -p patch-cli -- precondition \
     --input /path/to/XDJ700.UPD \
     --recipe /path/to/draft.json \
     --committed-recipes recipes \
     --protected-set /path/to/xdj700-v1.15-protected-set.tsv
   ```

   Without the set, pass `--no-protected-set` instead; the maintainer then checks the recipe
   against it ([The protected set](#the-protected-set)).

   Before it computes any hash, the command runs the recipe's own checks (release pins, version
   order, bounds, protected ranges), checks that its windows are disjoint from those of the other
   committed recipes or repeat one of their replacements exactly (the draft's own file, and any
   recipe with its `recipe_id`, are skipped, and the directory must hold another recipe for the
   release), and then rebuilds the draft, running each window's leak checks before it
   hashes the window. So it never prints the hash of a window those rules refuse, prints nothing
   unless the whole rebuild succeeds, and writes nothing. For each replacement it prints the
   window and its SHA-256, for each [image edit](#image-edits) the image (with no hash), and the
   output identities (`expected.application_sha256` and `expected.upd_sha256`), and says whether
   the recipe already declares each. Copy them into the recipe.
   (The first recipe for a newly added release therefore needs a committed recipe for that
   release first, such as its version marker.)
3. Run `precondition --check`: it exits with an error unless every hash and identity the recipe
   declares is `as declared`, and says which kind differs (precondition hashes from the official
   update, or output identities from the rebuilt output). Then run `patch` to build the update.
   It checks every hash and the rebuild's own rules (bounded diff, version rule, image size), and
   prints the output identities, which match those `precondition` printed.

Settle your windows before you push: hashes of windows that later move stay in the history, and
two overlapping windows reveal the bytes between them. CI checks the committed recipes again.

A hash covers whatever is at the declared offset, so it cannot show that the offset is the one
you meant. Check offsets against your own analysis before step 2. After that, the hash catches a
later change to an offset, unless the same bytes also occur at the new offset (code and tables
can repeat), so prefer windows long enough to be unique.

## Image edits

Some changes are pictures, not code: a button's label, for example, is drawn into an RGB565 image
stored in the application. Copying finished pixels into a recipe would publish vendor pixels, so
an image edit carries only coordinates and the author's own glyph, and the engine computes every
pixel from the owner's file at patch time. It carries no hash of the image either: anyone could
check a guess at the stock pixels against one, and a rendered label can be easy to guess. A recipe
with image edits pins its output instead:

```json
"image_edits": [
  {
    "offset": 3471680,
    "width": 80,
    "height": 53,
    "erase": {"x": 30, "y": 19, "width": 19, "height": 14},
    "glyph": {
      "at": {"x": 31, "y": 19, "width": 17, "height": 14},
      "alpha_hex": "00f8…",
      "colour_from": {"x": 33, "y": 25}
    },
    "purpose": "Why the image changes."
  }
],
"expected": {
  "application_sha256": "<SHA-256 of the output application, from precondition>",
  "upd_sha256": "<SHA-256 of the output update, from precondition>"
}
```

| Field | Meaning |
| --- | --- |
| `offset`, `width`, `height` | The image: `width * height` 16-bit little-endian RGB565 pixels, rows `width` pixels apart, at a decoded-application offset. At most 1 MiB. |
| `erase` | Optional. Each row of the box is refilled by interpolating between the stock pixels just left and right of it, which removes a label from a smooth background. The box needs a column of the image on each side. |
| `glyph.at`, `glyph.alpha_hex` | The author's own coverage mask, one hex digit per pixel, row by row: `0` leaves the pixel, `f` paints it fully, and the digits between blend linearly (per channel, in fifteenths, rounded). |
| `glyph.colour_from` | The stock pixel whose colour the glyph is drawn in, read before anything changes: for a relabelled button, a pixel inside a stroke of the old label. |
| `purpose` | Why the image changes. |

The rules:
- A recipe with image edits must pin `expected.application_sha256` (`expected.upd_sha256` is
  recommended too). The input and the stock application are pinned by SHA-256, so the image at
  `offset` is fixed; the output pin then catches an offset changed after the recipe was completed
  and any change in the pixel arithmetic. It covers the whole application, so it reveals nothing
  about one image. When it does not match, the refusal cannot say which image differs.
- Images may not overlap each other or any replacement's window (whose hash would cover the
  image's pixels), and an image may not overlap another committed recipe's window either. Images
  of different recipes may overlap: each recipe is applied on its own, and none publishes a hash
  of the image.
- The glyph mask must be the author's own drawing, never traced from vendor pixels.
- Images are subject to the protected ranges and the [protected set](#the-protected-set), like
  replacements.
- The output may differ from stock only in the edited rows: per row, the span covering the
  erase box and the glyph box.
- Whether the image is read during start-up or an update is shown in emulation, as for any
  replaced bytes (flashing guide, section 5). Images are normally read only when they are drawn.

## What the engine checks

Before the input is read (`check_recipe_v2`):
1. The recipe's static checks: fields, hex, kept bytes (`--`) neither at a span's edge nor 32 or
   more in a row, order, no overlapping spans, precondition windows of at most 4096 bytes, with
   at least 32 outside the span, that start inside the application and do not overlap each
   other, and an `expected.application_sha256` for a recipe with image edits.
2. The release is known, and the recipe repeats its id and pins exactly.
3. The label is higher, and the reported version lower, than the release's own version.
4. Every precondition window ends inside the application (its length is pinned).
5. No replacement, precondition window or edited image overlaps a protected range. For v1.15
   that is `[0, 0x800)`: the application header and its version block. The version changes only
   through `reported_version`.
6. Every [image edit](#image-edits) is well formed: its boxes and colour pixel lie inside the
   image, its mask has one digit per pixel and paints something, and its image lies inside the
   application, clear of the other images and of the replacements' windows.

Then, unless it is skipped with `--no-protected-set`, `patch` and `precondition` check the recipe
against the [protected set](#the-protected-set) (`check_recipe_against_protected_set`, called by
the CLI after `check_recipe_v2` and again by the engine entry points), still before the input is
read.

Then, on the official file:
1. The input is pinned by length (checked before reading) and by SHA-256.
2. On the stock application, before anything is replaced, each precondition window matches its
   SHA-256 and passes the [window rules](#precondition-windows), and each span changes its first
   and last bytes and, of the bytes it writes, at most half equal stock, fewer than 32 in a row.
   Kept bytes (`--`) stay stock: only written bytes are declared, so the next check covers them.
3. The modified application differs from stock **only** in the written bytes of the spans, the
   edited image rows and the version string (a byte-by-byte check that the rebuild entry point
   runs for every edit).
4. The rebuild runs its full verification, including the release rule: a modified application
   must report a lower version.
5. The output matches `expected`, when declared (always, for a recipe with image edits).

The output is written atomically, is never overwritten, and is read back before it is renamed
into place.

## Rules for every recipe

- **Report a version lower than 1.15** (the engine enforces this). The official v1.15 update then
  restores stock. This was observed on an owner's unit for a version-only change (the
  flashing guide's stages 3 and 4).
- **Stay out of start-up and the update path.** An application that crashes before its own
  update mode starts, or whose updater no longer works, cannot be recovered by software. Follow
  the rules for every modification in the flashing guide, section 5, which is their
  authoritative statement: windows outside the protected set measured in emulation, replaced
  bytes not read during start-up or an update, and an emulator rehearsal before a file is
  offered. Its addresses are run-time addresses, `0x08000000` plus a recipe's `offset`. The
  engine always enforces the header (`[0, 0x800)`), and the protected set when it is given
  ([below](#the-protected-set)); the data rule and the rehearsal remain the maintainer's.
- **Same length only.** Growing the application (for example appending code) is not supported
  until the memory after the application is understood.
- **Test on hardware in stages**, as the flashing guide describes, and record the result.

## The protected set

The code that runs at start-up and in the update path, measured in emulation (flashing guide,
section 5), is a list of address ranges kept outside this repository. With it, `patch` and
`precondition` refuse a schema-v2 recipe, before the firmware is read, if:
- the set was measured on another release;
- the set covers the version string, which every rebuild writes (the set must then be checked);
- a replacement's span or precondition window, or an edited image, overlaps it.

Skipping the check is a decision, never an omission. Each command takes the set from
`--protected-set <file>`, else from the `XDJ700_PROTECTED_SET` environment variable, and
refuses to run without one unless `--no-protected-set` is given. That flag prints a warning, and
the report line says `protected_set: skipped`. Owners applying a committed recipe pass it: the
maintainer has checked the committed recipes against the set, which is not published.
Contributors without the set pass it too, and the maintainer checks the recipe before it is
committed.

The library entry points (`apply_recipe_v2_to`, `precondition_hashes`) take the set through
their `RecipeChecks` argument and run the check again themselves. The set covers code only:
whether a recipe's replaced bytes are read during start-up or an update is still shown in
emulation, not by the tool.

```bash
cargo run --release -p patch-cli -- precondition \
  --input /path/to/XDJ700.UPD \
  --recipe /path/to/draft.json \
  --committed-recipes recipes \
  --protected-set /path/to/xdj700-v1.15-protected-set.tsv
```

The file is UTF-8 text:

```text
# comments and blank lines are ignored
release xdj700-v1.15
start   end       bytes
08000600 08000605 6
```

- The first line that is not blank or a comment names the release the set was measured on:
  `release <id>`, the id recipes use in `target.release`.
- The next line may be a header, exactly `start end` or `start end bytes`.
- Every other line is a range. `start` and `end` are hexadecimal **run-time** addresses
  (`0x08000000` plus a decoded offset; the `0x` prefix is optional), and `end` is the last
  address of the range, **inclusive**. The optional `bytes` column, in decimal digits, must
  equal `end - start + 1`, which catches exclusive ends.
- Every range must lie inside the application; ranges may overlap and need not be sorted. A
  leading byte-order mark is ignored. A file without ranges, or larger than 8 MiB, is refused.

The maintainer checks the committed recipes against it with an ignored test. Without the
variable it is skipped and says so; with it, it fails unless every known v1.15 recipe was
checked:

```bash
XDJ700_PROTECTED_SET=/path/to/xdj700-v1.15-protected-set.tsv \
  cargo test -p patch-core --test committed_recipes -- --ignored --nocapture
```

## The committed recipes

`recipes/<release>/` holds the project's recipes. CI checks that each one passes every check
that needs no firmware and pins its output identities, that their `recipe_id`s are distinct, and
that their precondition windows are disjoint, except where one recipe repeats another's
replacement exactly. The owner-input tests apply every one of them to the official file (which
runs the window rules) and check those identities.

| Recipe | What it does | Output |
| --- | --- | --- |
| `xdj700-v1.15/version-marker-0.10.json` | Only the reported version, `0.10`. The unit shows `0.10` on UTILITY. | The hardware-tested stage-3 file, `84cbd263…` |
| `xdj700-v1.15/beat-loop-16-plays-32.json` | **Experimental; passed on an owner's unit (2026-10-09).** The BEAT LOOP button labelled 16 sets a 32-beat loop (its label still reads 16); reports `0.11`. One table entry changes: the length list the player uses already holds 32 beats, and the button table selects it instead of 16. While that loop plays no pad lights, in emulation ([the lit BEAT LOOP pad](./xdj700-flashing.md#the-lit-beat-loop-pad-stages-5-and-7)). | Stage 5, `144f4b55…` |
| `xdj700-v1.15/beat-loop-1-to-32.json` | **Experimental; rehearsed in emulation, not yet tested on hardware.** The PERFORM screen's six BEAT LOOP buttons read and set 1, 2, 4, 8, 16 and 32 beats instead of 1/2, 1, 2, 4, 8 and 16; reports `0.12`. Each button selects the length the button after it selected, and the last one 32 beats (the table's last entry as in stage 5, repeated exactly), and each button's six images are relabelled with the project's own digits (36 image edits). Known limit: the pad to the right of the touched pad lights, none for 32, in emulation ([the lit BEAT LOOP pad](./xdj700-flashing.md#the-lit-beat-loop-pad-stages-5-and-7)). | Stage 7, `259c75da…` |

```bash
cargo run --release -p patch-cli -- patch \
  --input /path/to/XDJ700.UPD \
  --recipe recipes/xdj700-v1.15/version-marker-0.10.json \
  --output /path/to/new-dir/XDJ700.UPD \
  --no-protected-set
```

`--no-protected-set`: the maintainer has checked the committed recipes against the
[protected set](#the-protected-set), which is not published.

## Schema v1

`schema_version` 1 manifests write raw byte spans to the input file. They cannot produce an
installable update; do not flash their output. Any other `schema_version` is refused.
