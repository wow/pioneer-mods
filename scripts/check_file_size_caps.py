from __future__ import annotations

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


def git_tracked_files() -> list[Path]:
    output = subprocess.check_output(["git", "ls-files"], text=True)
    return [Path(line.strip()) for line in output.splitlines() if line.strip()]


def line_cap_for(path: Path) -> int | None:
    override = PATH_CAP_OVERRIDES.get(path.as_posix())
    if override is not None:
        return override
    return LINE_CAPS.get(path.suffix)


def main() -> int:
    violations: list[tuple[str, int, int]] = []

    for path in git_tracked_files():
        cap = line_cap_for(path)
        if cap is None or not path.is_file():
            continue

        line_count = sum(1 for _ in path.open("r", encoding="utf-8", errors="replace"))
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
