# The catalog: players, screens, features and skins

The catalog is the data a modular build is chosen from: which players the project supports, their
screens, the features that change behaviour and the skins that change a screen's look, each with
one implementation (a schema-v2 recipe) per player. It implements the concepts of the design,
[modular-builds.md](./modular-builds.md); recipes are described in [recipes.md](./recipes.md).

**Status.** The schemas, the committed XDJ-700 v1.15 catalog, the checks that need no firmware,
resolving a profile (`patch-cli resolve`) and building one (`patch-cli build`) are implemented
(below). No skin is committed yet.

## Layout

```text
catalog/
  players/<player>.json             one model and firmware release
  screens/<player>/<screen>.json    one screen of that player, and its slots
  features/<feature>.json           a behaviour change, with an implementation per player
  skins/<skin>.json                 a look for one screen, with implementations per player
```

Every file is named after its `id`, and a screen lies under its player's directory, which must
belong to a player in `players/` and hold at least one screen. Anything
else in `catalog/` is refused, so a misnamed file cannot be skipped silently; only a `README.md`
and a `.DS_Store` are skipped. A symbolic link in the catalog, or in a recipe path, is refused: it
could point outside the tree. Every file has `"schema_version": 1` and names a `maintainer`.
Unknown fields, and a key given twice in an object, are refused, and so is a control, format or
line-separator character (a line break, a tab, a bidirectional override) in any text shown to the
owner.

Ids are lowercase letters, digits and `-` (a player id may also hold `.`: `xdj700-v1.15`, the
release id recipes name), at most 64 bytes. Capability and slot names are lowercase segments of
letters, digits and `_` joined by `.` (`beat_loop.pad`). A slot is referred to as
`<screen>.<slot name>` (`perform.beat_loop.pad`).

## Player

[`catalog/players/xdj700-v1.15.json`](../catalog/players/xdj700-v1.15.json):

- `model`, and `firmware`: the update's file name and its pins (`upd_sha256`,
  `application_sha256`), the same as its recipes' `target`. A test checks them against the
  engine's own pins.
- `screen_class`: `width`, `height` and `pixels` (`rgb565`), what a skin is drawn for.
- `budgets.compressed_main_growth_bytes`: the engine's bound on the compressed MAIN image's
  growth (checked against it).
- `capabilities`: named facts about the stock firmware. Each gives the `values` established for
  it, or why it is `unavailable` ("absent on this model"), and its `evidence`. Values list what the
  evidence shows, not necessarily everything the firmware holds: `beat_loop.lengths` names the six
  stock button lengths and 32, the one other length a button has been shown to set.
- `screens`: the screens catalogued for it, each with its own file.
- `restore`: how an owner brings back the stock application after flashing a build, which
  `patch-cli build` prints with every build: what the `official` update does over a build, the
  `backup`, and the player's flashing `guide` (`docs/<file>.md`, a file the loader checks).

## Screen

[`catalog/screens/xdj700-v1.15/perform.json`](../catalog/screens/xdj700-v1.15/perform.json): an
`id` the same for every player that has the screen (`main`, `perform`), its `player`, a `title`,
and its `slots`: named places a skin can fill or a feature can use, each with a `count` (six BEAT
LOOP buttons) and a `description`. Layout data and the emulator evidence that decides which edits
a screen accepts come with the screen catalogue (design roadmap, step 3).

## Feature

[`catalog/features/beat-loop-1-to-32.json`](../catalog/features/beat-loop-1-to-32.json):

- `requires`: `capabilities`, each holding the listed values (an empty list: only available), and
  `slots`. A feature is declared against these names, never against a player.
- `conflicts`: features that mean something else for the same controls, listed on both sides.
  This is about meaning, not bytes: `beat-loop-1-to-32` and `beat-loop-16-plays-32` share a
  replacement, which composition would apply once, but define different button sets.
- `labels`: the text the feature gives each slot it relabels, one label per element (1 to 16
  printable ASCII characters). Every relabelled slot is in `requires.slots`, and two features that
  relabel one slot must conflict.
