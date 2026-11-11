#!/usr/bin/env bash
# Builds latest.json (the updater manifest) of a release from the signed files in it and uploads it.
# Parallel build jobs that each patch latest.json overwrite each other's entries, so the manifest
# is assembled once, after all builds.
# Origin: adapted from OpsDeck scripts/latest-json.sh (same author).
# Usage (GH_TOKEN set, gh + jq available): scripts/latest-json.sh v0.1.0 [owner/repo]
set -euo pipefail
tag="$1"
repo="${2:-${GITHUB_REPOSITORY:-LeoAlecksey/OpsOpsAi}}"
version="${tag#v}"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT

gh release download "$tag" --repo "$repo" --pattern '*.sig' --dir "$dir"
notes="$(gh release view "$tag" --repo "$repo" --json body --jq .body)"
base="https://github.com/$repo/releases/download/$tag"

# updater platform keys for a signed file; the first key of each line is also the generic one
keys_for() {
  case "$1" in
    *_amd64.AppImage) echo "linux-x86_64-appimage linux-x86_64" ;;
    *_amd64.deb) echo "linux-x86_64-deb" ;;
    *.x86_64.rpm) echo "linux-x86_64-rpm" ;;
    *_aarch64.AppImage) echo "linux-aarch64-appimage linux-aarch64" ;;
    *_arm64.deb) echo "linux-aarch64-deb" ;;
    *.aarch64.rpm) echo "linux-aarch64-rpm" ;;
    *_x64-setup.exe) echo "windows-x86_64-nsis windows-x86_64" ;;
    *_x64_*.msi) echo "windows-x86_64-msi" ;;
    *aarch64.app.tar.gz) echo "darwin-aarch64-app darwin-aarch64" ;;
    # only Apple Silicon is built: an .app archive without an arch in its name is that one
    *.app.tar.gz) echo "darwin-aarch64-app darwin-aarch64" ;;
    *) echo "" ;;
  esac
}

platforms='{}'
for sig in "$dir"/*.sig; do
  file="$(basename "$sig" .sig)"
  keys="$(keys_for "$file")"
  if [ -z "$keys" ]; then
    echo "::warning::$file: неизвестный тип пакета, в latest.json не попадает"
    continue
  fi
  for key in $keys; do
    platforms="$(jq --arg k "$key" --arg url "$base/$file" --rawfile sig "$sig" \
      '. + {($k): {signature: ($sig | rtrimstr("\n")), url: $url}}' <<<"$platforms")"
  done
done

if [ "$(jq 'length' <<<"$platforms")" -eq 0 ]; then
  echo "::error::в релизе $tag нет подписанных файлов обновлений (*.sig)"
  exit 1
fi

jq -n --arg v "$version" --arg notes "$notes" --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson p "$platforms" \
  '{version: $v, notes: $notes, pub_date: $date, platforms: $p}' >"$dir/latest.json"
echo "latest.json for $tag: $(jq -r '.platforms | keys | join(", ")' "$dir/latest.json")"
gh release upload "$tag" "$dir/latest.json" --repo "$repo" --clobber
