#!/usr/bin/env bash
# generates a markdown snippet with direct download links to release assets.
# scans a directory (usually the downloaded artifacts/) for matching files
# and emits links via github's release asset URL.
# usage: release-links.sh <owner/repo> <tag> <assets-dir>
set -euo pipefail

REPO="${1:?usage: release-links.sh <owner/repo> <tag> <assets-dir>}"
TAG="${2:?usage: release-links.sh <owner/repo> <tag> <assets-dir>}"
ROOT="${3:?usage: release-links.sh <owner/repo> <tag> <assets-dir>}"

[[ -d "$ROOT" ]] || { echo "error: $ROOT not found" >&2; exit 1; }

BASE="https://github.com/${REPO}/releases/download/${TAG}"

emit_link() {
  local label="$1" pattern="$2"
  local file
  file="$(find "$ROOT" -type f -iname "$pattern" | head -1)"
  if [[ -z "$file" ]]; then
    echo "warning: no asset matching '$pattern' under $ROOT" >&2
    return
  fi
  echo "- [${label}](${BASE}/$(basename "$file"))"
}

{
  echo "## downloads"
  echo
  emit_link 'windows (.exe)' '*setup.exe'
  emit_link 'macOS (.dmg, ARM/M-series)' '*aarch64.dmg'
  emit_link 'macOS (.dmg, x64)' '*x64.dmg'
  emit_link 'linux (.AppImage)' '*.AppImage'
}