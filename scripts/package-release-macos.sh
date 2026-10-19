#!/usr/bin/env bash
set -euo pipefail

version=""
output_dir="release-assets"
skip_tests=0
skip_install=0
skip_build=0
version_only=0
clean=0
RUN=0
ALL=0

usage() {
  cat <<'EOF'
Usage: scripts/package-release-macos.sh [options]

Options:
      --clean                 Preview build-artifact cleanup, then exit.
      --run                   With --clean, actually remove artifacts.
      --all                   With --clean, include release builds and node_modules.
  -v, --version, -Version VERSION
                              Release version, for example 0.11.1 or v0.11.1.
  -o, --output-dir, -OutputDir DIR
                              Directory for release assets. Default: release-assets.
      --skip-tests, -SkipTests
                              Skip TypeScript and Rust checks.
      --skip-install, -SkipInstall
                              Skip pnpm install when node_modules is missing.
      --skip-build, -SkipBuild
                              Skip Tauri build and asset copy.
      --version-only, -VersionOnly
                              Update version files, then exit.
  -h, --help                 Show this help.
EOF
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
while [[ $# -gt 0 ]]; do
  case "$1" in
    --clean) clean=1; shift ;;
    --run) RUN=1; shift ;;
    --all) ALL=1; shift ;;
    -v|--version|-Version)
      version="${2:-}"
      shift 2
      ;;
    -o|--output-dir|-OutputDir)
      output_dir="${2:-}"
      shift 2
      ;;
    --skip-tests|-SkipTests)
      skip_tests=1
      shift
      ;;
    --skip-install|-SkipInstall)
      skip_install=1
      shift
      ;;
    --skip-build|-SkipBuild)
      skip_build=1
      shift
      ;;
    --version-only|-VersionOnly)
      version_only=1
      shift
      ;;
    --)
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done
fi

repo_root() {
  local script_dir
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
  cd "$script_dir/.." && pwd -P
}

run() {
  echo ">> $*"
  "$@"
}

print_build_output_layout() {
  local root="$1"
  local out_dir="$2"
  cat <<EOF

Build output layout:
  $root/dist
    Vite frontend build consumed by Tauri. Do not upload this as a desktop release.
  $root/src-tauri/target
    Cargo/Tauri build cache plus raw bundle output. Useful for diagnostics, not the curated release folder.
  $out_dir
    Final renamed release assets produced by this script. Use this directory for manual distribution.

EOF
}

print_release_asset_summary() {
  local out_dir="$1"
  echo
  echo "Final release assets:"
  if find "$out_dir" -maxdepth 1 -type f -print -quit | grep -q .; then
    find "$out_dir" -maxdepth 1 -type f -print | sort | sed 's/^/  /'
  else
    echo "  (none)"
  fi
  echo
}

read_json_field() {
  local file="$1"
  local field="$2"
  node -e "const fs=require('fs'); const data=JSON.parse(fs.readFileSync(process.argv[1], 'utf8')); const value=data$(printf '%s' "$field"); if (value === undefined || value === null) process.exit(1); process.stdout.write(String(value));" "$file"
}

normalize_version() {
  local input="$1"
  local normalized="${input#v}"
  if [[ ! "$normalized" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.+][0-9A-Za-z.-]+)?$ ]]; then
    echo "Version must look like SemVer, for example 0.10.1 or v0.10.1. Got: $input" >&2
    exit 1
  fi
  printf '%s' "$normalized"
}

replace_text() {
  local file="$1"
  local pattern="$2"
  local replacement="$3"
  if ! perl -0ne "exit(/$pattern/ ? 0 : 1)" "$file"; then
    echo "Pattern not found in $file: $pattern" >&2
    exit 1
  fi
  perl -0pi -e "s/$pattern/$replacement/" "$file"
}

assert_macos_host() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "Cannot package macOS assets on $(uname -s). Tauri desktop bundles are host-specific; run this script on macOS." >&2
    exit 1
  fi
}

