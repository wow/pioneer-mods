# Flashing a rebuilt XDJ-700 update (owner guide)

> **Alpha software. You flash at your own risk.** A rebuilt update is not an official release.
> Never use a unit running a rebuilt update for a live performance. Read section 5 on recovery
> before you flash anything.

This guide covers the files `patch-cli rebuild` writes. Read it completely before you flash
anything.

## 1. How the updater treats versions

Observed on an owner's unit (2026-10-08): the updater compares each document's version with the
installed one. When they are equal, it skips that document and still reports "Firmware update
is complete". When the official v1.15 file was flashed over v1.14, PANEL `Ver1.00 -> Ver1.00`
showed 100% at once while MAIN was still updating. Flashed again over v1.15, both documents
completed immediately.

**So a rebuilt file labelled with the installed version is skipped, not flashed.** A rebuild
for a unit on v1.15 therefore needs a different label. The project first tries a **lower**,
clearly unofficial label, **`Ver0.90`**. If the updater accepts it, the step is reversible
whatever the updater does afterwards. Only if it is refused does the project use a higher one,
**`Ver1.90`**, which is clearly unofficial and far above any likely official release.

Whether the updater accepts a *lower* version is not yet known. Official files older than
v1.15 are no longer downloadable.

**Where the "installed" version probably comes from.** The application carries its own version
block (model, `1.15`, build date), and the update screens are formatted from a version string.
So the installed version the updater compares against, and the version on the UTILITY screen,
**probably come from the running application, not from the label of the last file flashed**.
This is not yet confirmed on hardware.

If it holds:
- after a no-op rebuild (stock application) the unit still reports `1.15`, whatever the label;
- the label then does not stick, and an official v1.15 file flashed later shows
  `Ver1.15 -> Ver1.15` and is skipped. That is harmless, because the application is already the
  official one.

## 2. Stages

Go one stage at a time. Do not move on until the previous stage has passed. Photograph the
**MAIN line** of every update (section 4).

| Stage | File | What to look for | Next |
| --- | --- | --- | --- |
| 0 | The **official** v1.15 update, if the unit runs an older version | MAIN progresses to v1.15 | Stage 1 |
| 1 | **No-op rebuild labelled `Ver0.90`** (lower than the installed version) | **Accepted:** MAIN shows `… -> Ver0.90` and progresses for about 3 minutes; the stock application is installed. **Refused or skipped:** an error, or MAIN jumps to 100%; nothing is written | Accepted: stage 2. Refused or skipped: stage 1b |
| 1b | **No-op rebuild labelled `Ver1.90`** (higher), only if stage 1 was refused or skipped | MAIN shows `… -> Ver1.90` and progresses | Stage 2 |
| 2 | The **official** v1.15 update again | The MAIN line's **left-hand** version: `Ver1.15 -> Ver1.15` (and a skip) means the unit reports the application's own version. `Ver0.90 -> Ver1.15` or `Ver1.90 -> Ver1.15` means it stores the last file's label | Done: the unit runs the official application either way |

A no-op rebuild's application is the official one, re-compressed, so nothing should change
functionally at any stage. After stage 1 accepted, any official file is "higher", so the unit can
always go back to official firmware. After stage 1b, see section 1.

The stage files built from the official v1.15 file (`XDJ700.UPD`, SHA-256 `73edec98…f99c`) are
always the same. Both are 17,368,545 bytes:

| Label | SHA-256 |
| --- | --- |
| `Ver0.90` (stage 1) | `79f25fa1be84e0e5323273eb36ca5cbfd0f532824f6a2fde0a80db6965380252` |
| `Ver1.90` (stage 1b) | `aff3a1b9f887dc6d6e35f5686d0edfa644ce9e661011775489315ddbcf928f99` |

(With `--label Ver1.15` the SHA-256 is
`f2dd19d47b8253fbea189009166f958b2d9f29a0bb8a5d7d258f98144134d06c`, but on a v1.15 unit that
file is skipped.)

## 3. Before every flash

1. **Power:** mains power. Never power off or pull the USB stick during an update.
2. **Write the file to a local disk first**, not straight to the stick. From the repository
   root (the output directory must exist):

   ```bash
   mkdir -p ~/xdj700-stage1
   cargo run --release -p patch-cli -- rebuild --input /path/to/XDJ700.UPD \
     --application stock --label Ver0.90 --output ~/xdj700-stage1/XDJ700.UPD
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
   - it must equal the identity in section 2. **On any mismatch, stop.** The read-back done by
     `patch-cli` can be served from the operating system's cache, so it does not prove what is on
     the stick.
5. **Recovery stick ready:** keep a second stick with the official v1.15 file, checked the same
   way. After a rebuild it may be skipped rather than flashed (see section 6).
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
  `MAIN Ver1.15 -> Ver0.90`), and its progress bar runs for about 3 minutes.
- **Skip:** the MAIN line jumps straight to 100%.

A stage only passes if MAIN really progressed. **Photograph the MAIN line**: its left-hand
version tells us where the installed version comes from.

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
  try the official v1.15 stick.
- **Interruptions in the loader region are not covered.** The update also writes the loader
  region; it is part of the file, rewritten with identical bytes. An interruption there is not
  covered by the fallback, for official files too. So **never interrupt an update**.
- **No button combination reaches the fallback.** The IN + RELOOP/EXIT update mode belongs to
  the application itself.
- **The unprotected case:** an application whose checksum is valid but which crashes or hangs
  before its update mode starts cannot be recovered by software.
  - The stage-1 no-op file is **not expected** to cause this: offline checks show the device
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

Then try the official v1.15 update with the official procedure. **Check whether MAIN really
progressed** (section 4). After a rebuild the updater may skip the official file as
`Ver1.15 -> Ver1.15`. That is harmless after a no-op rebuild, whose application is already the
official one, but it does **not** restore a modified application. Restoring one needs a stock
no-op rebuild under a label different from the version the unit reports. That route is untested.
If the unit no longer starts, leave it powered off and open an issue with your notes.
