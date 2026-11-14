#!/usr/bin/bash
# Render the app icon — `[a]` — from its spec, in the typeface the spec names.
#
# The mark is Geist: weight-300 teal brackets around a weight-700 off-white `a`,
# on the dark rounded pad (brand-review/refs/app-icon-1024.svg). The icons shipped
# through 1-4 were a DejaVu Sans redraw in a single teal, and the reference PNG
# beside the spec is a blank black pad because its renderer had neither Geist nor
# oklch(). So: render with the real font, and convert the text to outlines so the
# installed SVGs need no font at all.
#
#   GEIST_TTF=/path/to/Geist[wght].ttf packages/pkgbuilds/appsynergy-branding-desktop/scripts/gen-icons.sh
#   (OFL; https://github.com/google/fonts/tree/main/ofl/geist)
#
# Output is committed. Colours are the spec's oklch values converted to sRGB.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/icons/hicolor"
: "${GEIST_TTF:?set GEIST_TTF to the Geist variable font}"
[[ -f $GEIST_TTF ]] || { echo "no such font: $GEIST_TTF"; exit 1; }
command -v rsvg-convert >/dev/null || { echo "need rsvg-convert (librsvg)"; exit 1; }

TMP=$(mktemp -d "$ROOT/.gen-icons.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/fonts"
cp "$GEIST_TTF" "$TMP/fonts/"
cat > "$TMP/fonts.conf" <<XML
<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig><dir>$TMP/fonts</dir><cachedir>$TMP/cache</cachedir></fontconfig>
XML
export FONTCONFIG_FILE="$TMP/fonts.conf"
# Only Geist is visible to the renderer: a missing font must fail, not fall back.
fc-list | grep -q 'Geist' || { echo "fontconfig cannot see Geist in $GEIST_TTF"; exit 1; }

mark='<text x="216" y="720" fill="#4a9bb8" font-family="Geist" font-size="780" font-weight="300">[</text>
  <text x="416" y="690" fill="#f1f1f1" font-family="Geist" font-size="540" font-weight="700" letter-spacing="-21.6">a</text>
  <text x="700" y="720" fill="#4a9bb8" font-family="Geist" font-size="780" font-weight="300">]</text>'
head='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">'
cat > "$TMP/pad.svg" <<XML
$head
  <defs><radialGradient id="bg" cx="50%" cy="35%" r="80%"><stop offset="0%" stop-color="#141b20"/><stop offset="100%" stop-color="#0b0b0b"/></radialGradient></defs>
  <rect width="1024" height="1024" rx="228" fill="url(#bg)"/>
  <circle cx="512" cy="512" r="360" fill="none" stroke="#4a9bb8" stroke-opacity="0.25" stroke-width="2" stroke-dasharray="6 10"/>
  $mark
</svg>
XML
printf '%s\n  %s\n</svg>\n' "$head" "$mark" > "$TMP/mark.svg"

rm -rf "$OUT"
mkdir -p "$OUT/scalable/apps"
# -f svg writes glyphs as outlines: no font needed where these are installed.
for name in appsynergy-linux appsynergy-start; do
  rsvg-convert -f svg -w 1024 -h 1024 "$TMP/pad.svg" -o "$OUT/scalable/apps/$name.svg"
  for s in 16 22 24 32 48 64 128 256 512; do
    mkdir -p "$OUT/${s}x${s}/apps"
    rsvg-convert -w "$s" -h "$s" "$TMP/pad.svg" -o "$OUT/${s}x${s}/apps/$name.png"
  done
done
rsvg-convert -f svg -w 1024 -h 1024 "$TMP/mark.svg" -o "$OUT/scalable/apps/appsynergy-linux-mark.svg"
if grep -l '<text' "$OUT"/scalable/apps/*.svg; then echo "an SVG still carries live text"; exit 1; fi
echo "icons: $(find "$OUT" -type f | wc -l) files in $OUT"
