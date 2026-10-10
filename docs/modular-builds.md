# Modular builds: players, per-screen skins and features (design, draft)

Status: proposal (2026-10-10). Nothing here is implemented yet; the schemas below are sketches
to be settled in the pull requests that implement them. The rules in
[xdj700-flashing.md](./xdj700-flashing.md) section 5 and [recipes.md](./recipes.md) stay in force
until this document changes them, one limit at a time, with the evidence described in
[Extending the limits](#extending-the-limits).

## Goal

An owner builds their own firmware in three choices:

1. **Player:** the hardware and its official firmware (today the XDJ-700 v1.15; next the
   XDJ-1000MK2 v1.45).
2. **A skin per screen:** the main screen, the PERFORM screen, later BROWSE, UTILITY and others,
   each chosen on its own (stock, or a skin made for that screen).
3. **Features:** behaviour changes such as BEAT LOOP lengths. A feature the chosen player or
   skins cannot support is shown switched off, with the reason.

Later, an owner can make a **custom skin**: start from a screen's skeleton (its default layout)
and choose which features appear where. The community adds features and skins as data that the
same checks verify.

## Principles

- **Owner-supplied firmware only.** The owner provides the official update; nothing of the
  vendor's is distributed: no firmware, no extracted images, no stock bytes beyond what the leak
  rules allow. Skins are the project's or the community's own art, or transforms of the chosen
  player's own images done on the owner's computer (as image edits are today).
- **Pinned and fail-closed.** Every input is pinned by SHA-256; every change is checked against
  stock bytes it may not reveal, against protected code and data, and against the bounded diff;
  output is written only after the whole build verifies.
- **Recovery first.** Nothing may change what a normal boot runs before the update-mode
  decision, or the update path, unless the limit is extended with evidence. The official update
  must always restore stock (the reported version stays lower).
- **Evidence over assumption.** A feature or skin is offered only with the evidence its maturity
  tier requires (emulator rehearsals, acceptance tests, hardware stages).

## Concepts

### Player (target)

One file per model and firmware release:

```json
{
  "id": "xdj700-v1.15",
  "model": "XDJ-700",
  "firmware": {
    "file": "XDJ700.UPD", "sha256": "73edec98…", "application_sha256": "1875381b…"
  },
  "screen_class": { "width": 800, "height": 480, "pixels": "rgb565" },
  "budgets": { "main_growth_bytes": 262144 },
  "capabilities": {
    "beat_loop.lengths": ["1/2", "1", "2", "4", "8", "16", "32"],
    "beat_loop.buttons": 6,
    "jog_display": { "available": false, "reason": "absent on this model" }
  },
  "screens": ["main", "perform"]
}
```

Capabilities are named facts about the stock firmware. Missing or unusable ones carry a reason
("absent on this model", "broken on this firmware", "not mapped yet"), which the builder shows.

### Screen

One file per player and screen. A screen names its layout data, the images it uses, and its
**slots**: named places a skin can fill or a feature can use.

```json
{
  "id": "perform",
  "player": "xdj700-v1.15",
  "layout": { "table": "0x08E3A684", "record": "pad-layout-v1" },
  "slots": {
    "beat_loop.pad": { "count": 6, "box": [116, 154, 500, 53], "states": 6 }
  },
  "evidence": { "layout_read_at_startup": true, "used_in_update_mode": false }
}
```

The `evidence` block records what the emulator showed about the screen's data. It decides which
kinds of edits the screen accepts: pixels only, or positions too (see
[Extending the limits](#extending-the-limits)).

### Feature

A behaviour change, declared against capability and slot names, never against a player:

```json
{
  "id": "beat-loop-1-to-32",
  "requires": ["beat_loop.lengths:32", "slot:perform.beat_loop.pad"],
  "conflicts": ["beat-loop-16-plays-32"],
  "provides": ["beat_loop.buttons:1,2,4,8,16,32"],
  "maturity": "experimental",
  "implementations": { "xdj700-v1.15": "recipes/xdj700-v1.15/beat-loop-1-to-32.json" },
  "acceptance": ["perform/pads-select-lengths", "perform/lit-pad-follows-touch"]
}
```

`requires` and `conflicts` are what let the builder switch a feature off with a reason. An
implementation is a recipe fragment for one player: replacements, image edits, and later code.
`acceptance` names emulator tests the implementation must pass (below).

The sketch is illustrative. Its values come from the XDJ-700 recipes, but two details show where
the design goes further than today: `conflicts` is about meaning, not bytes (this feature and
`beat-loop-16-plays-32` share a replacement, which composition would apply once, but they define
different button sets), and today's stage-7 recipe would fail `perform/lit-pad-follows-touch`
until the lit-pad code fix exists (its known limit).

### Skin

A look for one screen. A skin requires a screen class and slots, fills them with its own art or
with transforms of the player's own images, and gives a fallback for every slot it depends on:

```json
{
  "id": "perform-dark-pads",
  "screen": "perform",
  "requires": { "screen_class": "800x480 rgb565", "slots": ["beat_loop.pad"] },
  "art": "original",
  "fallback": { "beat_loop.pad": "stock" },
  "implementations": { "xdj700-v1.15": "skins/xdj700-v1.15/perform-dark-pads.json" }
}
```

Skins are named for what they look like ("dark pads", "full-width waveform"), never after another
product, and carry no logos.

### Profile (the owner's build)

The owner's choices, small and shareable, separate from the definitions so that they survive a
firmware port:

```json
{
  "player": "xdj700-v1.15",
  "screens": { "main": "stock", "perform": "perform-dark-pads" },
  "features": ["beat-loop-1-to-32"],
  "label": "Ver1.16",
  "reported_version": "0.12"
}
```

## Building a profile

1. **Resolve.** Load the player, its screens, and the chosen skins and features. Check each
   `requires` against capabilities and slots, each `conflicts`, and maturity against the owner's
   setting. Report every switched-off item with its reason; never drop one silently.
2. **Compose.** Collect the fragments into one rebuild with one label and one reported version.
   Spans must be disjoint; a replacement repeated exactly by two fragments is applied once (the
   rule `check_windows_across` already allows). Image edits must not overlap unless they are the
   same edit.
3. **Check.** Everything `patch` checks today: pins, windows and leak rules, protected ranges and
   the protected set, image bounds, the size budget, the bounded diff, and the rebuild's own
   verification. In addition, each fragment keeps its own output pin (the identity it produces
   when applied alone to the official file). The builder first applies each fragment alone and
   confirms the result against that pin; the composed application must then equal, at every
   byte a fragment changes, that fragment's own verified output, and stock everywhere else
   (apart from the version string). This keeps what an output pin guards today, for image edits
   especially, whose pixels no hash covers.
4. **Report.** The output identity, each fragment's evidence and tier, and the restore plan (the
   official file, and the stock no-op stick).

A combination of fragments has no published output pin of its own unless it is a listed
combination: it is checked through its fragments' pins as above, the builder prints the identity
it produced, and the fragments carry the evidence.

## Verification

- **Static:** the existing recipe checks, applied to every fragment alone and to the composed
  build.
- **Emulator rehearsal:** every fragment is installed over stock and stock over it, with read
  watches on everything it changes (as for stages 5 and 7).
- **Acceptance tests:** scripted emulator runs that drive the panel and touch screen and assert
  behaviour: which length-list entry a touch selects, which pad lights, that changed data is not
  read at start-up. The probes used on 2026-10-10 (call logs, the pad-state array, screen-region
  measurements) become these tests. Stage 7 shows why: its lengths were right, but a test of the
  lit pad would have caught that the lighting follows the stock lengths.
- **Maturity tiers:** `stable` (passed on hardware), `experimental` (emulator only), `dev` (not
  offered). The builder offers `stable` by default.
- **Hardware stages:** new fragments go through the flashing guide's staged tests on the owner's
  units (XDJ-700 and XDJ-1000MK2), with the official file as the way back.

## Extending the limits

Three limits stand between today's recipes and full skins. Each is lifted only through its gate.

### 1. Data read at start-up (moving elements)

Today the bytes a recipe changes must not be read during a normal boot, an update-mode boot, or
a complete update in either rehearsal direction ([xdj700-flashing.md](./xdj700-flashing.md),
section 5). A screen's layout table is read at every start-up (it is copied to RAM), so moving an
element is refused. The gate below relaxes the rule for layout tables only, screen by screen:
the copy at start-up is allowed when the copied data is used only later.

Gate, per screen:
1. In emulation, show where the table's copy is consumed: only when the screen is built, after
   the update-mode decision, and never by the update-mode screen.
2. Validate layout records structurally: positions inside the screen, sizes matching the images,
   image numbers inside the archive, so a malformed record cannot reach the drawing code.
3. Rehearse both ways and boot both paths with the moved layout; then a hardware stage.
4. Record the result in the screen's `evidence`; only then does the screen accept positions.

Longer term, a **safe mode** (a button held at power-on that skips the project's changes) would
make start-up changes recoverable without a reflash. It needs a code change, and one that runs
at start-up, before the update-mode decision and inside the protected set: the riskiest place,
which limit 3's gate excludes. Safe mode therefore needs its own, stricter gate (to be written
before any work on it), not limit 3's.

### 2. Size budget

A rebuilt application may grow by at most 256 KiB, because the size of the flash region that
holds it is not confirmed. Larger skins need more.

Gate:
1. Establish the region from the loader: where an update erases and writes, and where the
   settings area starts (emulation already shows the application region erased up to
   `0x7DFFFF` and settings from `0x7E0000`, in the emulated layout).
2. Confirm on hardware without risk to the loader: the loader region is never written, and the
   official file restores the application region in full.
3. Raise the bound with a margin, in the release's pins, with the evidence cited.

### 3. Code changes

Some features cannot be expressed as data. Stage 7's lit pad appears to be chosen by code from
the loop length (no data table holding the mapping was found). Code fragments would place new code in verified unused areas (the three `0xFF` runs
already checked at run time) and divert to it from short hooks.

Gate, per fragment:
1. The hook and the new code lie outside the protected set; the hook is not reached before the
   update-mode decision or in the update path (emulator coverage).
2. The unused areas stay unused by stock in every rehearsal.
3. Acceptance tests cover the behaviour; rehearsals pass both ways; then a hardware stage.

The first candidate is the lit-pad rule, so that stage 7's lighting follows the pad that was
touched.

## Players

- **XDJ-700 v1.15:** the current target; three committed recipes.
- **XDJ-1000MK2 v1.45:** next. The owner has a unit; the emulator already runs this model. Needs
  the owner's official file, its pins, its own protected set from rehearsals, and a check that
  its image archive and layout tables match the XDJ-700's format.
- **Same family later:** players with the same processor, 800x480 panel and unencrypted update
  format may follow. Players whose update files are encrypted are out of scope for an
  owner-file builder.

A look modelled on a larger-screen player can only be a redrawn skin at this player's resolution.

## Community content

- Features and skins are data in this repository, validated by CI with the same schemas.
- Code keeps the repository's licence; art carries an open art licence (to be decided by the
  maintainer); contributors declare that art is their own and contains no extracted vendor
  images.
- No firmware, keys or built updates are accepted.
- Each player, screen, feature and skin names a maintainer.

## Roadmap

1. Schemas for player, screen, feature, skin and profile; composition of several fragments into
   one rebuild; the builder's switched-off reasons.
2. Emulator acceptance harness: scripted, asserting tests per feature and screen.
3. Screen catalogue for the XDJ-700: `main` and `perform`, with evidence.
4. First per-screen skins (PERFORM, main), from original art or owner-local transforms.
5. Code fragments, starting with the lit-pad rule.
6. The XDJ-1000MK2 v1.45 as the second player.
7. A builder that runs in the browser on the owner's computer, and signed releases of the
   catalogue.
8. Custom skins with positions, screen by screen as limit 1 is lifted.

## Open questions

- Where the XDJ-700 derives the "selected button" value from the loop length (for the lit-pad
  fix).
- Whether the XDJ-1000MK2's image archive and layout records match the XDJ-700's.
- The art licence for community skins.
- Whether the builder runs in the browser, as a desktop app, or both.