ensure_macos_rust_targets() {
  if [[ "$skip_build" -eq 1 ]]; then
    return
  fi

  if ! command -v rustup >/dev/null 2>&1; then
    echo "rustup is required to verify macOS universal Rust targets." >&2
    echo "Install rustup, then run: rustup target add aarch64-apple-darwin x86_64-apple-darwin" >&2
    exit 1
  fi

  local rustup_cargo
  local rustup_rustc
  local rustup_bin
  rustup_cargo="$(rustup which cargo)"
  rustup_rustc="$(rustup which rustc)"
  rustup_bin="$(dirname "$rustup_cargo")"
  export PATH="$rustup_bin:$PATH"

  if [[ "$(command -v cargo)" != "$rustup_cargo" || "$(command -v rustc)" != "$rustup_rustc" ]]; then
    echo "Failed to activate rustup toolchain. Expected cargo at $rustup_cargo and rustc at $rustup_rustc." >&2
    exit 1
  fi

  local missing_targets=()
  local target
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    if ! rustup target list --installed | grep -qx "$target"; then
      missing_targets+=("$target")
    fi
  done

  if [[ "${#missing_targets[@]}" -gt 0 ]]; then
    run rustup target add "${missing_targets[@]}"
  fi

  local rust_sysroot
  rust_sysroot="$(rustc --print sysroot)"
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    if [[ ! -d "$rust_sysroot/lib/rustlib/$target" ]]; then
      echo "Rust target $target is not available in the active sysroot: $rust_sysroot" >&2
      echo "Run: rustup target add $target" >&2
      exit 1
    fi
  done
}

assert_compatible_app_identity() {
  local root="$1"
  local tauri_path="$root/src-tauri/tauri.conf.json"
  local identifier
  local product_name
  local upgrade_code

  identifier="$(read_json_field "$tauri_path" "['identifier']")"
  product_name="$(read_json_field "$tauri_path" "['productName']")"
  upgrade_code="$(read_json_field "$tauri_path" "['bundle']['windows']['wix']['upgradeCode']")"

  if [[ "$identifier" != "com.iamzhihuix.skillsmanage" ]]; then
    echo "Refusing to package: Tauri identifier changed. Keeping com.iamzhihuix.skillsmanage preserves existing app data." >&2
    exit 1
  fi
  if [[ "$product_name" != "Arsenal" ]]; then
    echo "Refusing to package: productName must remain Arsenal. Keep the identifier stable for older installs." >&2
    exit 1
  fi
  if [[ "$upgrade_code" != "28d41c68-f4a4-5134-b959-34babea58f7f" ]]; then
    echo "Refusing to package: Windows Wix upgradeCode changed. Keeping it preserves MSI upgrades from older versions." >&2
    exit 1
  fi

  grep -q 'join("\.arsenal")' "$root/src-tauri/src/path_utils.rs" || {
    echo "Refusing to package: app data must default to ~/.arsenal." >&2
    exit 1
  }
  grep -q 'storage::store::Store::open' "$root/src-tauri/src/lib.rs" || {
    echo "Refusing to package: current JSON store startup is missing." >&2
    exit 1
  }
}

update_version_files() {
  local root="$1"
  local next_version="$2"
  local today
  today="$(date +%F)"

  replace_text "$root/package.json" '("version"\s*:\s*)"[^"]+"' "\${1}\"$next_version\""
  replace_text "$root/src-tauri/tauri.conf.json" '("version"\s*:\s*)"[^"]+"' "\${1}\"$next_version\""
  replace_text "$root/src-tauri/Cargo.toml" '(\[package\]\s+name = "arsenal"\s+)version = "[^"]+"' "\${1}version = \"$next_version\""
  replace_text "$root/src/pages/SettingsView.tsx" 'const APP_VERSION = "[^"]+";' "const APP_VERSION = \"$next_version\";"

  local metainfo_path="$root/src-tauri/bundle/linux/com.iamzhihuix.skillsmanage.metainfo.xml"
  if ! grep -q "<release version=\"$next_version\"" "$metainfo_path"; then
    perl -0pi -e 'BEGIN { ($version, $date) = @ARGV; @ARGV = (); } s/(<releases>\s*\n)/$1    <release version="$version" date="$date"\/>\n/s' "$next_version" "$today" "$metainfo_path"
  fi

}

ensure_dependencies() {
  local root="$1"
  if [[ "$skip_install" -eq 1 ]]; then
    return
  fi
  if [[ ! -d "$root/node_modules" ]]; then
    run pnpm install --frozen-lockfile
  fi
}

run_checks() {
  local root="$1"
  if [[ "$skip_tests" -eq 1 ]]; then
    return
  fi

  local tsc_cmd="$root/node_modules/.bin/tsc"
  if [[ -x "$tsc_cmd" ]]; then
    run "$tsc_cmd" --noEmit
  else
    run pnpm typecheck
  fi
  run cargo test --manifest-path "$root/src-tauri/Cargo.toml" --no-run --message-format short -q
}

build_frontend() {
  local root="$1"
  local vite_cmd="$root/node_modules/.bin/vite"
  if [[ ! -x "$vite_cmd" ]]; then
    echo "Vite executable not found at $vite_cmd. Run pnpm install first." >&2
    exit 1
  fi
  run "$vite_cmd" build
}

