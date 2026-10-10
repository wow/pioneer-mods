# Changelog

All notable changes to this project should be documented in this file.

The format is inspired by Keep a Changelog and follows [VERSIONING.md](./VERSIONING.md).

## [Unreleased]

### Added
- `patch-cli build`: a profile resolved against the catalog (as `patch-cli resolve`) and its
  fragments composed under the profile's label and reported version (as `patch-cli compose`), with
  every check that needs no firmware before the input is read; refused when nothing is on or when
  the build is less settled than the profile accepts (`Resolution::buildable`, for every front
  end). It reports the resolution, and once the file is written its identity and the player's
  restore plan. A profile with one feature under its recipe's own label and version builds that
  recipe's pinned update, checked against the pin before it is written
  (`Resolution::pinned_output`): the owner-input tests build the stage-5 and stage-7 files byte
  for byte. `docs/catalog.md`, "Building a profile".
  `patch_cli::recipe::CheckedRecipe::from_recipe` checks a recipe already parsed. `Status::On`
  carries the implementation's evidence, and `resolve` and `build` print it. A player names its
  `restore` plan and flashing guide, which the loader checks is a file. A lone fragment keeps its
  tier only if its recipe pins its update file (`Fragment::pinned_update`).
- `patch-cli resolve` and `patch_schema::catalog::resolve`: a profile resolved against the catalog
  (`Resolution`): every skin choice and feature on or off with its reason, the implementation each
  uses, the fragments a build composes and the build's tier (a lone fragment under its own label and
  version keeps its maturity; anything else is experimental at most). Chosen features that conflict
  are both off, whatever the skins; then the chosen skins are kept as far as they can be, and of
  those configurations the one with the most features on is taken, each feature with its first
  implementation that fits the skins and each kept skin drawing exactly the labels of the features
  kept on. `resolve` takes a `CheckedCatalog` (`Catalog::into_checked`);
  `patch_cli::catalog::resolve_profile` applies the release's label and reported-version rules first
  (`xdj700::check_label_and_version`, now public), and `load_catalog` returns the checked catalog.
  Needs no firmware; `docs/catalog.md`, "Resolving a profile". `draws_labels` accepts `stock` only
  for now, and catalog text may not hold control, format or line-separator characters.
- `patch_schema::catalog` and `catalog/`: the formats of players, screens, features, skins and
  profiles (`docs/catalog.md`), and the committed XDJ-700 v1.15 catalog: the player with its pins,
  budget and capabilities, the `main` and `perform` screens, and the two BEAT LOOP features with
  their implementations, maturity, evidence and known limits. Each file is checked on its own and
  the catalog as a whole (`Catalog::check`): unique ids, resolved references, conflicts on both
  sides (and between features that relabel one slot), implementations that meet their requirements,
  label counts that match slot counts, and recipes for their player that carry its pins and pin
  their output. `patch_cli::catalog::load_catalog` reads it without trusting the files (unknown
  fields, repeated keys and symbolic links are refused) and holds it to the engine: every player is
  a release the engine pins, with its pins and budget, and every recipe passes the engine's
  firmware-free checks.
- `patch-cli compose` and `xdj700::compose_recipes` (`compose_recipes_to`, `check_composition`,
  `CheckedComposition`, `check_composed`, `Composition`, `ComposedUpdate`, `ComposeError`): several
  schema-v2 recipes of one release built into one update with one label and one reported version, as
  `docs/modular-builds.md` describes. Each recipe is given once, must pin its output and is first
  applied alone and checked against it; windows must be disjoint across the recipes except exact
  repeats, and images disjoint unless they are the same edit (`patch_schema::Replacement::repeats`,
  `ImageEdit::same_edit`); repeats and same edits are applied once; the composed application must
  equal each recipe's own output where it changes bytes, and stock elsewhere apart from the version
  string. The owner-input tests compose stages 3 and 5, and stages 5 and 7, and reproduce the
  stage-5 and stage-7 files byte for byte. Format and rules in `docs/recipes.md`, "Composing
  recipes".
