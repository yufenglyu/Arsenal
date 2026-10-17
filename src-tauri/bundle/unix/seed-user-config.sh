#!/bin/sh
# Copy the packaged default .skillshub tree into the installing user's home.
# Existing user data is left untouched; the app initializes JSON under its lock.
set -eu

case "${1:-}" in
  abort-upgrade|abort-remove|abort-deconfigure)
    exit 0
    ;;
esac

find_src() {
  script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
  for candidate in \
    "/usr/share/skillshub/packaged-config" \
    "/usr/lib/skillshub/packaged-config" \
    "/usr/lib/SkillsHub/packaged-config" \
    "/usr/lib/skillshub/resources/packaged-config" \
    "/usr/lib/SkillsHub/resources/packaged-config" \
    "$script_dir/packaged-config" \
    "$script_dir/resources/packaged-config" \
    "$script_dir/../resources/packaged-config"
  do
    if [ -d "$candidate/platform" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

lookup_home() {
  user="$1"
  if command -v getent >/dev/null 2>&1; then
    getent passwd "$user" | cut -d: -f6
    return
  fi
  awk -F: -v u="$user" '$1 == u { print $6; exit }' /etc/passwd
}

lookup_install_user() {
  if [ "$(id -u)" -ne 0 ]; then
    printf '%s\n' "$(id -un)"
    return 0
  fi
  if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
    printf '%s\n' "$SUDO_USER"
    return 0
  fi
  if [ -n "${PKEXEC_UID:-}" ]; then
    user=$(awk -F: -v id="$PKEXEC_UID" '$3 == id { print $1; exit }' /etc/passwd || true)
    if [ -n "$user" ] && [ "$user" != "root" ]; then
      printf '%s\n' "$user"
      return 0
    fi
  fi
  return 1
}

resolve_dest() {
  if [ -n "${SKILLSHUB_CONFIG_DIR:-}" ]; then
    printf '%s\n' "$SKILLSHUB_CONFIG_DIR"
    return 0
  fi

  if [ "$(id -u)" -eq 0 ]; then
    target_user=$(lookup_install_user || true)
    if [ -z "$target_user" ]; then
      # Package managers often run as root without SUDO_USER; skip rather than
      # writing into /root. First launch still seeds ~/.skillshub.
      return 1
    fi
    home=$(lookup_home "$target_user")
    if [ -n "$home" ]; then
      printf '%s\n' "$home/.skillshub"
      return 0
    fi
    return 1
  fi

  if [ -z "${HOME:-}" ]; then
    return 1
  fi
  printf '%s\n' "$HOME/.skillshub"
}

copy_missing() {
  src="$1"
  dest="$2"

  mkdir -p "$dest/library"

  if [ ! -d "$dest/platform" ] && [ -d "$src/platform" ]; then
    mkdir -p "$dest/platform"
    cp -R "$src/platform/." "$dest/platform/"
  fi


  if [ -f "$src/library/.keep" ] && [ ! -f "$dest/library/.keep" ]; then
    cp "$src/library/.keep" "$dest/library/.keep"
  fi
}

src=$(find_src) || exit 0
dest=$(resolve_dest) || exit 0
copy_missing "$src" "$dest"

if [ "$(id -u)" -eq 0 ]; then
  target_user=$(lookup_install_user || true)
  if [ -n "$target_user" ]; then
    chown -R "$target_user:" "$dest" || true
  fi
fi

exit 0
