#!/usr/bin/bash
# Generate the Plymouth theme from the real logo master.
#
# The logo is `[ appsynergy          ]`: the lowercase wordmark set hard left inside
# wide brackets, with the rest of the slot empty. The first theme re-typed the word
# in DejaVu Sans, centred it and tinted it teal — a lookalike, not the logo. This
# one never re-renders the wordmark: watermark.png is the master, scaled.
#
# The logo is the logo: nothing is drawn on it, in it or over it. All motion lives
# on a hairline well below the brackets — a short teal run gliding along a faint
# track the width of the logo, which fills the track when boot completes. Line
# frames are taller than the logo but share its centre and alignment, so the line
# keeps the same distance from the logo at every resolution; during the LUKS
# prompt two-step hides the line and the logo stays put above the passphrase field.
#
# Output is committed (loose files are canonical; make-srctars.sh packs them).
# Needs ffmpeg (lanczos downscale with alpha) and rsvg-convert.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PLY="$ROOT/plymouth/appsynergy"
MASTER="$ROOT/plymouth-src/logo-fira-2k.png"   # 2800x560 RGBA, light wordmark + teal brackets

command -v rsvg-convert >/dev/null || { echo "need rsvg-convert (librsvg)"; exit 1; }
command -v ffmpeg >/dev/null || { echo "need ffmpeg"; exit 1; }
command -v python3 >/dev/null || { echo "need python3"; exit 1; }
[[ -f $MASTER ]] || { echo "missing logo master: $MASTER"; exit 1; }

TMP=$(mktemp -d "$ROOT/.gen-plymouth.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$PLY"
# Frame counts changed between themes; a stale higher-numbered frame would still
# be loaded and played.
rm -f "$PLY"/animation-*.png "$PLY"/throbber-*.png "$PLY"/logo.png

W=800 H=160          # 2/7 of the master
ffmpeg -y -loglevel error -i "$MASTER" -vf "scale=$W:$H:flags=lanczos" -pix_fmt rgba \
  -frames:v 1 "$PLY/watermark.png"
# The session splash (look-and-feel) shows the same logo; 2x for HiDPI panels.
KSPLASH="$ROOT/lookandfeel/io.appsynergy.desktop/contents/splash/images"
mkdir -p "$KSPLASH"
ffmpeg -y -loglevel error -i "$MASTER" -vf "scale=1600:320:flags=lanczos" -pix_fmt rgba \
  -frames:v 1 "$KSPLASH/logo.png"

python3 -I - "$PLY" "$TMP" "$W" "$H" <<'PY'
import math, subprocess, sys
ply, tmp, W, H = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
TEAL, INK, FIELD = "#4a9bb8", "#f0f0f0", "#10161c"

def render(name, svg, w, h):
    src = f"{tmp}/{name}.svg"
    open(src, "w").write('<?xml version="1.0" encoding="UTF-8"?>\n' + svg)
    subprocess.check_call(["rsvg-convert", "-w", str(w), "-h", str(h), src, "-o", f"{ply}/{name}.png"])

# The line: FH is the frame height, centred on the logo, so the logo occupies
# FH/2 +- H/2 and the track sits a clear 44 px under its lowest pixel.
FH = 2 * (H // 2 + 46)
Y = FH - 2                     # track centre line
X0, X1 = 22, 778               # outer edges of the two brackets
RUNLEN = 132

def line(a, b, glow=1.0):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {FH}" width="{W}" height="{FH}">'
            f'<path d="M {X0} {Y} H {X1}" stroke="{TEAL}" stroke-opacity="0.16" stroke-width="2"/>'
            f'<path d="M {a:.2f} {Y} H {b:.2f}" stroke="{TEAL}" stroke-opacity="{glow:.4f}" '
            f'stroke-width="2"/></svg>')

def ease(t):
    return t * t * (3 - 2 * t)

# Boot: one pass in 1.8 s at Plymouth's 30 fps. The run enters and leaves through
# the track ends (clipped, not faded), so the loop has no visible seam.
N = 54
for i in range(N):
    head = X0 + (X1 - X0 + RUNLEN) * ease(i / (N - 1))
    render(f"throbber-{i + 1:04d}", line(max(X0, head - RUNLEN), min(X1, head)), W, FH)

# Boot complete: the track fills from the left, then holds.
RUN, HOLD = 22, 10
for i in range(RUN + HOLD):
    render(f"animation-{i + 1:04d}", line(X0, X0 + (X1 - X0) * ease(min(i / (RUN - 1), 1.0))), W, FH)

# Passphrase dialog: square-shouldered like the brackets, same two colours.
render("entry", f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 340 44">'
       f'<rect x="1" y="1" width="338" height="42" rx="2" fill="{FIELD}" stroke="{TEAL}" '
       f'stroke-opacity="0.55" stroke-width="1.5"/></svg>', 340, 44)
render("bullet", f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14">'
       f'<rect x="3" y="3" width="8" height="8" rx="1" fill="{INK}"/></svg>', 14, 14)
render("lock", f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">'
       f'<rect x="8" y="14" width="16" height="12" rx="1.5" fill="none" stroke="{TEAL}" stroke-width="2"/>'
       f'<path d="M11.5 14 V11 a4.5 4.5 0 0 1 9 0 v3" fill="none" stroke="{TEAL}" stroke-width="2"/></svg>', 32, 32)
key = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">'
       f'<rect x="4" y="10" width="24" height="13" rx="1.5" fill="none" stroke="{TEAL}" stroke-width="2"/>'
       f'<path d="M9 19 H23" stroke="{TEAL}" stroke-width="2"/>%s</svg>')
render("keyboard", key % "", 32, 32)
render("capslock", key % f'<path d="M16 3 L21 8 H11 Z" fill="{TEAL}"/>', 32, 32)
PY

cat > "$PLY/appsynergy.plymouth" <<EOF
[Plymouth Theme]
Name=appsynergy
Description=appsynergy boot splash: the logo, untouched, over a gliding hairline
ModuleName=two-step

[two-step]
Font=DejaVu Sans 11
TitleFont=DejaVu Sans 24
MonospaceFont=DejaVu Sans Mono 11
ImageDir=/usr/share/plymouth/themes/appsynergy
# Logo (watermark) and line (throbber/animation) share one alignment and one
# centre, which is what holds the line a fixed distance under the logo.
WatermarkHorizontalAlignment=.5
WatermarkVerticalAlignment=.44
HorizontalAlignment=.5
VerticalAlignment=.44
DialogHorizontalAlignment=.5
DialogVerticalAlignment=.64
TitleHorizontalAlignment=.5
TitleVerticalAlignment=.26
Transition=none
TransitionDuration=0.0
BackgroundStartColor=0x10161c
BackgroundEndColor=0x080b0e
ProgressBarBackgroundColor=0x1c252d
ProgressBarForegroundColor=0x4a9bb8
DialogClearsFirmwareBackground=true
MessageBelowAnimation=true

[boot-up]
UseEndAnimation=true
UseFirmwareBackground=false

[shutdown]
UseEndAnimation=false
UseFirmwareBackground=false

[reboot]
UseEndAnimation=false
UseFirmwareBackground=false

[updates]
SuppressMessages=true
ProgressBarShowPercentComplete=true
UseProgressBar=true
Title=Installing updates
SubTitle=Do not turn off your computer

[system-upgrade]
SuppressMessages=true
ProgressBarShowPercentComplete=true
UseProgressBar=true
Title=Upgrading system
SubTitle=Do not turn off your computer
EOF

echo "plymouth theme: $(find "$PLY" -type f | wc -l) files in $PLY"
