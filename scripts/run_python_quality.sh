#!/usr/bin/env bash
set -euo pipefail

if ! command -v git >/dev/null 2>&1; then
  echo "git is required to discover tracked Python files."
  exit 1
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

python_files=()
while IFS= read -r -d '' file; do
  python_files+=("${file}")
done < <(git ls-files -z '*.py')

if [[ "${#python_files[@]}" -eq 0 ]]; then
  echo "No tracked Python files found; skipping Python quality checks."
  exit 0
fi

ruff format --check "${python_files[@]}"
ruff check "${python_files[@]}"
mypy "${python_files[@]}"

set +e
pytest -q
pytest_status=$?
set -e

if [[ "${pytest_status}" -ne 0 && "${pytest_status}" -ne 5 ]]; then
  exit "${pytest_status}"
fi

if [[ "${pytest_status}" -eq 5 ]]; then
  echo "pytest reported no tests collected; treating as success."
fi