- `docs/modular-builds.md`: the design for modular builds (a proposal, nothing implemented yet).
  An owner picks a player, a skin per screen and features. Players, screens with named slots,
  features with requires/conflicts/provides, per-screen skins with fallbacks, and the owner's
  profile are data, composed into one verified rebuild. It defines verification by maturity tier
  (emulator acceptance tests, rehearsals, hardware stages; combinations rehearsed as a whole or
  offered as experimental), the evidence gates for extending today's limits (data read at
  start-up, the bound on the compressed MAIN image, code changes, same-length images), with
  section 5 of the flashing guide staying the authoritative statement of the rules, rules for
  community content, and a roadmap with the XDJ-1000MK2 v1.45 as the second player. The README
  links it.
- `recipes/xdj700-v1.15/beat-loop-1-to-32.json`, **experimental; rehearsed in emulation, not yet
  tested on hardware:** the PERFORM screen's six BEAT LOOP buttons read and set 1, 2, 4, 8, 16 and
  32 beats instead of 1/2, 1, 2, 4, 8 and 16, reporting `0.12`. One span with kept bytes (`--`)
  changes the first byte of the button table's first five entries, the sixth repeats the stage-5
  replacement exactly, and 36 image edits relabel each button's six images with the project's own
  digits. In emulation no code read the table or the images at start-up, in update mode or while
  installing in either direction, the pads read 1, 2, 4, 8, 16, 32 and selected those lengths, and
  the four rehearsals passed. The owner-input tests pin the stage-7 file (`259c75da…`) and run the
  guide's `patch` command for stages 5 and 7. They now check every committed recipe's output against
  a bounded diff derived from the recipe (the version string, the bytes its replacements write, and
  its images' erase and glyph boxes), with the stage pins in one table and each stage's exact
  changes outside the images; stage 3 now also pins its MAIN length. A firmware-free test checks
  that each button's six image edits share one drawing, and the protected-set test requires the
  stage-7 recipe. The window over the 3,000 bytes before the table is recorded under "Known
  exceptions" in `docs/recipes.md`. `docs/xdj700-flashing.md` adds stage 7 (the experiment) and
  stage 8 (restore).
- Kept bytes in schema-v2 replacements: `--` in `bytes_hex` keeps the stock byte at that place
  and does not publish it (`Replacement::pattern`, `Replacement::written_runs`), so changes a few
  bytes apart, such as the fields of one table, fit one span without publishing the stock bytes
  between them. A span must write its first and last bytes (`RecipeV2Error::KeptSpanEdge`) and
  keep fewer than 32 bytes in a row (`RecipeV2Error::LongKeptRun`). Kept bytes stay inside the
  precondition window and its hash, and only the written runs are declared, so the bounded diff
  checks that kept bytes stay stock.
- Image edits in schema-v2 recipes (`image_edits`, `patch_schema::ImageEdit`): changes to a 16-bit
  RGB565 image stored in the decoded application that publish no stock pixel and no hash of one
  (anyone could check a guess at the pixels against it). An edit names the image (offset, width,
  height); it erases a box by interpolating each row between the stock pixels just outside it,
  then draws the author's own coverage mask (one hex digit per pixel) in the colour of a stock
  pixel it names. The pixel arithmetic is fixed in `patch_schema` (`ImageEdit::erase_row`,
  `blend`). A recipe with image edits must pin its output (`expected.application_sha256`,
  `RecipeV2Error::UnpinnedImageEdits`): with the input pinned, that catches a changed offset or
  arithmetic. Images are refused if they leave the application or overlap each other, a
  replacement's window (also another committed recipe's), a protected range or the protected set;
  images of different recipes may overlap. The output may differ from stock only in the edited
  rows. `patch-cli precondition` now rebuilds every draft once: it prints each image without a
  hash and the output identities (`xdj700::PreconditionHashes::output`,
  `xdj700::OutputIdentities`), compared through one list with `patch` (`OutputIdentities::pins`);
  `--check` counts precondition hashes and output identities separately. `patch` reports
  `image_edits`.
  `RecipeError` moved to its own module, with the image refusals. Format in `docs/recipes.md`,
  "Image edits".