build_app() {
  local root="$1"
  if [[ "$skip_build" -eq 1 ]]; then
    return
  fi

  build_frontend "$root"
  prepare_packaged_config "$root"

  local tauri_cmd="$root/node_modules/.bin/tauri"
  if [[ ! -x "$tauri_cmd" ]]; then
    echo "Tauri CLI executable not found at $tauri_cmd. Run pnpm install first." >&2
    exit 1
  fi

  local skip_before_build_path
  skip_before_build_path="$(mktemp "${TMPDIR:-/tmp}/skillshub-tauri-build.XXXXXX")"
  printf '%s' '{"build":{"beforeBuildCommand":""}}' > "$skip_before_build_path"
  trap 'rm -f "$skip_before_build_path"' RETURN
  run "$tauri_cmd" build --target universal-apple-darwin --bundles app,dmg --no-sign --ci --config "$skip_before_build_path"
}

copy_macos_assets() {
  local root="$1"
  local next_version="$2"
  local out_dir="$3"
  local bundle_root="$root/src-tauri/target/universal-apple-darwin/release/bundle"
  local app
  local dmg
  local config_dir="$root/src-tauri/resources/packaged-config"

  echo "Tauri raw bundle output: $bundle_root"
  app="$(find "$bundle_root/macos" -maxdepth 1 -type d -name '*.app' -print -quit 2>/dev/null || true)"
  dmg="$(find "$bundle_root/dmg" -maxdepth 1 -type f -name '*.dmg' -print -quit 2>/dev/null || true)"
  if [[ -z "$app" ]]; then
    echo "macOS .app bundle not found under $bundle_root." >&2
    exit 1
  fi
  if [[ -z "$dmg" ]]; then
    echo "macOS .dmg bundle not found under $bundle_root." >&2
    exit 1
  fi

  mkdir -p "$out_dir"
  cp -f "$dmg" "$out_dir/arsenal_${next_version}_macos_universal.dmg"
  create_macos_portable_archives \
    "$app" \
    "$config_dir" \
    "$out_dir/arsenal_${next_version}_macos_universal.zip" \
    "$out_dir/arsenal_${next_version}_macos_universal.tar.gz"
}

prepare_packaged_config() {
  local root="$1"
  local dest="$root/src-tauri/resources/packaged-config"
  mkdir -p "$dest"
  echo ">> cargo run --manifest-path $root/src-tauri/Cargo.toml --bin prepare_config_dir --release --features prepare-config -- $dest"
  cargo run --manifest-path "$root/src-tauri/Cargo.toml" --bin prepare_config_dir --release --features prepare-config -- "$dest"
}

copy_packaged_config() {
  local config_dir="$1"
  local dest="$2/.arsenal"
  if [[ ! -f "$config_dir/config.json" ]]; then
    echo "Packaged config directory not found at $config_dir. Run prepare_config_dir first." >&2
    return 1
  fi
  rm -rf "$dest"
  mkdir -p "$dest"
  cp -R "$config_dir/." "$dest/"
  rm -f "$dest/.gitkeep"
}

create_macos_portable_archives() {
  local app_path="$1"
  local config_dir="$2"
  local zip_path="$3"
  local tar_path="$4"
  local app_name
  local stage
  local status=0
  app_name="$(basename "$app_path")"
  stage="$(mktemp -d "${TMPDIR:-/tmp}/skillshub-portable.XXXXXX")"

  if ! (
    set -euo pipefail
    ditto "$app_path" "$stage/$app_name"
    copy_packaged_config "$config_dir" "$stage"
    rm -f "$zip_path" "$tar_path"
    ditto -c -k "$stage" "$zip_path"
    tar -C "$stage" -czf "$tar_path" "$app_name" ".arsenal"
  ); then
    status=1
  fi
  rm -rf "$stage"
  return "$status"
}


