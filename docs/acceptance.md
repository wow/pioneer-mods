# Acceptance tests: scripted emulator runs

An acceptance test drives one player's panel and touch screen in the emulator and checks what the
running application does: which length a touched BEAT LOOP pad selects, which pad lights. It is
roadmap step 2 of [modular-builds.md](./modular-builds.md) ("Verification"). Stage 7 as first
published (`0.12`) showed why it is needed: its lengths were right, but the pad that lit followed
the stock lengths, and only a test of the lit pad caught it.

**Status.** The format, the checks that need no firmware and the runner are implemented: tests
load with the catalog ([catalog.md](./catalog.md)), implementations name the tests they must
pass, and `patch-cli accept` runs a test on an update in the owner's emulator
([Running a test](#running-a-test)). Three XDJ-700 tests are committed
([The BEAT LOOP tests](#the-beat-loop-tests)).

## What a test may hold

A test holds no firmware and no pixels. It presses named panel controls, touches screen points,
waits, and checks:

- **memory:** a value of the application's run-time state, the variables it sets while it runs
  (the selected loop length, the lit pad). A check may read only the address ranges its player
  names as run-time state (`state_memory`), never the loaded application, and a test holds at
  most 64 memory checks, so a test cannot carry the application's code or data;
- **brightness:** that one region of the screen is brighter than others of the same frame, by a
  margin. Regions are compared with each other; no reference image is stored or published.

The emulator, the official update and anything a run produces (frames, logs, memory) stay on the
maintainer's computer; a runner reports PASS or FAIL per check, with counts.

## Format

`catalog/acceptance/<id>.json`, named after its `id`:

```json
{
  "schema_version": 1,
  "id": "beat-loop-1-to-32-pads-select-lengths",
  "player": "xdj700-v1.15",
  "description": "Each BEAT LOOP pad selects the length it shows",
  "stick": true,
  "steps": [
    { "wait_until": { "check": { "memory": { "address": "0x…", "bytes": 4, "equals": 1 } },
                      "timeout_ms": 120000 } },
    { "press": { "control": "usb" } },
    { "touch": { "x": 50, "y": 420, "ms": 150 } },
    { "wait": { "ms": 500 } },
    { "expect": { "memory": { "address": "0x…", "bytes": 4, "equals": 1 } } },
    { "expect": { "brightest": { "region": { "x": 10, "y": 400, "w": 80, "h": 53 },
                                 "others": [{ "x": 95, "y": 400, "w": 80, "h": 53 }],
                                 "margin": 16 } } }
  ],
  "maintainer": "…"
}
```

- `player`: the player whose application the test runs. Its controls and screen bound the steps.
- `stick`: whether the run needs the owner's USB stick image, with music, plugged in from the
  start (default `false`).
- `steps`, in order, at most 1000, each one of:
  - `wait`: `ms` milliseconds of host time (1 ms to 10 minutes);
  - `press`: one of the player's `controls`, held `ms` milliseconds (default 300, at most 10 s);
  - `touch`: the screen at (`x`, `y`), in pixels from the top left, held `ms` milliseconds
    (default 300, at most 10 s); the point must be on the player's screen;
  - `expect`: a check that must hold now;
  - `wait_until`: a `check` polled until it holds, failing after `timeout_ms` (1 ms to
    10 minutes).

  At least one step checks something, and the waits, holds and timeouts add up to at most one
  hour.
- Checks:
  - `memory`: `address` (a guest physical address, as the emulator's memory bus sees it: `0x`
    and one to eight hexadecimal digits, aligned to `bytes`), `bytes` (1, 2 or 4) and `equals` (a
    value that fits), in the emulated processor's byte order. The bytes lie in one of the
    player's `state_memory` ranges; at most 64 memory checks per test;
  - `brightest`: the mean brightness of `region` exceeds that of each of `others` (one to 16
    regions) by at least `margin` (1 to 255). Every region (`x`, `y`, `w`, `h`) is non-empty and
    on the player's screen; no region of `others` overlaps `region` or another of `others`,
    since shared pixels would make the check impossible to pass, or weaker than its margin.

A pixel's brightness is its Rec. 601 luma, `(299 R + 587 G + 114 B) / 1000` rounded down, from
8-bit channels; an RGB565 pixel is expanded first by replicating each channel's high bits into
its low bits (31 and 63 become 255). A region's brightness is the mean over its pixels, rounded
down (`patch_schema::catalog::mean_brightness`, which every runner uses).

Unknown steps, checks and fields are refused.

## Controls and run-time state

A player names the panel controls tests may press (`controls` in its catalog entry): each has the
`code` and `value` the application receives from the panel, as the emulator delivers them (`0x`
and two hexadecimal digits each; not checked against the hardware panel), and a `description`.
No two controls have the same code and value: a test pressing one would press the other.
The XDJ-700 lists `usb` and `rotary_push`, the two the emulator runs have used so far; more are
added with the evidence that names them.

A player also names the address ranges memory checks may read (`state_memory`): each a `start` and
an exclusive `end` (guest physical addresses; ranges that overlap or touch are written as one) and
the `evidence` that the range holds only run-time state, outside the loaded application (and
outside any copy of it the application unpacks into RAM). The catalog check refuses a range that
overlaps the loaded application (the engine's load address and application length for the
player's release); the rest, that a range holds run-time state and no unpacked copy, rests on its
evidence and on review. The XDJ-700 names three, all found in emulation by
comparing its memory before and after each BEAT LOOP pad is touched, at the same address in every
boot of stock, stage 5 and stage 7, and above the loaded application (`0x08000000` to
`0x091BD788`):

- `0x0B6C3E3C` to `0x0B6C3E44`: the current loop's length in beats, as a fraction: the numerator,
  then the denominator (32 bits each); 0 and 0 before any loop;
- `0x0C0E6C2C` to `0x0C0E6C44`: the six BEAT LOOP pads' lit states, left to right (32 bits each):
  2 lit, 0 not;
- `0x0C0E75C0` to `0x0C0E75C4`: the index of the pad touched last, 0 to 5 from the left (32 bits);
  0 before any touch.

## Running a test

`patch-cli accept` runs one test on one update in the owner's local emulator:

```bash
cargo run --release -p patch-cli -- accept \
  --test <id> \
  --update /path/to/XDJ700.UPD \
  --emulator /path/to/qemu-system-sh4 \
  [--stick /path/to/stick.img] [--time-factor 4]
```

- `--update` is any update of the test's player: the official file, a stage file or a build. It
  is read, never changed.
- `--emulator` is the owner's emulator build (`XDJ700_QEMU` names it otherwise). The emulator is
  not part of this repository; the runner starts it headless, with the player's machine (the
  XDJ-700's is `xdj700`, with a 16 MiB flash), and quits it afterwards.
- `--stick` is the owner's USB stick image, for a test with `"stick": true`; the emulator's writes
  go to a throwaway overlay.
- `--time-factor` (1 to 20, default 4) is how many times slower than real time the emulator runs
  at most. The emulator times a hold on its own clock, so the runner waits a hold of `ms` out for
  `ms` times the factor, then 100 ms more for the release to be acted on. The XDJ-700 emulator
  runs at about a quarter of real time; a slower computer needs a larger factor.

Before anything is read or started, the test must be in the catalog, the runner must know the
player's emulator, the emulator and any stick image must be files, and a test that needs a stick
must be given one. The update must be of the test's release: its decoded application has the
release's length, which every update of a release keeps (the official file, stage files, builds).
The runner then lays the update's MAIN image out in an erased flash image, as the player's updater
does, and boots it with none of the owner's `CDJ_*` emulator settings but the player's own. Steps
run in order:

- `press` and `touch` go to the emulator's panel key socket (UDP on 127.0.0.1; another local
  process could send it panel input too, which affects only the emulated player), and the runner
  waits out the hold. The emulator binds that socket on its first panel frame, well into boot, and
  logs it; the first press or touch waits for that line (at most five minutes), and a run whose
  emulator could not bind the socket is refused, so no input is silently lost;
- `memory` checks read the value through the emulator's monitor (`xp`), a Unix socket in the
  run's private directory: the monitor can read memory and run programs as the owner, so no other
  user may reach it;
- `brightest` checks take a frame (`screendump`), check that it has the player's screen size and
  holds one pixel per position, measure each region and compare;
- `wait_until` reads its check again every 500 ms until it holds or its timeout passes.

Each check prints one line as soon as it is decided, PASS or FAIL, with what was read (a value, or
the brightness of each region); a failed check does not stop the run, and the command fails if any
check failed. An emulator error ends the run, after the lines already printed. The flash image,
frames and the emulator's log stay in a temporary directory only the owner may enter (mode 0700),
removed afterwards; nothing else is printed. Ctrl-C (or SIGTERM) ends the run at the next step or
poll, or before the emulator starts: the emulator is stopped and the directory removed. A second
Ctrl-C ends the runner at once, like a run killed outright (`kill -9`), which cannot remove the
directory: its flash image, from the owner's update, then stays in the system's temporary directory
until deleted.

## The BEAT LOOP tests

Three tests touch the six BEAT LOOP pads of the PERFORM screen left to right, with a track loaded
from the owner's stick (`"stick": true`). The stick's first track must load after USB and seven
pushes of the rotary selector; the PERFORM screen opens when its tab is touched. Each test first
checks that no loop is set yet (and the lit-pad test that no pad is lit), then touches
each pad and polls for a value no earlier touch can leave: the touched pad's index, and for the
first pad (index 0 before any touch) its length or its lit state. A length test then expects the
length; the lit-pad test waits 2 s more for the lit states to settle, then expects the touched pad
lit and the other five not. So a poll never passes on the previous touch's state, whether the build
passes the test or not, and a failing run fails at the same steps every time:

| Test | Checks | Stock | Stage 5 | Stage 7 | Neon pads |
| --- | --- | --- | --- | --- | --- |
| `beat-loop-1-to-32-pads-select-lengths` | lengths 1, 2, 4, 8, 16, 32 | FAIL | FAIL | PASS | FAIL |
| `beat-loop-16-plays-32-pads-select-lengths` | lengths 1/2, 1, 2, 4, 8, 32 | FAIL | PASS | FAIL | FAIL |
| `beat-loop-lit-pad-follows-touch` | the touched pad lights | PASS | FAIL | PASS | PASS |

Stage 7 names the first test and the lit-pad test, and stage 5 the second. Stock and stage 7 pass
the lit-pad test (stage 7 changes the list that decides the lit pad along with its lengths); stage
5 fails it, its known limit ([xdj700-flashing.md](./xdj700-flashing.md), "The lit BEAT LOOP pad").
The Neon pads skin names it: it changes only images, so the pads light and set lengths as on
stock, and it fails the length tests at stock's steps. The suite also runs the lit-pad test on
stage 7 as first published (`0.12`, without the lit list), which lights the pad to the right of
the touched one and none for 32: the failing control that shows the checks that the other pads
stay unlit can fail.
All thirteen results are the owner suite's (`official_accept_beat_loop`), in the owner's emulator
with the official v1.15 file and the stage and skin files built from it; for each failing run it
also checks which steps fail. Boot and browsing are waited out for fixed times (about two and a half
minutes, host time), which a much slower or busier host may outrun; the suite runs at most one
emulator per two host cores, three at most, for that reason. The waits become polls when a state
range shows that a track is loaded.

## Naming tests

A feature's or skin's implementation names the tests it must pass (`acceptance`, a list of test
ids). Each must exist and be for the implementation's player. A test an implementation does not
pass yet is not named by it: stage 5 fails the lit-pad test, and that result goes with its known
limits.
