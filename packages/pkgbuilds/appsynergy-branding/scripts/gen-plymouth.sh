#!/usr/bin/bash
# Generate Plymouth theme assets: wordmark pulse + arc throbber (NO hub-spoke).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PLY="$ROOT/plymouth/appsynergy"
TEAL="#4a9bb8"
mkdir -p "$PLY"

command -v rsvg-convert >/dev/null || { echo "need rsvg-convert (librsvg)"; exit 1; }
command -v python3 >/dev/null || { echo "need python3"; exit 1; }

python3 - "$PLY" "$TEAL" <<'PY'
import math, subprocess, sys
ply, teal = sys.argv[1], sys.argv[2]

def write_svg(path, body):
    open(path, "w").write(body)

def rsvg(src, out, w, h):
    subprocess.check_call(["rsvg-convert", "-w", str(w), "-h", str(h), src, "-o", out])

# logo / animation base (wordmark in brackets)
def wordmark_svg(opacity=1.0):
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect width="512" height="512" fill="none"/>
  <g transform="translate(0,176)" opacity="{opacity:.4f}">
    <path d="M 72 24 L 52 24 L 52 136 L 72 136" fill="none" stroke="{teal}" stroke-width="9" stroke-linecap="square"/>
    <path d="M 440 24 L 460 24 L 460 136 L 440 136" fill="none" stroke="{teal}" stroke-width="9" stroke-linecap="square"/>
    <text x="256" y="100" text-anchor="middle" font-family="DejaVu Sans, Liberation Sans, sans-serif" font-size="52" font-weight="600" fill="{teal}" letter-spacing="1.5">appsynergy</text>
  </g>
</svg>
"""

write_svg("/tmp/as-logo.svg", wordmark_svg(1.0))
rsvg("/tmp/as-logo.svg", f"{ply}/logo.png", 512, 512)

# watermark wide
wm = f"""<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 384 96" width="384" height="96">
  <path d="M 36 16 L 24 16 L 24 80 L 36 80" fill="none" stroke="{teal}" stroke-width="5" stroke-linecap="square"/>
  <path d="M 348 16 L 360 16 L 360 80 L 348 80" fill="none" stroke="{teal}" stroke-width="5" stroke-linecap="square"/>
  <text x="192" y="60" text-anchor="middle" font-family="DejaVu Sans, Liberation Sans, sans-serif" font-size="32" font-weight="600" fill="{teal}">appsynergy</text>
</svg>
"""
write_svg("/tmp/as-wm.svg", wm)
rsvg("/tmp/as-wm.svg", f"{ply}/watermark.png", 384, 96)

for i in range(24):
    t = i / 24.0
    opacity = 0.35 + 0.65 * (0.5 - 0.5 * math.cos(2 * math.pi * t))
    write_svg(f"/tmp/as-anim-{i:04d}.svg", wordmark_svg(opacity))
    rsvg(f"/tmp/as-anim-{i:04d}.svg", f"{ply}/animation-{i+1:04d}.png", 512, 512)

for i in range(18):
    ang = i * 20
    thr = f"""<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96" width="96" height="96">
  <rect width="96" height="96" fill="none"/>
  <g transform="rotate({ang} 48 48)">
    <circle cx="48" cy="48" r="30" fill="none" stroke="{teal}" stroke-opacity="0.15" stroke-width="6"/>
    <path d="M 48 18 A 30 30 0 0 1 78 48" fill="none" stroke="{teal}" stroke-width="6" stroke-linecap="round"/>
  </g>
</svg>
"""
    write_svg(f"/tmp/as-thr-{i:04d}.svg", thr)
    rsvg(f"/tmp/as-thr-{i:04d}.svg", f"{ply}/throbber-{i+1:04d}.png", 96, 96)

# chrome
for name, svg, wh in [
    ("bullet", f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><circle cx="8" cy="8" r="4" fill="{teal}"/></svg>', (16, 16)),
    ("entry", f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 48"><rect x="1" y="1" width="318" height="46" rx="6" fill="#12151c" stroke="{teal}" stroke-opacity="0.5" stroke-width="2"/></svg>', (320, 48)),
    ("lock", f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect x="8" y="14" width="16" height="12" rx="2" fill="none" stroke="{teal}" stroke-width="2"/><path d="M11 14 V11 a5 5 0 0 1 10 0 v3" fill="none" stroke="{teal}" stroke-width="2"/></svg>', (32, 32)),
    ("keyboard", f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect x="4" y="10" width="24" height="14" rx="2" fill="none" stroke="{teal}" stroke-width="2"/></svg>', (32, 32)),
]:
    p = f"/tmp/as-{name}.svg"
    write_svg(p, f'<?xml version="1.0"?>{svg}')
    rsvg(p, f"{ply}/{name}.png", wh[0], wh[1])

import shutil
shutil.copy(f"{ply}/keyboard.png", f"{ply}/capslock.png")
print("plymouth assets OK in", ply)
PY

cat > "$PLY/appsynergy.plymouth" <<EOF
[Plymouth Theme]
Name=appsynergy
Description=AppSynergy boot splash — wordmark pulse (no hub-spoke mark)
ModuleName=two-step

[two-step]
Font=DejaVu Sans 12
TitleFont=DejaVu Sans 28
MonospaceFont=DejaVu Sans Mono 12
ImageDir=/usr/share/plymouth/themes/appsynergy
DialogHorizontalAlignment=.5
DialogVerticalAlignment=.45
TitleHorizontalAlignment=.5
TitleVerticalAlignment=.38
HorizontalAlignment=.5
VerticalAlignment=.55
WatermarkHorizontalAlignment=.5
WatermarkVerticalAlignment=.92
Transition=none
TransitionDuration=0.0
BackgroundStartColor=0x0b0e11
BackgroundEndColor=0x0b0e11
ProgressBarBackgroundColor=0x2a333c
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
Title=Installing Updates...
SubTitle=Do not turn off your computer

[system-upgrade]
SuppressMessages=true
ProgressBarShowPercentComplete=true
UseProgressBar=true
Title=Upgrading System...
SubTitle=Do not turn off your computer
EOF

echo "done: $PLY"
ls "$PLY" | wc -l