- The protected set on `patch-cli patch` and `precondition` (`xdj700::ProtectedSet`,
  `xdj700::check_recipe_against_protected_set`, `xdj700::RecipeChecks`): the code that runs at
  start-up and in the update path, measured in emulation and kept outside the repository, as
  run-time address ranges with inclusive ends for one named release (format in
  `docs/recipes.md`, "The protected set"). Before the firmware is read, a schema-v2 recipe is
  refused if the set belongs to another release, covers the version string every rebuild writes,
  or overlaps a span or precondition window (reported in run-time addresses). Skipping the check
  is a decision: the set comes from `--protected-set <file>` or `XDJ700_PROTECTED_SET`, and
  without one both commands refuse to run unless `--no-protected-set` is given, which warns and
  reports `protected_set: skipped` (the documented owner commands pass it; the maintainer checks
  the committed recipes). `apply_recipe_v2`, `apply_recipe_v2_to` and `precondition_hashes` take
  a `RecipeChecks` argument and run the check again themselves. A malformed set is refused with
  its line number (no release line, a header other than `start end [bytes]`, decoded offsets in
  place of run-time addresses, a byte count that does not match inclusive ends, ranges outside
  the application, an empty set); a byte-order mark is ignored. `RecipeTarget` gains
  `load_address` (`0x0800_0000` for v1.15). An ignored test checks the committed recipes against
  `XDJ700_PROTECTED_SET` (skipped, saying so, without it); both pass against the measured set
  (1,692 ranges), and the stage-5 output is unchanged.
- `recipes/xdj700-v1.15/beat-loop-16-plays-32.json`, **experimental; passed on an owner's unit
  (2026-10-09)**, where BEAT LOOP 16 looped 32 beats and the official update restored stock:
  the BEAT LOOP button labelled 16 sets a 32-beat loop (the label still reads 16), reporting
  `0.11`. Static analysis found the player's list of loop lengths (which already holds 32 beats)
  and the six-entry table mapping the PERFORM screen's buttons to it, read only by the BEAT LOOP
  touch handler; the recipe changes that table's last entry. The decoded application differs from
  stock in exactly three bytes (version string and table entry), checked by the owner-input
  tests, which also pin the stage-5 file and run the guide's `patch` command. Its precondition
  window lies over table data, recorded under "Known exceptions" in `docs/recipes.md`.
  `docs/xdj700-flashing.md` adds stage 5 (the experiment) and stage 6 (restore).
- `patch-cli precondition --input --recipe --committed-recipes [--check]`
  (`xdj700::precondition_hashes`): prints the precondition hashes of a schema-v2 recipe, typically a
  draft with placeholder hashes. Before the input is read, the recipe is checked and its windows
  must be disjoint from those of the other committed recipes, or repeat one of their replacements
  exactly (`patch_schema::check_windows_across`, shared with CI; the draft's own file is skipped,
  and the directory must hold another recipe for the release); each window then passes the leak
  checks before its hash is computed (`checked_window`, shared with applying a recipe), so no hash
  of a refused window is ever shown. It writes nothing; `--check` fails unless every declared hash
  matches. The recipe reader, the version dispatch and the v2 checks are shared with `patch`
  (`patch_cli::recipe`), and the recipe walk with the tests (`patch_core::recipe_files`, which
  refuses symbolic links).
- **Recipe schema v2** (`patch_schema::RecipeV2`, `docs/recipes.md`): same-length replacements in
  the decoded application of a pinned release. Each replacement has a precondition: the SHA-256 of
  the stock bytes `before` and `after` the span, with at least 32 bytes outside the span
  (`MIN_PRECONDITION_LEN`). The window moves with the offset, so a mistyped offset fails, and over
  a window of code the hash is impractical to invert to recover vendor bytes. A recipe declares a
  label higher and a reported version lower than the release's own, and may pin its output
  identities (an empty `expected` is refused). `patch` refuses any `schema_version` other than 1
  and 2. Precondition windows are at most 4096 bytes and may not overlap each other or a
  protected range; CI checks that the windows of all committed recipes are disjoint, too. On the
  stock application the engine refuses a window in which four byte values fill more than half of
  the bytes outside the span, and a span that keeps its first or last stock byte, more than half
  of them, or 32 in a row (`bytes_hex` would publish them). A precondition mismatch does not
  print the actual hash. These guard against accidentally revealing stock bytes; they are
  heuristics, and review remains the backstop.
