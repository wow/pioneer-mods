"""The committed Neon pads recipe is what its generator writes: the same 36 paint edits."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
from types import ModuleType

ROOT = Path(__file__).resolve().parent.parent


def generator() -> ModuleType:
    path = ROOT / "scripts/skins/perform_neon_pads.py"
    spec = importlib.util.spec_from_file_location("perform_neon_pads", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_the_committed_recipe_is_the_generators_art() -> None:
    perform = json.loads((ROOT / "catalog/screens/xdj700-v1.15/perform.json").read_text())
    committed = json.loads((ROOT / "recipes/xdj700-v1.15/skin-perform-neon-pads.json").read_text())
    draft = generator().recipe(perform)
    assert draft["image_edits"] == committed["image_edits"]
    for key in ("recipe_id", "description", "target", "label", "reported_version"):
        assert draft[key] == committed[key], key
    assert len(draft["image_edits"]) == 36
