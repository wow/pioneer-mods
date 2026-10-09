# Flashing a rebuilt XDJ-700 update (owner guide)

> **Alpha software. You flash at your own risk.** A rebuilt update is not an official release.
> Never use a unit running a rebuilt update for a live performance. Read section 5 on recovery
> before you flash anything.

This guide covers the files `patch-cli rebuild` writes, and those `patch-cli patch` writes with a
schema-v2 recipe. Schema-v1 output (raw byte spans) is not an installable update: never flash it.
Read this guide completely before you flash anything.

## 1. How the updater treats versions

**Observed on an owner's unit (2026-10-08): the updater writes a document only when the file's
version is higher than the installed one.** Equal and lower versions are skipped without an
error, and the unit still reports "Firmware update is complete":

| File version vs installed | Result | Observed |
| --- | --- | --- |
| higher (`Ver1.14 -> Ver1.15`) | written, MAIN progresses | official v1.15 over v1.14 |
| higher (`Ver1.15 -> Ver1.16`) | accepted, MAIN progresses (about 3 minutes, as in a real write) | the project's stage-1b no-op rebuild over v1.15 |
| higher (`Ver1.15 -> Ver1.16`) | **written** (about 3 minutes; UTILITY then shows the new application's `0.10`) | the project's stage-3 file over v1.15 |
| higher (`Ver0.10 -> Ver1.15`) | **written** (about 3 minutes; UTILITY back to `1.15`) | official v1.15 over the stage-3 application |
| higher (`Ver1.15 -> Ver1.16`) | **written** (UTILITY then shows `0.11`; BEAT LOOP 16 loops 32 beats) | the project's stage-5 file over v1.15 (2026-10-09) |
| higher (`Ver0.11 -> Ver1.15`) | **written** (UTILITY back to `1.15`) | official v1.15 over the stage-5 application (2026-10-09) |
| equal (`Ver1.15 -> Ver1.15`, PANEL `Ver1.00 -> Ver1.00`) | skipped at once | official v1.15 over v1.15; PANEL in every update |
| lower (`Ver1.15 -> Ver0.90`) | skipped at once | the project's stage-1 file over v1.15 |

**Where the installed version comes from (observed 2026-10-08):** not from the label of the last
file. After the project's `Ver1.16` file was flashed with MAIN progressing for about 3 minutes, as
in a real write (stage 1b):
- the boot screen and the UTILITY screen still showed `Ver 1.15`;
- the official v1.15 file flashed next showed `MAIN Ver1.15 -> Ver1.15` and was skipped.

So **labels do not stick**. **The unit reports the application's own version string** (decoded
offset `0x740`, `1.15` in the official application). Observed on the same unit (stages 3 and 4):
after the project's stage-3 file changed only that string to `0.10`, UTILITY showed `0.10`, and
the next official update showed `MAIN Ver0.10 -> Ver1.15`. (The boot screen was not reported at
stage 3; at stage 1b it matched UTILITY.)

What this means:
- **A rebuild for a unit on v1.15 needs a label higher than `Ver1.15`.** The project uses
  **`Ver1.16`**, the smallest one. The version field holds exactly seven characters (`VerX.YY`),
  so no label sits between 1.15 and 1.16. `patch-cli rebuild` warns about labels that are not
  higher.
- **Future official releases are expected to install normally** after a no-op rebuild, because
  the unit still reports `1.15`, not the label. No release above 1.15 exists yet to confirm it.
- **In normal update mode the official v1.15 file never restores the official application on a
  v1.15 unit:** it is skipped (equal version). To write the official application, flash the
  stock no-op rebuild labelled `Ver1.16` (section 3, step 5). That is tested over the official
  application only.
- **A modified application reports a version lower than 1.15**: `patch-cli rebuild
  --report-version` (for example `0.10`) or a schema-v2 recipe's `reported_version` with `patch`
  (stage 5: `0.11`). Both refuse 1.15 or higher. The unit then reports that version, so **the
  official v1.15 file is written over it and restores the stock application with the vendor's
  own file**. *(Observed on an owner's unit: stages 3 and 4, 2026-10-08, `Ver0.10 -> Ver1.15`
  progressed for about 3 minutes and UTILITY returned to `1.15`; stages 5 and 6, 2026-10-09, the
  unit went from `0.11` back to `1.15`.)* This was observed for an application changed in its
  version string, and in its version string and one table entry (stage 5). An application whose
  code is changed also needs its own update mode to keep working (section 5).