- **Recipe engine** (`xdj700::apply_recipe_v2`, `check_recipe_v2`, `RECIPE_TARGETS`). Before
  reading the input it checks the recipe, the release pins, the version order and the protected
  ranges (v1.15: `[0, 0x800)`, the header and version block). On the official file it checks each
  precondition, that only the declared spans and the version string changed, the rebuild's own
  verification and the expected identities. It is built on the new
  `xdj700::rebuild_with_edited_stock_application`, whose `edit` returns its declared ranges: the
  entry point checks the bounded diff for every caller (`RebuildError::UndeclaredChange`).
- `patch-cli patch` chooses the engine by `schema_version`. A v2 recipe is checked before the
  input is read; the input is length-checked before reading; the output is never overwritten.
- `recipes/xdj700-v1.15/version-marker-0.10.json`, the first committed recipe. It reproduces the
  hardware-tested stage-3 file (`84cbd263…`), checked by the owner-input tests. CI checks that
  every committed recipe passes the firmware-free checks and pins its outputs.
- `xdj700::stock` holds the verified rebuild input and its verification (split from `rebuild`);
  `patch_cli::input::read_pinned_input` is shared by `rebuild` and `patch`.
- `patch-cli rebuild --report-version X.YY` sets the version the application reports about
  itself (the NUL-terminated string at decoded offset `0x740`, `xdj700::VERSION_STRING_OFFSET`);
  only those bytes change. It must be lower than 1.15, so that the official v1.15 update is a
  higher version and restores the stock application. The engine step is
  `xdj700::rebuild_with_stock_application_reporting`, which loads the input once.
  `RebuiltUpdate` reports the verified application's version, and `inspect --structure` always
  reports `reported_version` (`none`/`null` when there is no well-formed string). The owner-input
  tests pin the stage-3 file (`Ver1.16`, reporting `0.10`: `84cbd263…`), cross-checked
  byte-identical against the reference serializer.
- **Release rule for modified applications.** `StockRelease::version_block` (a `VersionBlock`:
  offset, stock version and stock application SHA-256) pins where each release's application
  reports its version; `OFFICIAL_V115` pins `0x740`, `1.15` and `1875381b…`. Every
  `rebuild_with_application` and `verify_rebuild` refuses a modified application that does not
  report a version lower than the stock one (`RebuildError::ModifiedApplicationVersion`),
  including one with no version string, so the official update can always restore it. The
  hardware-tested reference alpha.2 reports `0.96` and passes. Release definitions moved to
  `xdj700::release`; one `X.YY` parser serves labels and reported versions.
- `patch_core::xdj700::FALLBACK_SECTION_OFFSET` (`0x10000`) records the loader's fallback updater
  section. Static analysis of the v1.15 loader, not yet observed on hardware, shows it runs this
  section instead of the application when the application section is left with a bad checksum. The
  owner-input tests pin its decoded identity and check that a rebuild keeps it intact. They also
  pin the hardware stage files: the no-op rebuild labelled `Ver0.90` (stage 1, the lower probe),
  `Ver1.16` (stage 1b, also the recovery stick) and `Ver1.17` (a spare).
- The XDJ-700 application layout is accepted when the MAIN label is verified (`Ver1.15`) **or**
  the loader region `[0, 0x40000)` matches the official v1.15 loader
  (`xdj700::VERIFIED_LOADER_SHA256`). The loader fixes the application offset, and rebuilds keep
  that region byte-identical, so `inspect --structure` decodes the stage files instead of
  reporting them as unsupported. `verify_main_version` is replaced by `verify_main_layout`
  (and `verify_main_layout_with`, which takes the loader identities to accept).
- `patch-cli rebuild` warns on stderr when the label is not higher than `Ver1.15`, since a unit
  running official v1.15 or later skips such a file. The comparison is
  `xdj700::is_label_higher`; the label helpers (`xdj700::OFFICIAL_V115_LABEL`,
  `is_label_higher`, `validate_version_label`) live in one place.
- Hardware result, stages 3 and 4 (owner's unit, 2026-10-08): the stage-3 file (stock
  application reporting `0.10`, label `Ver1.16`) was written (about 3 minutes); UTILITY then
  showed `0.10`, and the owner's checks of the unit were all good. The official v1.15 file then
  showed `MAIN Ver0.10 -> Ver1.15`, progressed for about 3 minutes, and UTILITY returned to
  `1.15`. So the unit reports the application's version string, and the vendor's own file
  restores an application that reports a lower version (observed for a version-only change).
