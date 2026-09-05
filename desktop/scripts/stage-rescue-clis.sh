#!/usr/bin/bash
# Copy Grok + Claude CLIs into the live airootfs for rescue work.
# Run from build-iso.sh. Binaries are NOT committed to git (~400MB).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/iso/airootfs/usr/local/bin"
DOC="$ROOT/iso/airootfs/etc/appsynergy/RESCUE-CLI.txt"
mkdir -p "$DEST" "$(dirname "$DOC")"

# When build-iso.sh runs under sudo, $HOME is /root. Use SUDO_USER's home,
# else the caller's; GROK_BIN / CLAUDE_BIN override the lookup entirely.
if [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
  UHOME=$(getent passwd "$SUDO_USER" | cut -d: -f6)
else
  UHOME="${HOME:-}"
fi
[[ -n "$UHOME" && -d "$UHOME" ]] || { echo "ERROR: cannot resolve the build user's home"; exit 1; }

GROK_SRC="${GROK_BIN:-}"
if [[ -z "$GROK_SRC" ]]; then
  if [[ -L "$UHOME/.grok/bin/grok" ]]; then
    GROK_SRC=$(readlink -f "$UHOME/.grok/bin/grok")
  else
    # newest grok-* binary in downloads
    GROK_SRC=$(ls -1t "$UHOME"/.grok/downloads/grok-*-linux-x86_64 2>/dev/null | head -1 || true)
  fi
fi
CLAUDE_SRC="${CLAUDE_BIN:-}"
if [[ -z "$CLAUDE_SRC" ]]; then
  if [[ -L "$UHOME/.bun/bin/claude" ]]; then
    CLAUDE_SRC=$(readlink -f "$UHOME/.bun/bin/claude")
  elif [[ -x "$UHOME/.bun/install/global/node_modules/@anthropic-ai/claude-code-linux-x64/claude" ]]; then
    CLAUDE_SRC="$UHOME/.bun/install/global/node_modules/@anthropic-ai/claude-code-linux-x64/claude"
  fi
fi

ok=0
if [[ -n "$GROK_SRC" && -x "$GROK_SRC" ]]; then
  install -m755 "$GROK_SRC" "$DEST/grok"
  ln -sfn grok "$DEST/agent"
  echo "  staged grok ($(du -h "$DEST/grok" | awk '{print $1}')) from $GROK_SRC"
  ok=1
elif [[ -x "$DEST/grok" ]]; then
  chmod 755 "$DEST/grok" "$DEST/agent" 2>/dev/null || chmod 755 "$DEST/grok"
  echo "  keep existing staged grok"
  ok=1
else
  echo "WARN: grok binary not found (tried: ${GROK_SRC:-none})"
fi

if [[ -n "$CLAUDE_SRC" && -x "$CLAUDE_SRC" ]]; then
  install -m755 "$CLAUDE_SRC" "$DEST/claude"
  echo "  staged claude ($(du -h "$DEST/claude" | awk '{print $1}')) from $CLAUDE_SRC"
  ok=1
elif [[ -x "$DEST/claude" ]]; then
  chmod 755 "$DEST/claude"
  echo "  keep existing staged claude"
  ok=1
else
  echo "WARN: claude binary not found (tried: ${CLAUDE_SRC:-none})"
fi

cat > "$DOC" <<'EOF'
Rescue CLIs on this live USB
============================

  grok     — Grok Build CLI (also linked as `agent`)
  claude   — Claude Code CLI

Both need network + your own API auth. This image does NOT ship API keys.

Grok:   run `grok` then complete login / set auth as usual for your install
Claude: run `claude` and authenticate when prompted

Use for fixing a broken install (chroot, edit configs, diagnose LUKS) — not as a daily driver.
EOF

[[ "$ok" -eq 1 ]] || echo "WARN: no rescue CLIs staged"