- Official files older than v1.15 are no longer downloadable.

## 2. Stages

Go one stage at a time. Do not move on until the previous stage has passed, except where the
table says otherwise. Photograph the **MAIN line** of every update (section 4).

| Stage | File | What to look for | Next |
| --- | --- | --- | --- |
| 0 | The **official** v1.15 update, if the unit runs an older version | MAIN progresses to v1.15 | Stage 1b |
| 1 | *(done: the probe that established the policy)* No-op rebuild labelled `Ver0.90`, lower than the installed version | Skipped at once, as the policy predicts; nothing was written | — |
| 1b | **No-op rebuild labelled `Ver1.16`** (higher) | **Real flash:** MAIN shows `Ver1.15 -> Ver1.16` and progresses for about 3 minutes, as in a real write; the unit still reports `1.15`. *(Passed on an owner's unit, 2026-10-08: about 3 minutes, normal cold boots, browsing, playback, cue and loop unchanged.)* **Skipped:** MAIN jumps to 100%: stop and report it. **An error at any point:** stop and follow section 6 | Stage 2 |
| 2 | The **official** v1.15 update again (optional: it only confirms section 1) | `MAIN Ver1.15 -> Ver1.15` and a skip, so the label did not stick. *(Observed on an owner's unit, 2026-10-08.)* Nothing is written | Stage 3 |
| 3 | **Stock application reporting `0.10`, labelled `Ver1.16`** (built **with** `--report-version 0.10`; the recovery stick is built without it): only the application's 4-byte version string differs from stock. This is the first flashed application that differs from stock. A static scan finds one pointer to the string, in a small table with the model string, and no code literal that points to it directly (computed addresses are not ruled out); the reference implementation changed the same string and its build booted on hardware | *(Passed on an owner's unit, 2026-10-08: about 3 minutes; UTILITY showed `0.10`; the owner's checks of the unit were all good. Whether Pro DJ Link or rekordbox was exercised was not stated.)* **Real flash:** MAIN shows `Ver1.15 -> Ver1.16` and progresses for about 3 minutes. Afterwards the boot screen and UTILITY show **`0.10`**; the unit otherwise behaves as stock (cold boots, browsing, playback, cue, loop, and Pro DJ Link or rekordbox if you use them: the version may be announced there). **Skipped**, or UTILITY still shows `1.15`: stop and report it (the unit then runs an application equivalent to stock; the `Ver1.16` stick restores it exactly). **An error:** section 6 | Stage 4 |
| 4 | The **official** v1.15 update | *(Passed on an owner's unit, 2026-10-08: `MAIN Ver0.10 -> Ver1.15` progressed for about 3 minutes and UTILITY returned to `1.15`.)* **Real flash:** `MAIN Ver0.10 -> Ver1.15` progressing for about 3 minutes, and UTILITY shows `1.15` again: the vendor file restores stock. **If it is skipped** (`MAIN Ver0.10 -> Ver1.15` at 100% at once, UTILITY still `0.10`): use the `Ver1.16` stock no-op stick, which is higher than `0.10`, and report it | Done: the unit runs the official application. Stage 5 is an optional experiment |
| 5 | **Beat-loop experiment, reporting `0.11`, labelled `Ver1.16`** (`patch` with `recipes/xdj700-v1.15/beat-loop-16-plays-32.json`). **The first file that changes behaviour.** Besides the version string, one table entry differs from stock: the BEAT LOOP button labelled 16 selects the player's existing 32-beat length instead of 16. By static analysis only the PERFORM screen's BEAT LOOP touch handler reads that table, so the change acts when the button is touched, not during start-up | *(Passed on an owner's unit, 2026-10-09: the owner reported that all steps were tested and worked as expected, and that BEAT LOOP 16 looped 32 beats. Which storage options were tried, and how long the update ran, were not stated.)* **Real flash:** `MAIN Ver1.15 -> Ver1.16` progressing for about 3 minutes; UTILITY shows **`0.11`**; three cold boots reach the normal screen. **Skipped**, or UTILITY still shows `1.15`: stop and report it. The experiment was not installed, so the checks below would say nothing about it. Then, with an analysed track (beat grid) loaded, touch BEAT LOOP **16**: the loop should span **32 beats** (8 bars: count bars on the waveform or the beat display). Touch 1/2, 1, 2, 4 and 8: each unchanged. Exit and reloop, and loop with QUANTIZE on and off: unchanged. Also try the cases most likely to differ from stock: a **slow track** (about 70 BPM: 32 beats last about 27 seconds, twice the longest loop this button gave before); **storing the 32-beat loop** wherever the unit can (REC to a hot cue, a memory) and recalling it; **loading another track** while the loop plays; and **three more cold boots after** these checks, in case loop state is saved. **16 still gives 16 beats** (after a real flash): the player limits this path; report it. **Anything odd** (a freeze, a wrong length, a display glitch): note it and go to stage 6. **An error:** section 6 | Stage 6 |
| 6 | The **official** v1.15 update | *(Passed on an owner's unit, 2026-10-09: with the official v1.15 file, `MAIN Ver0.11 -> Ver1.15` progressed and the unit went from `0.11` back to `1.15`.)* **Real flash:** `MAIN Ver0.11 -> Ver1.15` progressing for about 3 minutes, and UTILITY shows `1.15` again. **If it is skipped:** use the `Ver1.16` stock no-op stick, which is higher than `0.11`, and report it | Done |

A no-op rebuild's application is the official one, re-compressed, so nothing should change
functionally in it. Stage 3 changes only the text of the application's version string. Stage 5
also changes one table entry, the first change to behaviour. Since 2026-10-09 a stage file is
also rehearsed in emulation before it is offered (section 5, rules for every modification).

The stage files built from the official v1.15 file (`XDJ700.UPD`, 17,371,335 bytes, SHA-256
`73edec9802da51672257c2599efc04209dc92478fcbaa1a0425b3b122e33f99c`) are always the same. All are
17,368,545 bytes:

| Stage | Label | SHA-256 |
| --- | --- | --- |
| 1 | `Ver0.90` | `79f25fa1be84e0e5323273eb36ca5cbfd0f532824f6a2fde0a80db6965380252` |
| 1b | `Ver1.16` | `9e1ac10e09c701cb6863b8667131e03452156a0bd7702823bc5f0502a88b6a08` |
| (spare) | `Ver1.17` | `2d0a4a09a90494c8af26fd585ec5bc1b058b2d731f9d5d72d8ed03fafc4a758d` |

The `Ver1.17` file was prepared in case labels stuck. They do not (section 1), so the stage-1b
file is also the recovery file, and `Ver1.17` is not needed.

(The no-op rebuild with `--label Ver1.15` has SHA-256
`f2dd19d47b8253fbea189009166f958b2d9f29a0bb8a5d7d258f98144134d06c`, but on a v1.15 unit that
file is skipped.)

The stage-3 file (`--label Ver1.16 --report-version 0.10`) is 17,368,543 bytes with SHA-256
`84cbd2637b167893c6ad3ff8bc4a0b4cfb7cf5984dc399f6b018a8b5ddb0be5c`; its application has SHA-256
`74afbf4409242f58f11caac5142ccc220d39199f28c50155d0a2af9ed42d5ad3`. All of these files are
cross-checked byte-identical against the reference serializer.

The stage-5 file (`patch` with `recipes/xdj700-v1.15/beat-loop-16-plays-32.json`) is 17,368,527
bytes with SHA-256 `144f4b557127a7eec2d5d2f5fadee0e6585f05a83af321097876a0edca345e12`; its
application has SHA-256 `dd5adad4ae531db95c9dae10d9fa534afc4e31d2ffd54daadc22da6e463f25db` and
differs from stock in exactly three bytes (two in the version string, one table entry), as the
owner-input tests check.

## 3. Before every flash

1. **Power:** mains power. Never power off or pull the USB stick during an update.
2. **Write the file to a local disk first**, not straight to the stick. From the repository
   root (the output directory must exist):

   ```bash
   # Stage 1b, and the recovery stick
   mkdir -p ~/xdj700-stage1b
   cargo run --release -p patch-cli -- rebuild --input /path/to/XDJ700.UPD \
     --application stock --label Ver1.16 --output ~/xdj700-stage1b/XDJ700.UPD

   # Stage 3: the stock application reporting 0.10
   mkdir -p ~/xdj700-stage3
   cargo run --release -p patch-cli -- rebuild --input /path/to/XDJ700.UPD \
     --application stock --label Ver1.16 --report-version 0.10 \
     --output ~/xdj700-stage3/XDJ700.UPD

   # Stage 5: the beat-loop experiment (reports 0.11)
   mkdir -p ~/xdj700-stage5
   cargo run --release -p patch-cli -- patch --input /path/to/XDJ700.UPD \
     --recipe recipes/xdj700-v1.15/beat-loop-16-plays-32.json \
     --output ~/xdj700-stage5/XDJ700.UPD
   ```

   Each command checks the result against the official file, writes atomically, reads the file
   back, and prints `output_sha256_hex`. None overwrites an existing file.
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
     `9e1ac10e…`; stage 5: `144f4b55…`). **On any mismatch, stop.** The read-back done by
     `patch-cli` can be served from the operating system's cache, so it does not prove what is on
     the stick.
5. **Recovery sticks ready**, each checked the same way (re-insert, then `shasum`):
   - **Stock no-op rebuild labelled `Ver1.16`**, built **without** `--report-version` (the
     stage-1b file, `9e1ac10e…`): the official application, re-compressed. On a unit that
     reports 1.15 it is accepted in normal update mode (stage 1b, over the official
     application). It is higher than any version a modified application reports (lower than
     1.15), so it is the backup restore stick for those too (untested over a modified
     application: stages 4 and 6 did not need it). **Check that its SHA-256 is `9e1ac10e…`:**
     the stage-3 file (`84cbd263…`) and the stage-5 file (`144f4b55…`) have the same name and
     label, and the stage-5 file, flashed by mistake, progresses like a restore but reinstalls
     the experiment.
   - **Official v1.15 file: the first restore stick for a modified application.** On a unit that
     reports a lower version (for example `0.10` after stage 3) it is written and restores stock
     (observed, stage 4). On a unit that reports 1.15 it is skipped in normal update mode, so
     there it only helps where no version is compared, for example possibly the fallback updater
     (section 5).
   - `patch-cli inspect --structure --input <file>` shows the version a file's application
     reports (`reported_version=…`).
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

Stages 0, 1b, 3, 4, 5 and 6 only pass if MAIN really progressed. For stage 2, a skip is the expected
outcome (section 2). **Photograph the MAIN line**: its left-hand version is what the unit reports.
While the unit runs the stock application it should be `Ver1.15` (section 1): at stages 1b, 2, 3
and 5 (at stage 5, after stage 4 restored stock: `MAIN Ver1.15 -> Ver1.16`). At stage 4, after
stage 3, it should be `Ver0.10`, and at stage 6, after stage 5, `Ver0.11`. Report anything else.

Then check each of these:
- the update completed with no error message;
- the UTILITY screen (hold MENU/UTILITY for over a second) shows a version; write it down. After a
  no-op rebuild or stages 4 and 6 it shows `1.15` (section 1), after stage 3 `0.10`, after stage 5
  `0.11`. All are expected;
- three cold boots reach the normal screen;
- browsing, playback, cue and loop behave as before (after stage 5, except the BEAT LOOP 16
  button, which should give 32 beats).

## 5. Recovery: what protects the unit, and what does not

**Everything in this section comes from static analysis of the v1.15 loader, except where marked
observed.**

- **The loader checks the application before running it.** It verifies the application
  section's checksum before starting it.
- **A bad checksum starts the fallback updater.** When the application section is left with a
  bad checksum (for example by an update interrupted while the application was being written),
  the loader starts a separate fallback updater stored in the loader region. Rebuilt files keep
  that region byte-identical. Nobody has seen the fallback updater run, so its screens and the
  stick it expects are unknown. If the unit powers on into an unfamiliar update or USB prompt,
  try the official v1.15 stick first. If its MAIN line jumps to 100%, try the `Ver1.16`
  stock no-op stick. Untested in this situation.
- **Interruptions in the loader region are not covered.** The file carries loader-region
  records, identical to the installed bytes. Whether the updater writes them was unknown; in
  emulation it does not: a local QEMU model of the board (not part of this repository) boots the
  unit's flash through its own loader and runs the application's update mode with a stick. In
  four traced updates (an empty flash, stock to the `Ver1.16` no-op, stock to stage 5, stage 5
  to official v1.15) the updater never erased or wrote the loader region `0x000000`–`0x03FFFF`.
  Each update erased the 122 application sectors `0x040000`–`0x7DFFFF` and the 8 settings
  sectors `0x7E0000`–`0x7FFFFF`; a same-version file erased only the settings sectors, the skip
  section 1 describes. Not verified on hardware, so an interruption while the loader region is
  written is still treated as uncovered, for official files too. **Never interrupt an update.**
- **No button combination reaches the fallback.** The IN + RELOOP/EXIT update mode belongs to
  the application itself. In emulation its decision is one branch of the application (at
  `0x08D50D78`, in the function at `0x08D50D24`), taken after the kernel, its tasks and the
  panel link are running. Everything that runs before that branch is the code the next bullet
  is about.
- **The unprotected case:** an application whose checksum is valid but which crashes or hangs
  before its update mode starts cannot be recovered by software.
  - *Observed:* the stage-1b and stage-3 files did not cause this. On one owner's unit stage 1b
    was accepted and stage 3 was visibly written (UTILITY changed to `0.10`); the unit booted
    normally each time, and its update mode still worked (stage 4). Stage 3's application differs
    from stock only in its version string; a changed application yields a different compressed
    stream and needs its own test. Stage 5 was that test for a one-entry table change read, by
    static analysis, only by a touch handler: on an owner's unit (2026-10-09) it was written,
    the unit booted normally and behaved as expected, and its update mode still worked (stage 6,
    with the official file). An application whose code is changed is still untested.
  - Files that change behaviour can cause it, so every future modification must stay out of the
    code that runs before the update-mode decision.
- **Rules for every future modification:**
  - stay out of the code that runs before the update-mode decision, and out of the update path
    itself (above). In emulation that set is measured: the blocks a normal boot runs before the
    branch at `0x08D50D78`, plus every block a 30-second update-mode boot runs (1,359 address
    ranges, about 350 KiB of the application). It is a lower bound: the unit runs code the
    model does not (the real panel, storage, DSP replies). Every recipe window must lie outside
    it; a check in `patch-cli` is planned, until then it is checked by hand before a recipe is
    committed;
  - **rehearse the update in emulation before a stage file is offered.** The rehearsal flashes
    the current flash image with the file through the application's own update mode, then
    checks that the flash holds the file's MAIN records exactly, that a normal reboot through
    the loader reaches the running system, and that a reboot holding IN + RELOOP/EXIT reaches
    the update path again. Stages 5 and 6 passed this rehearsal (2026-10-09), as they did on
    the owner's unit. A file that fails it is not offered for flashing. Emulation is not the
    unit: a rehearsal that passes lowers the risk, it does not remove it;
  - **report a version lower than 1.15**, and change the version block only through
    `rebuild --report-version` or a schema-v2 recipe's `reported_version` (both refuse 1.15 or
    higher; a recipe's replacements cannot reach the block). The official v1.15 file and the
    `Ver1.16` stock no-op are then both higher, so both can restore the stock application. A
    modified application that reported 1.16 or more would make the updater skip both.

## 6. If something goes wrong

**Stop at the first unexpected screen and do not retry blindly.** Write down:
- the stage;
- the file's SHA-256;
- the step;
- what you expected and what you saw.

Then use a recovery stick (section 3, step 5), and **check whether MAIN really progressed**
(section 4).
- **In normal update mode** the official v1.15 file is skipped on a unit that reports 1.15. That
  is harmless after a no-op rebuild, whose application is already the official one. On a unit
  that reports a lower version (a modified application, section 5) it is written and restores
  the stock application (observed at stage 4, over an application changed only in its version
  string, and at stage 6, over one with a changed table entry; untested over one whose code is
  changed).
- **Otherwise, to write the official application**, use the `Ver1.16` stock no-op stick, or any
  stock no-op rebuild labelled higher than the version the unit reports. Writing it over the
  official application is tested (stage 1b); over a modified application it is not yet.

**If the unit no longer starts, leave it powered off and open an issue with your notes.**