- `implementations`, by player id, a list: one per way of drawing the labels (`draws_labels`), so
  a feature can have one implementation that draws them in the stock style and one that leaves
  them to the chosen skin. Each has:
  - `recipe`: a path under `recipes/`. The recipe must be for that player, carry its pins, and pin
    its output (`expected.application_sha256`), since every fragment of a build is checked
    against its own output.
  - `maturity`: `stable`, `experimental` or `dev`, and its `evidence`. A passed hardware stage is
    needed for `stable`, but the maintainer decides: `beat-loop-16-plays-32` passed on an owner's
    unit and stays `experimental` while its lit-pad limit stands.
  - `limits`: known limits the builder shows with the feature.
  - `draws_labels`: the screens whose labels the recipe draws itself in the stock style
    (`{"perform": "stock"}`), so each such screen must keep the `stock` skin. A labelled screen
    not listed is left to the chosen skin, which must have an implementation drawing that label
    set (the built-in `stock` skin has none: its labels are drawn by features). Only `stock` is
    accepted for now: a skin cannot yet leave slots for a feature to draw, so a feature drawing
    in another skin's style would edit the same images as that skin. Labels belong to skins;
    today's stage-7 recipe draws its labels in the stock style, so the PERFORM screen keeps the
    stock skin with it.

The implementation must meet the feature's requirements on its player: the capabilities with
their values, and the slots, with as many elements as there are labels.

## Skin

