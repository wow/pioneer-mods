# Flashing a rebuilt XDJ-700 update (owner guide)

> **Alpha software. You flash at your own risk.** A rebuilt update is not an official release.
> Never use a unit running a rebuilt update for a live performance. Read section 5 on recovery
> before you flash anything.

This guide covers the files `patch-cli rebuild` writes. Read it completely before you flash
anything.

## 1. How the updater treats versions

**Observed on an owner's unit (2026-10-08): the updater writes a document only when the file's
version is higher than the installed one.** Equal and lower versions are skipped without an
error, and the unit still reports "Firmware update is complete":

| File version vs installed | Result | Observed |
| --- | --- | --- |
| higher (`Ver1.14 -> Ver1.15`) | written, MAIN progresses | official v1.15 over v1.14 |
| equal (`Ver1.15 -> Ver1.15`, PANEL `Ver1.00 -> Ver1.00`) | skipped at once | official v1.15 over v1.15; PANEL in every update |
| lower (`Ver1.15 -> Ver0.90`) | skipped at once | the project's stage-1 file over v1.15 |

**So a rebuild for a unit on v1.15 needs a higher label.** The project uses the smallest one,
**`Ver1.16`**. If the unit remembers the label of the last file (see below), any official
release numbered at or below the label is skipped from then on. With `Ver1.16` that is only a
future official 1.16 (1.17 and later still install). A larger label such as `Ver1.90` would
block every release up to 1.90. The version field holds exactly seven characters (`VerX.YY`), so
no label sits between 1.15 and 1.16. Official files older than v1.15 are no longer
downloadable.

**Where the "installed" version probably comes from.** The application carries its own version
block (model, `1.15`, build date), and the update screens are formatted from a version string.
So the installed version the updater compares against, and the version on the UTILITY screen,
**probably come from the running application, not from the label of the last file flashed**.
This is not yet confirmed on hardware.

If it holds:
- after a no-op rebuild (stock application) the unit still reports `1.15`, whatever the label;
- the label then does not stick, and future official releases install normally. An official
  v1.15 file flashed later shows `Ver1.15 -> Ver1.15` and is skipped, which is harmless because
  the application is already the official one.

If it does not hold, the unit reports the label instead (`Ver1.16`), and a future official 1.16
would be skipped. In that case the project would first need to add support for that release
(new pins and a verified layout) before it could rebuild it, contents
unchanged, under a higher label.

## 2. Stages

Go one stage at a time. Do not move on until the previous stage has passed, except where the
table says otherwise. Photograph the **MAIN line** of every update (section 4).

| Stage | File | What to look for | Next |
| --- | --- | --- | --- |
| 0 | The **official** v1.15 update, if the unit runs an older version | MAIN progresses to v1.15 | Stage 1b |
| 1 | *(done: the probe that established the policy)* No-op rebuild labelled `Ver0.90`, lower than the installed version | Skipped at once, as the policy predicts; nothing was written | — |
| 1b | **No-op rebuild labelled `Ver1.16`** (higher), after reading the note below | **Real flash:** MAIN shows `Ver1.15 -> Ver1.16` and progresses for about 3 minutes; the stock application is installed. **Skipped:** MAIN jumps to 100%: stop and report it. **An error at any point:** stop and follow section 6 | Stage 2 |
| 2 | The **official** v1.15 update again | The MAIN line's **left-hand** version. **`Ver1.15 -> Ver1.15` and a skip:** the unit reports the application's own version, the label does not stick, and future official releases install normally (expected outcome, not a failure). **`Ver1.16 -> Ver1.15` and a skip:** the unit stores the label, so a future official 1.16 would be skipped (section 1). In both cases nothing is written | Done: the unit runs the official application in every case |

A no-op rebuild's application is the official one, re-compressed, so nothing should change
functionally at any stage.

> **Note before stage 1b.** Because lower versions are skipped, a unit that has run stage 1b may
> report `1.16` from then on if it remembers labels (stage 2 tells you). The consequences:
> - the official v1.15 update is skipped;
> - a future official 1.16 is skipped;
> - recovering from a later modified application needs a stock no-op rebuild with a label
>   higher than the version the unit reports.
>
> The application stays the official one either way, so the unit keeps working normally.
> Stage 1b is your decision.

The stage files built from the official v1.15 file (`XDJ700.UPD`, 17,371,335 bytes, SHA-256
`73edec9802da51672257c2599efc04209dc92478fcbaa1a0425b3b122e33f99c`) are always the same. All are
17,368,545 bytes:

| Stage | Label | SHA-256 |
| --- | --- | --- |
| 1 | `Ver0.90` | `79f25fa1be84e0e5323273eb36ca5cbfd0f532824f6a2fde0a80db6965380252` |
| 1b | `Ver1.16` | `9e1ac10e09c701cb6863b8667131e03452156a0bd7702823bc5f0502a88b6a08` |
| recovery | `Ver1.17` | `2d0a4a09a90494c8af26fd585ec5bc1b058b2d731f9d5d72d8ed03fafc4a758d` |

(With `--label Ver1.15` the SHA-256 is
`f2dd19d47b8253fbea189009166f958b2d9f29a0bb8a5d7d258f98144134d06c`, but on a v1.15 unit that
file is skipped.)

## 3. Before every flash

