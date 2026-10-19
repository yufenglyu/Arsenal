#!/usr/bin/env bash
# The cleanup allowlist is platform independent; keep it in the Linux entrypoint.
# Requires only the system Bash (including macOS Bash 3.2), not Node or Cargo.
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
if [[ "${1:-}" == '-h' || "${1:-}" == '--help' ]]; then
  printf 'Usage: bash %s [--run]\nDefault: preview. --run: delete build intermediates only.\n' "$(basename "$0")"
  exit 0
fi
exec bash "$script_dir/clean-build-linux.sh" "$@"
