#!/usr/bin/env bash
# Remove only build intermediates. Compatible with Bash 3.2+.
set -euo pipefail

run=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --run) run=1 ;;
    -h|--help)
      printf 'Usage: bash %s [--run]\nDefault: preview. --run: delete build intermediates only.\n' "$(basename "$0")"
      exit 0 ;;
    *) printf 'Unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
  shift
done

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
if [[ ! -f "$repo_root/package.json" || ! -f "$repo_root/src-tauri/Cargo.toml" ]] ||
   ! grep -q '"name"[[:space:]]*:[[:space:]]*"arsenal"' "$repo_root/package.json"; then
  echo 'Expected a Arsenal repository next to this script.' >&2
  exit 1
fi

assert_safe_target() {
  local path="$1" cursor="$1"
  case "$path" in "$repo_root"/*) ;; *) echo 'Refusing cleanup outside repository.' >&2; exit 1 ;; esac
  while [[ "$cursor" != "$repo_root" ]]; do
    if [[ -L "$cursor" ]]; then
      echo 'Refusing cleanup through a filesystem link.' >&2
      exit 1
    fi
    cursor="${cursor%/*}"
  done
}

targets=("$repo_root/dist" "$repo_root/.vite" "$repo_root/node_modules/.vite"
  "$repo_root/node_modules/.vite-temp" "$repo_root/tsconfig.tsbuildinfo"
  "$repo_root/tsconfig.node.tsbuildinfo" "$repo_root/vite.config.js" "$repo_root/vite.config.d.ts")
shopt -s nullglob
for base in "$repo_root/src-tauri/target" "$repo_root/target"; do
  assert_safe_target "$base"
  [[ -d "$base" ]] || continue
  for profile in "$base/debug" "$base/release" "$base"/*/debug "$base"/*/release; do
    for cache in .fingerprint build deps incremental; do
      targets+=("$profile/$cache")
    done
  done
done

# Validate all paths before deleting anything; rm does not follow nested symlinks.
count=0
for target in "${targets[@]}"; do
  assert_safe_target "$target"
  if [[ -e "$target" ]]; then
    printf '  %s\n' "${target#"$repo_root/"}"
    count=$((count + 1))
  fi
done
if [[ "$run" -eq 1 ]]; then
  for target in "${targets[@]}"; do
    assert_safe_target "$target"
    [[ -e "$target" ]] || continue
    rm -rf -- "$target"
  done
  printf 'Removed %s intermediate path(s).\n' "$count"
else
  printf 'Preview: %s path(s). Pass --run to delete.\n' "$count"
fi
echo 'Kept final binaries, bundles, release-assets, dependencies and application data. Rebuild caches will be regenerated.'
