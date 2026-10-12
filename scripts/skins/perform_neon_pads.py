# SPDX-License-Identifier: CC-BY-SA-4.0
"""The source of the Neon pads skin's art: it generates the 36 paint edits of
`recipes/xdj700-v1.15/skin-perform-neon-pads.json` from the shapes, the font and the colours below.

Part of the skin, so under its licence (CC BY-SA 4.0, LICENSE-ART), like the recipe it writes.
Run over the committed recipe after an art change, it writes a new draft; with the art unchanged,
it leaves the recipe and its pins as they are.

- The pad: a rounded rectangle filling the 80x53 image (radius 7, a 2-pixel border). The four
  corners outside the rounding are black, as the screen around the pads is (measured in
  emulation), so the rounding shows.
- The labels: the stock lengths 1/2, 1, 2, 4, 8 and 16 in a 5x7 pixel font drawn for this skin
  (not copied from a font file; at 5x7, digit shapes converge on similar forms), shown at 3x.
- The colours: our own RGB565 values, one set per state.

Offsets come from the committed PERFORM screen (`catalog/screens/xdj700-v1.15/perform.json`). The
output pins are left as placeholders: complete them with `patch-cli precondition` on the official
update (docs/recipes.md, "Writing a recipe").

usage: python3 scripts/skins/perform_neon_pads.py <app root> <out.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

WIDTH, HEIGHT = 80, 53
RADIUS, BORDER = 7, 2
SCALE, GAP = 3, 3
LABELS = ["1/2", "1", "2", "4", "8", "16"]

# The skin's own 5x7 pixel font.
FONT = {
    "1": ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
    "2": [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
    "4": ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
    "6": ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."],
    "8": [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
    "/": ["....#", "....#", "...#.", "..#..", ".#...", "#....", "#...."],
}

# The corners outside the rounding, in every state: black.
CORNER = 0x0000

# Fill, border and label, per state: the skin's own RGB565 colours.
COLOURS = {
    "normal": (0x10A2, 0x07FF, 0xFFDF),
    "pressed": (0x0418, 0x07FF, 0xFFDF),
    "greyed_out": (0x10A2, 0x4208, 0x6B4D),
    "lit": (0x07FF, 0xFFDF, 0x0841),
    "lit_pressed": (0x9FFF, 0xFFDF, 0x0841),
    "lit_greyed_out": (0x0410, 0x4208, 0x8410),
}


def inside_rounded(x: int, y: int, inset: int) -> bool:
    """Whether (x, y) lies inside the pad's rounded rectangle shrunk by `inset` pixels."""
    left, top, right, bottom = inset, inset, WIDTH - 1 - inset, HEIGHT - 1 - inset
    if not (left <= x <= right and top <= y <= bottom):
        return False
    # Distance past the nearest corner centre on each axis (0 along the straight edges).
    r = RADIUS - inset
    dx = max(left + r - x, 0, x - (right - r))
    dy = max(top + r - y, 0, y - (bottom - r))
    return dx * dx + dy * dy <= r * r


def label_mask(text: str) -> set[tuple[int, int]]:
    """The pixels of `text` in the skin's font, centred in the pad."""
    width = len(text) * 5 * SCALE + (len(text) - 1) * GAP
    x0, y0 = (WIDTH - width) // 2, (HEIGHT - 7 * SCALE) // 2
    mask = set()
    for i, char in enumerate(text):
        left = x0 + i * (5 * SCALE + GAP)
        for row, line in enumerate(FONT[char]):
            for col, bit in enumerate(line):
                if bit == "#":
                    for dy in range(SCALE):
                        for dx in range(SCALE):
                            mask.add((left + col * SCALE + dx, y0 + row * SCALE + dy))
    return mask


def art(label: str) -> str:
    """One palette digit per pixel: 0 the corners, 1 fill, 2 border, 3 label."""
    mask = label_mask(label)
    digits = []
    for y in range(HEIGHT):
        for x in range(WIDTH):
            if not inside_rounded(x, y, 0):
                digits.append("0")
            elif not inside_rounded(x, y, BORDER):
                digits.append("2")
            elif (x, y) in mask:
                digits.append("3")
            else:
                digits.append("1")
    return "".join(digits)


def image_edits(perform: dict[str, Any]) -> list[dict[str, Any]]:
    """The skin's 36 paint edits, by offset, for the PERFORM screen entry `perform`."""
    pads = perform["slots"]["beat_loop.pad"]["images"]
    edits = []
    for index, element in enumerate(pads["elements"]):
        label = LABELS[index]
        pixels = art(label)
        for state in pads["states"]:
            fill, border, text = COLOURS[state]
            palette = [{"colour": f"0x{colour:04x}"} for colour in (CORNER, fill, border, text)]
            state_name = state.replace("_", " ")
            edits.append(
                {
                    "offset": int(element["images"][state], 16),
                    "width": WIDTH,
                    "height": HEIGHT,
                    "paint": {
                        "at": {"x": 0, "y": 0, "width": WIDTH, "height": HEIGHT},
                        "palette": palette,
                        "pixels_hex": pixels,
                    },
                    "purpose": f"Neon pads: the {state_name} image of BEAT LOOP pad "
                    f"{index + 1} of 6 (label {label}), our own art.",
                }
            )
    return sorted(edits, key=lambda edit: int(edit["offset"]))


def recipe(perform: dict[str, Any]) -> dict[str, Any]:
    """The recipe draft, its output pins placeholders."""
    return {
        "schema_version": 2,
        "recipe_id": "xdj700-v1.15-skin-perform-neon-pads",
        "description": "Neon pads, a PERFORM skin: the six BEAT LOOP pads in our own art (dark "
        "pads, a cyan border, filled cyan when lit), in all six states, with the stock labels "
        "1/2, 1, 2, 4, 8 and 16 in our own digits. Images only; the pads' behaviour is stock.",
        "target": {
            "release": "xdj700-v1.15",
            "upd_sha256": "73edec9802da51672257c2599efc04209dc92478fcbaa1a0425b3b122e33f99c",
            "application_sha256": (
                "1875381b56d065a2b0a97a63b64ead5ce71397c521b7a62713c5bb4a0e055939"
            ),
        },
        "label": "Ver1.16",
        "reported_version": "0.13",
        "replacements": [],
        "image_edits": image_edits(perform),
        "expected": {"application_sha256": "0" * 64},
    }


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit("usage: python3 scripts/skins/perform_neon_pads.py <app root> <out.json>")
    root, out = Path(sys.argv[1]), Path(sys.argv[2])
    perform = json.loads((root / "catalog/screens/xdj700-v1.15/perform.json").read_text())
    draft = recipe(perform)
    if out.exists():
        # Regenerating over a pinned recipe: unchanged, it keeps its pins; changed, it is a new
        # draft to pin again.
        current = json.loads(out.read_text())
        if unpinned(current) == unpinned(draft):
            print(f"{out}: unchanged, pins kept")
            return
        print(f"{out}: the art changed; pin it again with patch-cli precondition")
    out.write_text(json.dumps(draft, indent=2) + "\n")


def unpinned(recipe: dict[str, Any]) -> dict[str, Any]:
    """`recipe` without its output pins (`expected`)."""
    return {key: value for key, value in recipe.items() if key != "expected"}


if __name__ == "__main__":
    main()