- Hardware result (owner's unit, 2026-10-08): the `Ver1.16` no-op rebuild was flashed with MAIN
  progressing for about 3 minutes, as in a real write, and the unit boots and plays normally.
  Afterwards the unit still reports `1.15`, and the official v1.15 file shows `Ver1.15 -> Ver1.15`
  and is skipped: labels do not stick, and future official releases are expected to install
  normally. The guide's recovery stick is the `Ver1.16` stock no-op file; `Ver1.17` is kept only
  as a spare pin.
  The guide makes two rules for future modifications: stay out of the early start-up code, and
  report a version lower than 1.15 (see `--report-version` above).
- The owner flashing guide (`docs/xdj700-flashing.md`) records the observed updater behaviour:
  the updater writes only versions higher than the installed one; equal and lower versions
  (including the project's `Ver0.90` probe) are skipped. Stage 1b therefore uses `Ver1.16`, the
  smallest higher label. The guide also covers telling a
  real flash from a skip, the official update procedure, and what does and does not protect the
  unit.
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
  Motorola S-record syntax, checksum and layout validation (fail-closed). Parsed types have
  private fields and can only be built by the parser.
- `patch-cli inspect --structure` reports container documents, versions, CRCs, record counts and
  address extents in text or JSON. Without `--structure`, `inspect` output is unchanged.
- Canonical `.UPD` serializer (`UpdContainer::to_bytes`). It re-parses its own output and refuses
  unless the result equals the source container (`verify_serialized`). If the output does not
  parse, the error keeps the underlying parse error. `verify_roundtrip` is the
  Phase 1 no-op gate: parse, serialize, and require a byte-identical result
  (`UpdContainer::verify_reproduces`). The output is encoded into a single buffer and only
  re-parsed to diagnose a mismatch.
- Memory image reconstruction per document (`UpdDocument::image`): spans from the first to the last
  data byte, gaps filled with `0xFF`, capped at 64 MiB.
- `inspect --structure` now verifies the byte-exact roundtrip (`roundtrip_verified: true`) and
  reports each document's `image_span` and `image` with a `status`:
  - `reconstructed`, with an 8-digit base, length and SHA-256;
  - `span_exceeds_cap` (per-document 64 MiB cap);
  - `budget_exhausted` (128 MiB total across documents, which bounds the work a small crafted
    file can cause).
- Roundtrip and summary failures after a successful parse are reported as internal errors to
  report, not as an invalid input file.
- Ignored-by-default owner-input test (`official_firmware`) that checks an official XDJ-700 v1.15
  update against public pinned identities when `PIONEER_XDJ700_V115_UPD` is set.
- LZSS decoder for the XDJ-700 MAIN section format (`patch_core::lzss::decode`): 4096-byte ring
  pre-filled with spaces, absolute ring positions, 3..=18-byte matches, and a bounded output.
- XDJ-700 application section support (`patch_core::xdj700`). It verifies the size field, the
  16-bit additive checksum and the zero-prefix tag at MAIN image offset `0x40000`, then decodes
  the section with a 64 MiB output cap.
- `inspect --structure` reports the decoded application (offset, compressed length, checksum,
  decoded length and SHA-256) for XDJ-700 updates. Decoding is limited to MAIN images whose
  layout has been verified, by label (`Ver1.15`) or by loader region (see above); others are
  reported as `unsupported`.
  A container with more than one XDJ-700 MAIN document is refused as ambiguous. The MAIN image
  is built once, during the budgeted summary (`UpdContainer::summary_with_images`). An invalid
  section is reported with its reason instead of failing the whole report.
- `patch-cli rebuild --input <official .UPD> --application stock --label <VerX.YY> --output
  <new file>` writes a no-op rebuild of the official XDJ-700 v1.15 update: the stock
  application re-encoded under the declared label.
  - It refuses any input other than the pinned official file: the length is checked on the
    open handle before anything is read, then the library checks the hash. A malformed label,
    a missing output directory, or an existing output (including a dangling symlink) is refused
    before the input is read.
  - The output is never overwritten. It is written atomically: the temporary file is
    fsynced and read back (streamed, in chunks) before a no-clobber rename, so only verified
    bytes ever appear under the output name. Then the directory is synced. If that fails, the
    verified file is kept and the error says durability is not confirmed.
  - A file system that may lack a no-clobber rename (ENOTSUP/EOPNOTSUPP, or EPERM from the
    Linux hard-link fallback) gets a hint to write to a local disk.
  - `xdj700::rebuild_with_stock_application` rebuilds with the input's own application, parsing
    the input once. `StockRelease` also pins `upd_len`. `RebuiltUpdate` reports the decoded
    application's SHA-256. `patch_core::open_regular_file` is public.
  - It reports input, application, MAIN image and output identities.
  - `patch_core::xdj700::validate_version_label` is now public.
  - Owner guide for flashing rebuilt files: `docs/xdj700-flashing.md`.
- `patch_core::xdj700::rebuild_with_application` rebuilds a complete XDJ-700 `.UPD` around a
  new decoded application:
  - it encodes the section and places it after the input's unchanged loader region, dropping
    the input's trailing `0xFF` padding;
  - it re-cuts MAIN into 32-byte S2 records over the input's extents and sets the declared
    `VerX.YY` label;
  - it keeps every other document byte-identical and recomputes the CRCs and the length header.

  The input must be a pinned official release (`xdj700::StockRelease`; `OFFICIAL_V115` is the
  v1.15 file, by SHA-256). It must roundtrip byte-exactly, have a verified MAIN version and a
  valid section followed only by padding, and follow the 32-byte record grid. A rebuild is
  therefore never accepted as the next input. The MAIN image may be at most the release's bound:
  the official image plus `xdj700::MAX_MAIN_GROWTH` (256 KiB) until the flash layout is
  confirmed. The output is re-parsed and checked by `xdj700::verify_rebuild` before it is
  returned. `verify_rebuild` checks any output against its input independently of how it was
  produced, and each failed property has its own `RebuildCheck` variant. With the official v1.15
  file, the rebuild reproduces the reference alpha.2 MAIN image and update byte-for-byte
  (owner-input test `official_rebuild`). The no-op rebuild (the stock application re-encoded)
  is pinned. Under the stock label `Ver1.15` (`f2dd19d4…`) a v1.15 unit skips it; the hardware
  candidate is the `Ver1.16` file (`9e1ac10e…`, see `docs/xdj700-flashing.md`).
- Deterministic LZSS encoder (`patch_core::lzss::encode`, `encode_section_stream`). It is
  decision-identical to the reference encoder (see README, Acknowledgements). It was verified byte-identical on random,
  exhaustive and adversarial inputs and on the official v1.15 application. CI pins golden vectors
  for each search decision: history insertion order, probe order, the 4096 window edge, a binding
  candidate cap, and the early exit. Candidates are kept in hash chains (a fixed 24-bit key
  table plus window-sized links), so memory does not grow with the input: the official
  application encodes in about 0.6 s.
- `patch_core::xdj700::encode_section` builds complete section bytes (size field, stream,
  checksum). It is self-checked: the result must decode back to the input through
  `decode_section` (`xdj700::verify_encoded_section`), and on failure it reports the decode
  error or the mismatch. Inputs larger than the 64 MiB decode cap are refused up front with
  `SectionError::DecodedTooLarge`.
- `patch_core::read_firmware` reads an input once and returns its identity plus the hashed
  bytes; `read_regular_file` reads without hashing. Both refuse non-regular files (directories,
  FIFOs, devices): the path is checked first, the file is opened non-blocking on Unix so a path
  swapped to a FIFO cannot hang `open()`, and the open handle is checked again before reading.

### Changed
- Decided: community skin art is licensed under CC BY-SA 4.0, and the builder runs in the browser
  on the owner's computer, the official update never uploaded (`docs/modular-builds.md`,
  "Decisions"; `CONTRIBUTING.md`).
- Stage 7's known limit is documented: the BEAT LOOP lengths are right, but the pad that lights
  is the one to the right of the touched pad, and the 32 pad lights none, because the firmware
  lights the pad whose stock length matches the loop (found in emulation, 2026-10-10; no data
  table holding that mapping was found). One explanation, "The lit BEAT LOOP pad" in the
  flashing guide's section 2, with the stage-7 photo steps and the stage-5 and stage-8 checks
  adjusted to it, and the other places linking to it. In the
  flashing guide (stage 7 row and checks, section 4, section 5), the recipes table and the
  recipe's description; the recipe's output is unchanged. The same rule means that at stage 5 no
  pad lights while the 32-beat loop plays (confirmed in emulation); noted for stage 5 too, output
  unchanged. The README now lists the committed recipes with their status and describes kept
  bytes and the exact-repeat rule.
- **Breaking:** the leak rule on a span's bytes counts only the bytes it writes: at most half of
  them may equal stock, fewer than 32 in a row, and a kept byte (`--`) ends such a run. Spans
  without `--` are judged as before. The field `RecipeError::UnchangedSpanBytes::len` is renamed
  `written` and holds the number of written bytes, not the span length.
- The cross-recipe window rule is relaxed for one case: `check_windows_across` accepts a
  replacement repeated exactly in another recipe (offset, bytes, window and hash; the purpose may
  differ), since it publishes nothing new, so a recipe can build on another's change (the planned
  BEAT LOOP 1, 2, 4, 8, 16, 32 recipe repeats the stage-5 table entry). Any other overlap is still
  refused, and the refusal names the exact-repeat alternative. `precondition --committed-recipes`
  skips committed recipes with the draft's `recipe_id`, so a copy of the draft is not "another
  recipe"; CI refuses two committed recipes with one `recipe_id`.