A skin is a look for one `screen`. It is named for what it looks like, never after another
product, and carries no logos. The skin `stock` (the player's own look) is built in and has no
file. No skin is committed yet; the format:

```json
{
  "schema_version": 1,
  "id": "dark-pads",
  "title": "Dark pads",
  "screen": "perform",
  "requires": {
    "screen_class": { "width": 800, "height": 480, "pixels": "rgb565" },
    "slots": ["beat_loop.pad"]
  },
  "art": "original",
  "implementations": {
    "xdj700-v1.15": [
      { "recipe": "recipes/…", "maturity": "dev", "evidence": "…" },
      {
        "recipe": "recipes/…", "maturity": "dev", "evidence": "…",
        "labels": { "beat_loop.pad": ["1", "2", "4", "8", "16", "32"] }
      }
    ]
  },
  "maintainer": "…"
}
```

- `art`: `original` (drawn by the skin's authors, who declare it their own) or `transform`
  (computed from the player's own images on the owner's computer; no pixel is published).
- `implementations`: per player, one per label set the skin draws (none: the stock labels), since
  each (skin, label set) pair has its own output pin. Two implementations may not draw the same
  label set. The player must have the screen, the required slots and the screen class.

## Profile

The owner's choices, kept apart from the catalog so that they survive a firmware port:

```json
{
  "schema_version": 1,
  "player": "xdj700-v1.15",
  "screens": { "perform": "stock" },
  "features": ["beat-loop-1-to-32"],
  "label": "Ver1.16",
  "reported_version": "0.12",
  "maturity": "experimental"
}
```

A screen not listed keeps the `stock` skin. `maturity` is the least settled implementation the
owner accepts: `stable` (the default) or `experimental`; `dev` is never offered. The label and
reported version follow the recipe rules; the engine checks them against the release.

## Resolving a profile

`patch-cli resolve --profile <file>` (run from the repository root, or with `--root`) loads the
catalog, resolves the profile and prints every choice, on or off with its reason, and the
fragments a build composes; an item that is on comes with its evidence and limits. It needs no
firmware and writes nothing:

```text
player: xdj700-v1.15
label: Ver1.16
reported_version: 0.12
accepts: experimental and stable
screen main: stock
screen perform: stock
feature beat-loop-1-to-32: on (experimental, recipes/xdj700-v1.15/beat-loop-1-to-32.json)
  evidence: Rehearsed in emulation, both ways; not yet tested on hardware (…)
  limit: The lit pad follows the stock lengths: …
fragments: 1
fragment[0]: recipes/xdj700-v1.15/beat-loop-1-to-32.json (experimental; feature beat-loop-1-to-32)
tier: experimental
note: the build is recipes/xdj700-v1.15/beat-loop-1-to-32.json's own output, under its own …
```

A fragment line ends with why its build is a new update when it is: the profile's label or
reported version is not the recipe's own, or the recipe pins no update file.

The profile's label and reported version must pass the release's rules (higher than its own
version, and lower), as a build applies them. The choices are honoured as far as they fit, and
nothing is dropped silently:

1. A skin or feature that is not in the catalog, has no implementation for the player, or none
   at the maturity the profile accepts, is off. So is a skin for another screen. A screen the
   player lacks is listed apart, and the profile's choice for it is ignored.
2. Chosen features that conflict are both off: the owner chooses one. This is decided once,
   whatever the skins.
3. The chosen skins are kept as far as they can be. Of every set of them, largest first, and
   every choice of one implementation per kept skin, resolution takes the configuration in which
   the most features are on, where each feature takes its first implementation that fits the
   skins (it draws the labels of a screen that keeps `stock`, in the stock style, and leaves
   those of a screen with a kept skin to that skin), and each kept skin's implementation draws
   exactly the labels of the features kept on. A feature whose labels a kept skin does not draw
   is off (with a note when it has a stock-style implementation); a skin left out keeps its
   screen on `stock`.

Keeping no skin always works, so there is always a result. Apart from ties, what is on does not
depend on the order in which screens are listed; a tie between equally good configurations goes to
the first in screen and file order. The search is small for real catalogs; one needing more than
65,536 configurations is refused.

The **tier** of the build is its least settled fragment's when it is one fragment under the recipe's
own label and reported version, pinning its update file (the very file its pins describe), and
`experimental` at most otherwise: a combination, another label or version, or a recipe that pins no
update file, is a new update that no listed combination covers yet
([modular-builds.md](./modular-builds.md)). `Resolution::buildable` gives the tier of a build, or
why there is none to make, and `Resolution::pinned_output` the update file a build must be. Only an
invalid profile, a player the catalog lacks, a label or version the release refuses, or a search
over more than 65,536 configurations is refused outright. Front ends call
`patch_cli::catalog::resolve_profile`, which applies the release's label and version rules before
the search.

## Building a profile

`patch-cli build` resolves the profile as above, prints the resolution (without the note), and
composes its fragments under the profile's label and reported version into one update, as
`patch-cli compose` does ([recipes.md](./recipes.md), "Composing recipes"):

```bash
cargo run --release -p patch-cli -- build \
  --profile /path/to/profile.json \
  --input /path/to/XDJ700.UPD \
  --output /path/to/new-dir/XDJ700.UPD \
  --protected-set /path/to/protected-set.tsv
```

Before the input is read, the build is refused when nothing is on (every choice is off or keeps
`stock`), or when its tier is less settled than the profile accepts (`Resolution::buildable`;
a new update is `experimental` at most). Every fragment then passes its own checks and the
protected set, and the composition its checks; the input must be the player's official update;
each fragment is applied alone and must reproduce its pinned output, and the composed build is
checked against each fragment's output byte for byte before it is written. A profile with one
feature under its recipe's own label and version builds that recipe's own pinned update, and the
output is checked against that pin before it is written: the owner-input tests build the stage-5
and stage-7 files this way.

Once the file is written, `build` prints its identity, the note saying what it is, and the
player's restore plan (`restore:`, `restore_backup:`, `restore_guide:`, from the player's
`restore`). A refused build prints the resolution only. Anything but a recipe's own pinned file
is a new update: rehearse it both ways in emulation before flashing, and follow the player's
flashing guide ([XDJ-700](./xdj700-flashing.md)). Flashing is the owner's decision.

## Checks

Each file is checked on its own, then the catalog as a whole, with no firmware:

- ids unique; every player's screens have files, and every screen's player lists it;
- conflicts resolved and listed on both sides; features that relabel one slot conflict;
- every implementation's player exists and meets the requirements; label counts match slot
  counts; a skin named in `draws_labels` is built in, or exists, is for that screen and has an
  implementation for the player;
- every recipe named exists, is a valid schema-v2 recipe for its player, carries the player's pins
  and pins its output;
- every player's flashing guide is a file in the repository, not a symbolic link.

The loader (`patch_cli::catalog::load_catalog`) then holds the catalog to the engine: every
player is a release the engine pins, with the same pins and budget, and every recipe passes the
engine's firmware-free checks for its release. So what loads is what the engine accepts, short of
the firmware itself. CI loads the committed catalog (`core/patch-cli/tests/committed_catalog.rs`).
Whether a recipe applies to the official file, and what a composed build holds, needs the file
and is checked by `patch` and `compose`.

## Not yet in the format

These come with the steps of the design that need them: `provides` (a feature's capabilities for
others), `acceptance` (named emulator tests, step 2), a screen's layout data and evidence (step
3), and skin fallbacks. A file using them is refused until then.
