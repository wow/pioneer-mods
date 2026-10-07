from __future__ import annotations

import os
import subprocess
from pathlib import Path

LINE_CAPS = {
    ".rs": 400,
    ".py": 300,
    ".md": 500,
}

PATH_CAP_OVERRIDES = {
    "CHANGELOG.md": 2000,
}


def git_repo_root() -> Path:
    output = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True)
    return Path(output.strip())


def git_tracked_files(repo_root: Path) -> list[Path]:
    output = subprocess.check_output(["git", "ls-files", "-z"], cwd=repo_root)
    tracked = [Path(os.fsdecode(item)) for item in output.split(b"\0") if item]
    return tracked


def line_cap_for(path: Path) -> int | None:
    override = PATH_CAP_OVERRIDES.get(path.as_posix())
    if override is not None:
        return override
    return LINE_CAPS.get(path.suffix)


def main() -> int:
    repo_root = git_repo_root()
    violations: list[tuple[str, int, int]] = []

    for path in git_tracked_files(repo_root):
        cap = line_cap_for(path)
        absolute_path = repo_root / path
        if cap is None or not absolute_path.is_file():
            continue

        line_count = sum(1 for _ in absolute_path.open("r", encoding="utf-8", errors="replace"))
        if line_count > cap:
            violations.append((path.as_posix(), line_count, cap))

    if not violations:
        print("file-size-cap check passed")
        return 0

    print("file-size-cap check failed:")
    for file_path, line_count, cap in violations:
        print(f" - {file_path}: {line_count} lines (cap: {cap})")
    print("Refactor oversized files into smaller modules/docs.")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
