# Flashing a rebuilt XDJ-700 update (owner guide)

> **Alpha software. You flash at your own risk.** A rebuilt update is not an official release.
> It has not been confirmed that the update mode can still recover the unit if a rebuilt
> application fails to start. Never use a unit running a rebuilt update for a live performance.

This guide covers the files `patch-cli rebuild` writes. Read it completely before you flash
anything.

## 1. Stages

Go one stage at a time. Do not move on until the previous stage has passed.

| Stage | File | Purpose |
| --- | --- | --- |
| 0a | The **official** v1.15 update, if the unit runs an older version | Bring the unit to v1.15 and rehearse the procedure. This is also your recovery file |
| 0b | The official v1.15 update again, over v1.15 | Shows how the updater treats the version that is already installed |
| 0c | An **official** older update (for example v1.14), over v1.15 | Shows whether the updater accepts a lower version. Only if you have the official file |
| 1 | The **no-op rebuild**: `rebuild --application stock --label Ver1.15`, flashed over the older official version from stage 0c | The first rebuilt file. Its application is the official one, re-compressed. Nothing should change functionally |

**Observed on an owner's unit (2026-10-08):** the updater compares each document's version with
the installed one. When they are equal, it skips that document and still reports "Firmware
update is complete":
- **0a:** PANEL `Ver1.00 -> Ver1.00` showed 100% at once while MAIN was still updating;
- **0b:** both documents completed at once.

**So a rebuilt file labelled with the installed version is skipped, not flashed.** That is why
stage 1 runs from an older official version. Do not invent other labels without a plan: to
return from a higher, unofficial label to official firmware, the updater must accept a lower
version, which stage 0c tests.

The no-op rebuild of the official v1.15 file (`XDJ700.UPD`, SHA-256 `73edec98…f99c`) with
`--label Ver1.15` is always the same file:

| Property | Value |
| --- | --- |
| Size | 17,368,545 bytes |
| SHA-256 | `f2dd19d47b8253fbea189009166f958b2d9f29a0bb8a5d7d258f98144134d06c` |


## 2. Before every flash

1. **Power:** mains power. Never power off or pull the USB stick during an update.
2. **Write the file to a local disk first**, not straight to the stick. From the repository
   root (the output directory must exist):

   ```bash
   mkdir -p ~/xdj700-stage1
   cargo run --release -p patch-cli -- rebuild --input /path/to/XDJ700.UPD \
     --application stock --label Ver1.15 --output ~/xdj700-stage1/XDJ700.UPD
   ```

   The command checks the rebuild against the official file, writes atomically, reads the file
   back, and prints `output_sha256_hex`. It never overwrites an existing file.
3. **USB stick:**
   - use a stick formatted **FAT32**. On macOS the writer refuses exFAT; copying a finished file
     onto a FAT32 stick is the supported path everywhere;
   - copy only `XDJ700.UPD` to its root, under the name the official update instructions give;
   - on macOS, also delete the hidden `._XDJ700.UPD` file that macOS creates (for example with
     `dot_clean -m /Volumes/<stick>`). Whether the updater ignores it is not known.
4. **Check the stick, not the copy source:**
   - eject the stick and plug it back in;
   - run `shasum -a 256 /Volumes/<stick>/XDJ700.UPD`;
   - it must equal the identity in section 1. **On any mismatch, stop.** The read-back done by
     `patch-cli` can be served from the operating system's cache, so it does not prove what is on
     the stick.
5. **Recovery stick ready:** keep a second stick with the official v1.15 file, checked the same
   way.
6. **Follow the official update procedure** shipped with v1.15, step by step. Photograph each
   screen.

## 3. After the update

Check each of these:

- the update completes with no error;
- the version screen shows the expected version;
- three cold boots reach the normal screen;
- browsing, playback, cue and loop behave as before.

## 4. If something goes wrong

**Stop at the first unexpected screen and do not retry blindly.** Write down:
- the stage;
- the file's SHA-256;
- the step;
- what you expected and what you saw.

Then re-flash the official v1.15 update with the official procedure.