- `docs/xdj700-flashing.md` section 5 carries the findings of a local emulation of the board
  (not part of this repository; not hardware) and becomes the authoritative statement of the
  rules for modifications, which `docs/recipes.md` and the `RECIPE_TARGETS` doc comment now
  refer to. In seven rehearsals (stages 1b, 3 and 5 installed over stock and the official file
  over each, plus a same-version skip) the updater never wrote the loader region `0x000000`–
  `0x03FFFF`; the IN + RELOOP/EXIT decision is one branch (`0x08D50D78`); the code before it,
  an update-mode boot and complete updates form a measured lower-bound set of run-time
  addresses that recipe windows must avoid (both committed recipes do); bytes a recipe replaces
  must not be read during start-up or an update (stage 5's byte is not, until BEAT LOOP 16 is
  touched); and from now on every new stage file is rehearsed before it is offered, in reverse
  as well when it reports a version below 1.15. Section 5 also explains run-time and flash
  addresses; skips are described as leaving the application unwritten, and section 3 asks
  owners to note their settings, since an update erased the settings area in emulation. An
  interruption while the loader region is written is still treated as uncovered.
- `patch-cli` output-file safety (input-path check, no-clobber atomic write) moved to a
  library module, `patch_cli::output`, used by `patch` and `rebuild` and tested directly.
  `patch` output is now also read back before it is renamed into place. The overwrite refusal
  names the fix that applies to each command: `--force` for `patch`, a new path for `rebuild`.