1. **Power:** mains power. Never power off or pull the USB stick during an update.
2. **Write the file to a local disk first**, not straight to the stick. From the repository
   root (the output directory must exist):

   ```bash
   # Stage 1b (after the note in section 2)
   mkdir -p ~/xdj700-stage1b
   cargo run --release -p patch-cli -- rebuild --input /path/to/XDJ700.UPD \
     --application stock --label Ver1.16 --output ~/xdj700-stage1b/XDJ700.UPD
   ```

   The command checks the rebuild against the official file, writes atomically, reads the file
   back, and prints `output_sha256_hex`. It never overwrites an existing file.
3. **USB stick:**
   - use a stick formatted **FAT32**. On macOS the writer refuses exFAT; copying a finished file
     onto a FAT32 stick is the supported path everywhere;
   - copy only `XDJ700.UPD` to its root, under exactly that name;
   - on macOS, also delete the hidden `._XDJ700.UPD` file that macOS creates (for example with
     `dot_clean -m /Volumes/<stick>`). Whether the updater ignores it is not known.
4. **Check the stick, not the copy source:**
   - eject the stick and plug it back in;
   - run `shasum -a 256 /Volumes/<stick>/XDJ700.UPD`;
   - it must equal the identity **for the stage you are flashing** in section 2 (stage 1b:
     `9e1ac10e…`). **On any mismatch, stop.** The read-back done by
     `patch-cli` can be served from the operating system's cache, so it does not prove what is on
     the stick.
5. **Recovery sticks ready**, each checked the same way (re-insert, then `shasum`):
   - **Official v1.15 file.** In normal update mode it is skipped on any unit that reports 1.15 or
     higher, so it only helps where no version is compared, for example possibly the fallback
     updater (section 5).
   - **Recovery no-op rebuild labelled `Ver1.17`**: the official application, re-compressed, under
     a label above anything installed. Its identity is in section 2. Build it with the same
     command as stage 1b, using `--label Ver1.17` and its own output directory. Trade-off: if the
     unit remembers labels, using this stick means a future official 1.17 would be skipped.
     That is acceptable for a recovery. This route is untested.
6. **Follow the official update procedure:**
   - power off with nothing connected;
   - hold **IN** and **RELOOP/EXIT** while powering on, and release them when the unit asks for
     the USB device;
   - insert the stick into the top USB port.

   The update takes about 3 minutes. Photograph each screen.

## 4. After the update

**First, tell a real flash from a skip.** A completed update and a skipped one end on the same
"Firmware update is complete" screen:
- **Real flash:** the MAIN line shows the installed version, then the file's label (for example
  `MAIN Ver1.15 -> Ver1.16`), and its progress bar runs for about 3 minutes.
- **Skip:** the MAIN line jumps straight to 100%.

Stages 0 and 1b only pass if MAIN really progressed. For stage 2, a skip is an expected,
informative outcome (section 2). **Photograph the MAIN line**: its left-hand version tells us
where the installed version comes from.

Then check each of these:
- the update completed with no error message;
- the UTILITY screen (hold MENU/UTILITY for over a second) shows a version; write it down. After a
  no-op rebuild it probably still shows `1.15` (see section 1). That is expected and not a
  failure;
- three cold boots reach the normal screen;
- browsing, playback, cue and loop behave as before.

## 5. Recovery: what protects the unit, and what does not

**Everything in this section comes from static analysis of the v1.15 loader. None of it has been
observed on hardware.**

- **The loader checks the application before running it.** It verifies the application
  section's checksum before starting it.
- **A bad checksum starts the fallback updater.** When the application section is left with a
  bad checksum (for example by an update interrupted while the application was being written),
  the loader starts a separate fallback updater stored in the loader region. Rebuilt files keep
  that region byte-identical. Nobody has seen the fallback updater run, so its screens and the
  stick it expects are unknown. If the unit powers on into an unfamiliar update or USB prompt,
  try the official v1.15 stick first. If its MAIN line jumps to 100%, try the `Ver1.17`
  recovery stick. Untested.
- **Interruptions in the loader region are not covered.** The update also writes the loader
  region; it is part of the file, rewritten with identical bytes. An interruption there is not
  covered by the fallback, for official files too. So **never interrupt an update**.
- **No button combination reaches the fallback.** The IN + RELOOP/EXIT update mode belongs to
  the application itself.
- **The unprotected case:** an application whose checksum is valid but which crashes or hangs
  before its update mode starts cannot be recovered by software.
  - Stage 1b is the **first real write** of a rebuilt file (stage 1 was skipped). The stage-1b
    no-op file is **not expected** to cause this: offline checks show the device
    will run exactly the stock application.
  - What remains untested is how the updater handles the rebuilt file's layout. A reference
    build with the same layout installed successfully on hardware.
  - Files that change behaviour can cause it, so every future modification must stay out of the
    code that runs early during start-up.

## 6. If something goes wrong

**Stop at the first unexpected screen and do not retry blindly.** Write down:
- the stage;
- the file's SHA-256;
- the step;
- what you expected and what you saw.

Then use a recovery stick (section 3, step 5), and **check whether MAIN really progressed**
(section 4).
- **In normal update mode** the official v1.15 file is skipped on a unit that reports 1.15 or
  higher. That is harmless after a no-op rebuild, whose application is already the official one,
  but it does **not** restore a modified application.
- **To actually write the official application**, use the `Ver1.17` recovery stick, or any stock
  no-op rebuild labelled higher than the version the unit reports. That route is untested.
If the unit no longer starts, leave it powered off and open an issue with your notes.
