#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
branding_dir="$repo_root/dispatch-server/public/branding"
default_master="$branding_dir/dispatch-icon.png"

usage() {
  cat <<EOF
Usage: scripts/derive-icons.sh [MASTER_ICON]

Derive all Dispatch icon variants from the square master icon.
MASTER_ICON defaults to:
  $default_master

Requires ImageMagick (magick or convert) or macOS sips.
EOF
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
  usage
  exit 0
fi

if (( $# > 1 )); then
  usage >&2
  exit 2
fi

master="${1:-$default_master}"
if [[ ! -f "$master" ]]; then
  echo "Master icon not found: $master" >&2
  exit 1
fi

if command -v magick >/dev/null 2>&1; then
  resize_tool="magick"
  dimensions="$(magick identify -format '%w %h' "$master")"
elif command -v convert >/dev/null 2>&1 && command -v identify >/dev/null 2>&1; then
  resize_tool="convert"
  dimensions="$(identify -format '%w %h' "$master")"
elif command -v sips >/dev/null 2>&1; then
  resize_tool="sips"
  width="$(sips -g pixelWidth "$master" | awk '/pixelWidth:/ { print $2 }')"
  height="$(sips -g pixelHeight "$master" | awk '/pixelHeight:/ { print $2 }')"
  dimensions="$width $height"
else
  echo "Unable to derive icons: install ImageMagick or run the script on macOS with sips." >&2
  exit 1
fi

read -r width height <<<"$dimensions"
if [[ ! $width =~ ^[0-9]+$ || ! $height =~ ^[0-9]+$ ]]; then
  echo "Unable to read master icon dimensions: $master" >&2
  exit 1
fi

if (( width != height )); then
  echo "Master icon must be square; found ${width}x${height}: $master" >&2
  exit 1
fi

if (( width < 180 )); then
  echo "Master icon must be at least 180x180; found ${width}x${height}: $master" >&2
  exit 1
fi

mkdir -p "$branding_dir"
temp_dir="$(mktemp -d "$branding_dir/.derive-icons.XXXXXX")"
trap 'rm -rf "$temp_dir"' EXIT

resize_icon() {
  local size="$1"
  local output="$2"

  case "$resize_tool" in
    magick)
      magick "$master" -filter Lanczos -resize "${size}x${size}" -strip "$output"
      ;;
    convert)
      convert "$master" -filter Lanczos -resize "${size}x${size}" -strip "$output"
      ;;
    sips)
      sips -z "$size" "$size" "$master" --out "$output" >/dev/null
      ;;
  esac
}

while read -r size filename; do
  resize_icon "$size" "$temp_dir/$filename"
done <<'EOF'
180 dispatch-icon-180.png
64 dispatch-icon-64.png
32 favicon-32.png
EOF

while read -r _ filename; do
  mv "$temp_dir/$filename" "$branding_dir/$filename"
  echo "Updated dispatch-server/public/branding/$filename"
done <<'EOF'
180 dispatch-icon-180.png
64 dispatch-icon-64.png
32 favicon-32.png
EOF

echo "Derived Dispatch icons from ${width}x${height} master using $resize_tool."