- `patch-core` builds with `opt-level = 1` in the dev/test profile, so codec tests on 64 MiB
  inputs stay fast. Debug assertions and overflow checks remain enabled.
- Local Python validation instructions now use the same entrypoint as CI (`scripts/run_python_quality.sh`).
- `patch-cli patch` now loads and validates recipe manifests, enforces firmware identity compatibility
  (size + SHA-256), and applies deterministic `write_span` / `owner_copy_window` operations.
- Rust tests now live in dedicated `tests/` files per crate instead of inline `#[cfg(test)]` blocks
  in production source files.
- `patch-cli patch` now hashes in-memory input bytes (single-read flow), rejects overlapping destination
  ranges, verifies that all byte mutations stay within declared destination regions, and reports
  output SHA-256.
- `patch-cli patch` reads input through `read_regular_file`. A FIFO or device input is now
  refused instead of blocking or being read without limit. The input is hashed once, by the
  engine.
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
- **Breaking:** `patch_schema::Replacement::bytes`; use `Replacement::pattern` (byte by byte, with
  kept bytes as `None`) or `Replacement::written_runs`.
- Redundant `rustfmt.toml` and `clippy.toml` files (settings matched tool defaults or duplicated
  existing workspace metadata).

### Safety
- Project is unofficial and experimental; use at your own risk.
