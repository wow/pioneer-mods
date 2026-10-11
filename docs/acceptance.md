# Acceptance tests: scripted emulator runs

An acceptance test drives one player's panel and touch screen in the emulator and checks what the
running application does: which length a touched BEAT LOOP pad selects, which pad lights. It is
roadmap step 2 of [modular-builds.md](./modular-builds.md) ("Verification"). Stage 7 shows why it
is needed: its lengths were right, but the pad that lights follows the stock lengths, and only a
test of the lit pad would have caught it.

**Status.** The format and the checks that need no firmware are implemented: tests load with the
catalog ([catalog.md](./catalog.md)), and implementations name the tests they must pass. The
runner, which executes a test against an update in the emulator, comes next; no test is committed
yet.

## What a test may hold

A test holds no firmware and no pixels. It presses named panel controls, touches screen points,
waits, and checks:

- **memory:** the value at a run-time address of the running application (an address is a fact
  about the application, like a recipe's offset; the value is a number the test expects);
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

  At least one step checks something, and the waits and timeouts add up to at most one hour.
- Checks:
  - `memory`: `address` (`0x` and one to eight hexadecimal digits, aligned to `bytes`), `bytes`
    (1, 2 or 4) and `equals` (a value that fits), in the emulated processor's byte order;
  - `brightest`: the mean brightness of `region` exceeds that of each of `others` (one to 16
    regions) by at least `margin` (1 to 255). Every region (`x`, `y`, `w`, `h`) is non-empty and
    on the player's screen.

Unknown steps, checks and fields are refused.

## Controls

A player names the panel controls tests may press (`controls` in its catalog entry): each has the
`code` and `value` the panel reports to the application (`0x` and two hexadecimal digits each)
and a `description`. The XDJ-700 lists `usb` and `rotary_push`, the two the emulator runs have
used so far; more are added with the evidence that names them.

## Naming tests

A feature's or skin's implementation names the tests it must pass (`acceptance`, a list of test
ids). Each must exist and be for the implementation's player. A test an implementation does not
pass yet is not named by it: stage 7 fails a lit-pad test until the lit-pad fix exists, and that
result goes with its known limits.