clean_artifacts() {
  local ROOT="$1"
# Safety: refuse to run outside the Arsenal repo
if [ ! -f "$ROOT/package.json" ] || [ ! -f "$ROOT/src-tauri/Cargo.toml" ]; then
  echo "ERROR: package.json / src-tauri/Cargo.toml not found under $ROOT"
  echo "       This script must run inside the Arsenal repository."
  exit 1
fi

DEFAULT_TARGETS=(
  "src-tauri/target/debug"
  "src-tauri/target/x86_64-pc-windows-msvc"
  "src-tauri/target/flycheck0"
  "dist"
  "node_modules/.vite"
)

EXTRA_TARGETS=(
  "src-tauri/target/release"
  "node_modules"
)

TARGETS=("${DEFAULT_TARGETS[@]}")
if [ "$ALL" -eq 1 ]; then
  TARGETS+=("${EXTRA_TARGETS[@]}")
fi

human_size() {
  # bytes -> human readable, fallback to raw number
  local bytes="$1"
  if command -v numfmt >/dev/null 2>&1; then
    numfmt --to=iec --suffix=B "$bytes" 2>/dev/null || echo "${bytes}B"
  else
    echo "${bytes}B"
  fi
}

dir_size() {
  local p="$1"
  if [ -e "$p" ]; then
    du -sk "$p" 2>/dev/null | cut -f1
  else
    echo ""
  fi
}

if [ "$ALL" -eq 1 ]; then
  echo "Mode: FULL clean (includes release build and node_modules)"
else
  echo "Mode: default clean (keeps target/release and node_modules)"
fi
if [ "$RUN" -eq 0 ]; then
  echo "Mode: DRY-RUN (nothing will be deleted; pass --run to execute)"
fi
echo "Repo: $ROOT"
echo "-----------------------------------------------------------"

total_kb=0
existing=()

for rel in "${TARGETS[@]}"; do
  abs="$ROOT/$rel"
  kb="$(dir_size "$abs")"
  if [ -z "$kb" ]; then
    printf '  [skip] %-45s (not present)\n' "$rel"
    continue
  fi
  size="$(human_size $((kb * 1024)))"
  printf '  [%5s] %-45s %s\n' "OK" "$rel" "$size"
  total_kb=$((total_kb + kb))
  existing+=("$abs")
done

echo "-----------------------------------------------------------"
echo "Total reclaimable: $(human_size $((total_kb * 1024))) across ${#existing[@]} path(s)"

if [ "$RUN" -eq 0 ]; then
  echo ""
  echo "Dry-run only. To actually delete, run:"
  echo "  "$0" --clean --run          (default targets)"
  echo "  "$0" --clean --run --all    (also release + node_modules)"
  exit 0
fi

if [ "${#existing[@]}" -eq 0 ]; then
  echo "Nothing to delete."
  exit 0
fi

echo ""
echo "Deleting..."
for abs in "${existing[@]}"; do
  resolved="$(cd "$abs" && pwd -P)"
  case "$resolved" in "$ROOT"/*) ;; *) echo "Refusing cleanup outside repository" >&2; exit 1 ;; esac
  echo "  rm -rf $abs"
  rm -rf "$abs"
done

echo "Done. Reclaimed about $(human_size $((total_kb * 1024)))."
if [ "$ALL" -eq 1 ]; then
  echo "Reminder: run 'pnpm install' before the next dev/build."
fi

}

# CI imports functions without executing a build.
if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then return 0; fi

root="$(repo_root)"
cd "$root"

if [[ "$clean" -eq 1 ]]; then clean_artifacts "$root"; exit 0; fi
if [[ "$RUN" -eq 1 || "$ALL" -eq 1 ]]; then echo "--run/--all require --clean" >&2; exit 2; fi

if [[ -z "$version" ]]; then
  version="$(read_json_field "$root/package.json" "['version']")"
fi
next_version="$(normalize_version "$version")"

update_version_files "$root" "$next_version"
assert_compatible_app_identity "$root"

if [[ "$version_only" -eq 1 ]]; then
  echo "Version files updated to $next_version. Packaging skipped because --version-only was set."
  exit 0
fi

if [[ "$skip_build" -eq 0 ]]; then
  assert_macos_host
  ensure_macos_rust_targets
fi

out_path="$root/$output_dir"
mkdir -p "$out_path"
print_build_output_layout "$root" "$out_path"

ensure_dependencies "$root"
run_checks "$root"
if [[ "$skip_build" -eq 0 ]]; then
  plugin_args=("$root/scripts/package-obsidian.mjs" --output-dir "$out_path")
  if [[ "$skip_install" -eq 1 ]]; then plugin_args+=(--skip-install); fi
  if [[ "$skip_tests" -eq 1 ]]; then plugin_args+=(--skip-tests); fi
  run node "${plugin_args[@]}"
fi

echo "Packaging target: macos"
build_app "$root"

if [[ "$skip_build" -eq 1 ]]; then
  echo "Skipping asset copy for macos because -SkipBuild was set."
  exit 0
fi

copy_macos_assets "$root" "$next_version" "$out_path"
echo "Packaged macos assets in $out_path"
print_release_asset_summary "$out_path"
