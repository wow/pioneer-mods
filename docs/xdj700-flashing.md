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
for a unit on v1.15 therefore needs a different label. The project uses **`Ver1.90`**, which is
clearly unofficial and far above any likely official release.

Whether the updater accepts a *lower* version is not yet known. Official files older than
v1.15 are no longer downloadable. Until that is known, plan for a unit that has run a `Ver1.90`
file to keep showing `1.90`. Two consequences:
- re-flashing official v1.15 over it may be refused;
- a future official update numbered below 1.90 may be refused too.

## 2. Stages

Go one stage at a time. Do not move on until the previous stage has passed.

| Stage | File | Purpose |
| --- | --- | --- |
| 0 | The **official** v1.15 update, if the unit runs an older version | Bring the unit to v1.15 and rehearse the procedure |
| 1 | The **no-op rebuild** labelled `Ver1.90` | The first rebuilt file. Its application is the official one, re-compressed, so nothing should change functionally |
| 1b | The official v1.15 update, over stage 1 | Shows whether the updater accepts a lower version. Refused or skipped: harmless; the unit keeps running the stock application |

The stage-1 file built from the official v1.15 file (`XDJ700.UPD`, SHA-256 `73edec98…f99c`) is
always the same:

| Property | Value |
| --- | --- |
| Size | 17,368,545 bytes |
| SHA-256 | `aff3a1b9f887dc6d6e35f5686d0edfa644ce9e661011775489315ddbcf928f99` |

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
     --application stock --label Ver1.90 --output ~/xdj700-stage1/XDJ700.UPD
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
   way.
6. **Follow the official update procedure:**
   - power off with nothing connected;
   - hold **IN** and **RELOOP/EXIT** while powering on, and release them when the unit asks for
     the USB device;
   - insert the stick into the top USB port.

   The update takes about 3 minutes. Photograph each screen.

## 4. After the update

Check each of these:

- the update completes with no error;
- the UTILITY screen shows the expected version (hold MENU/UTILITY for over a second);
- three cold boots reach the normal screen;
- browsing, playback, cue and loop behave as before.

## 5. Recovery: what protects the unit, and what does not

From static analysis of the v1.15 loader:

- **The loader checks the application before running it.** It verifies the application
  section's checksum before starting it.
- **A bad checksum starts the fallback updater.** If the checksum does not match, for example
  after an interrupted update, the loader runs a separate fallback updater stored in the loader
  region. Rebuilt files never change that region, so this protection stays in place.
- **No button combination reaches the fallback.** The IN + RELOOP/EXIT update mode belongs to
  the application itself.
- **The unprotected case:** an application whose checksum is valid but which crashes or hangs
  before its update mode starts cannot be recovered by software. The stage-1 no-op file cannot
  cause this, because the device runs exactly the stock application. Files that change behaviour
  can, so every future modification must stay out of the code that runs early during start-up.

## 6. If something goes wrong

**Stop at the first unexpected screen and do not retry blindly.** Write down:
- the stage;
- the file's SHA-256;
- the step;
- what you expected and what you saw.

Then try the official v1.15 update with the official procedure. If the unit no longer starts,
leave it powered off and open an issue with your notes.
