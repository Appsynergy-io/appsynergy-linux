#!/usr/bin/bash
# Backup workstation essentials to BACKUP USB before AppSynergy reimage.
# Defaults: no NativeLink; include Brave + latest ISO; slim Grok/Claude.
set -euo pipefail

DEST="${DEST:-/run/media/imma/BACKUP}"
HOME_U="${HOME_U:-/home/imma}"
PROJECTS="$HOME_U/projects"
TS=$(date -Iseconds)
LOG="$DEST/05-manifest/backup.log"

die() { echo "ERROR: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null || die "missing $1"; }

need rsync
need find
[[ -d "$DEST" ]] || die "DEST not mounted: $DEST"
touch "$DEST/.write-test" 2>/dev/null || die "DEST not writable: $DEST"
rm -f "$DEST/.write-test"

mkdir -p "$DEST"/{00-secrets,01-projects,02-agents/grok,02-agents/claude,03-desktop,04-optional/{iso,brave},05-manifest}

log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG"; }

log "=== backup start $TS ==="
log "DEST=$DEST"

# --- package lists ---
pacman -Qqe >"$DEST/05-manifest/host-explicit-packages.txt" 2>/dev/null || true
pacman -Qq >"$DEST/05-manifest/host-all-packages.txt" 2>/dev/null || true
log "package lists written"

# --- 00 secrets ---
log "==> 00-secrets"
rsync -aH --info=stats2 \
  "$HOME_U/.ssh/" "$DEST/00-secrets/ssh/" 2>/dev/null || true
rsync -aH --info=stats2 \
  "$HOME_U/.gnupg/" "$DEST/00-secrets/gnupg/" 2>/dev/null || true
rsync -aH --info=stats2 \
  "$HOME_U/.config/appsynergy/" "$DEST/00-secrets/config-appsynergy/" 2>/dev/null || true
rsync -aH --info=stats2 \
  "$HOME_U/.config/tea/" "$DEST/00-secrets/config-tea/" 2>/dev/null || true
rsync -aH --info=stats2 \
  "$HOME_U/.config/gh/" "$DEST/00-secrets/config-gh/" 2>/dev/null || true
[[ -f "$HOME_U/.gitconfig" ]] && cp -a "$HOME_U/.gitconfig" "$DEST/00-secrets/"
[[ -d "$PROJECTS/secrets" ]] && rsync -aH "$PROJECTS/secrets/" "$DEST/00-secrets/projects-secrets/"
[[ -f "$PROJECTS/mellify.key" ]] && cp -a "$PROJECTS/mellify.key" "$DEST/00-secrets/"
# WireGuard (root)
if [[ -d /etc/wireguard ]]; then
  mkdir -p "$DEST/00-secrets/etc-wireguard"
  sudo rsync -aH /etc/wireguard/ "$DEST/00-secrets/etc-wireguard/" 2>/dev/null || true
  sudo chown -R "$(id -u):$(id -g)" "$DEST/00-secrets/etc-wireguard" 2>/dev/null || true
fi
log "secrets done"

# --- 01 projects (git trees, no build artifacts / worktrees) ---
log "==> 01-projects"
# Exclude disposable / regenerable bulk
RSYNC_EX=(
  --exclude='target/'
  --exclude='**/target/'
  --exclude='node_modules/'
  --exclude='.venv/'
  --exclude='__pycache__/'
  --exclude='work/'
  --exclude='out/*.iso'
  --exclude='*-wt-*/'
  --exclude='gate-ru-wt-*/'
  --exclude='gate-ru-integ-*/'
  --exclude='appsynergy-rs-wt-*/'
  --exclude='buildroot/'
  --exclude='transcript-backups/'
  --exclude='-home-imma-projects/'
)

# Prefer explicit allow-list of real projects + anything not matched by exclude names
# Copy entire projects tree with excludes (drops wt dirs by name pattern via --exclude)
rsync -aH --delete --info=progress2 \
  "${RSYNC_EX[@]}" \
  "$PROJECTS/" "$DEST/01-projects/" \
  | tee -a "$LOG" || true

# Git status snapshot for restore awareness
{
  echo "# git status snapshot $TS"
  for d in "$PROJECTS"/*/; do
    [[ -d "$d/.git" ]] || continue
    name=$(basename "$d")
    case "$name" in
      *-wt-*|gate-ru-wt-*|gate-ru-integ-*|appsynergy-rs-wt-*|buildroot|transcript-backups) continue ;;
    esac
    echo "=== $name ==="
    git -C "$d" remote -v 2>/dev/null | head -5
    git -C "$d" status -sb 2>/dev/null
    git -C "$d" stash list 2>/dev/null | head -5
    echo
  done
} >"$DEST/05-manifest/git-status-snapshot.txt" 2>/dev/null || true
log "projects done"

# --- 02 agents: Grok slim ---
log "==> 02-agents/grok"
GROK_SRC="$HOME_U/.grok"
GROK_DST="$DEST/02-agents/grok"
mkdir -p "$GROK_DST"

# configs + identity
for f in auth.json config.toml trusted_folders.toml agent_id AGENTS.md \
         active_sessions.json CHANGELOG.md README.md version.json \
         models_cache.json slash-mru.json; do
  [[ -e "$GROK_SRC/$f" ]] && cp -a "$GROK_SRC/$f" "$GROK_DST/"
done
rsync -aH "$GROK_SRC/rules/" "$GROK_DST/rules/" 2>/dev/null || true
rsync -aH "$GROK_SRC/skills/" "$GROK_DST/skills/" 2>/dev/null || true
# custom bin wrappers if any (not full vendor)
[[ -d "$GROK_SRC/bin" ]] && rsync -aH --max-size=50M "$GROK_SRC/bin/" "$GROK_DST/bin/" 2>/dev/null || true

# Latest session per primary project cwd only (skip .grok/worktrees paths)
mkdir -p "$GROK_DST/sessions"
SESS_ROOT="$GROK_SRC/sessions"
if [[ -d "$SESS_ROOT" ]]; then
  # Primary project session folders: URL-encoded /home/imma/projects/<name>
  # shellcheck disable=SC2010
  while IFS= read -r -d '' dir; do
    base=$(basename "$dir")
    # skip nested paths that encode subdirs under a project (contain extra %2F after project name)
    # keep only exactly: %2Fhome%2Fimma%2Fprojects%2F<single-segment>
    if [[ "$base" =~ ^%2Fhome%2Fimma%2Fprojects%2F[^%]+$ ]]; then
      :
    elif [[ "$base" == "%2Fhome%2Fimma" ]] || [[ "$base" == "%2Fhome%2Fimma%2Fprojects" ]]; then
      :
    else
      continue
    fi
    # newest session subdirectory
    latest=$(find "$dir" -mindepth 1 -maxdepth 1 -type d -printf '%T@ %p\n' 2>/dev/null | sort -nr | head -1 | cut -d' ' -f2-)
    if [[ -n "${latest:-}" && -d "$latest" ]]; then
      dest_sess="$GROK_DST/sessions/$base/$(basename "$latest")"
      mkdir -p "$(dirname "$dest_sess")"
      rsync -aH "$latest/" "$dest_sess/"
      log "  grok session: $base -> $(basename "$latest")"
    fi
  done < <(find "$SESS_ROOT" -mindepth 1 -maxdepth 1 -type d -print0)
fi
log "grok slim done ($(du -sh "$GROK_DST" | cut -f1))"

# --- 02 agents: Claude slim ---
log "==> 02-agents/claude"
CL_SRC="$HOME_U/.claude"
CL_DST="$DEST/02-agents/claude"
mkdir -p "$CL_DST"

for f in .credentials.json settings.json settings.local.json settings.json.bak \
         CLAUDE.md CODE.md history.jsonl; do
  [[ -e "$CL_SRC/$f" ]] && cp -a "$CL_SRC/$f" "$CL_DST/"
done
[[ -d "$HOME_U/.claude-hooks-local" ]] && rsync -aH "$HOME_U/.claude-hooks-local/" "$CL_DST/claude-hooks-local/"

# memories + latest jsonl per project
if [[ -d "$CL_SRC/projects" ]]; then
  while IFS= read -r -d '' pdir; do
    rel=${pdir#"$CL_SRC/projects/"}
    # skip tmp e2e
    [[ "$rel" == -tmp-* ]] && continue
    mkdir -p "$CL_DST/projects/$rel"
    if [[ -d "$pdir/memory" ]]; then
      rsync -aH "$pdir/memory/" "$CL_DST/projects/$rel/memory/"
    fi
    # latest .jsonl in project root
    latest=$(find "$pdir" -maxdepth 1 -type f -name '*.jsonl' -printf '%T@ %p\n' 2>/dev/null | sort -nr | head -1 | cut -d' ' -f2-)
    if [[ -n "${latest:-}" && -f "$latest" ]]; then
      cp -a "$latest" "$CL_DST/projects/$rel/"
      log "  claude session: $rel -> $(basename "$latest")"
    fi
  done < <(find "$CL_SRC/projects" -mindepth 1 -maxdepth 1 -type d -print0)
fi
# small plugins metadata (not full cache)
[[ -d "$CL_SRC/plugins" ]] && rsync -aH --exclude='cache/' --exclude='marketplaces/' \
  "$CL_SRC/plugins/" "$CL_DST/plugins/" 2>/dev/null || true
log "claude slim done ($(du -sh "$CL_DST" | cut -f1))"

# --- 03 desktop ---
log "==> 03-desktop"
mkdir -p "$DEST/03-desktop"
rsync -aH "$HOME_U/.config/fish/" "$DEST/03-desktop/fish/" 2>/dev/null || true
[[ -f "$HOME_U/.config/kscreenlockerrc" ]] && cp -a "$HOME_U/.config/kscreenlockerrc" "$DEST/03-desktop/"
rsync -aH "$HOME_U/.local/share/wallpapers/" "$DEST/03-desktop/wallpapers/" 2>/dev/null || true
# selective plasma / kickoff favorites fix already in live config
for f in plasma-org.kde.plasma.desktop-appletsrc powerdevilrc kscreenlockerrc \
         kdeglobals kwinrc kglobalshortcutsrc; do
  [[ -f "$HOME_U/.config/$f" ]] && cp -a "$HOME_U/.config/$f" "$DEST/03-desktop/"
done
# VS Code user settings if present
[[ -d "$HOME_U/.config/Code - OSS/User" ]] && \
  rsync -aH "$HOME_U/.config/Code - OSS/User/" "$DEST/03-desktop/code-oss-user/" 2>/dev/null || true
[[ -d "$HOME_U/.config/Code/User" ]] && \
  rsync -aH "$HOME_U/.config/Code/User/" "$DEST/03-desktop/code-user/" 2>/dev/null || true
log "desktop done"

# --- 04 optional: Brave + ISO ---
log "==> 04-optional/brave"
if [[ -d "$HOME_U/.config/BraveSoftware" ]]; then
  rsync -aH --info=progress2 \
    --exclude='**/Cache/' \
    --exclude='**/Code Cache/' \
    --exclude='**/GPUCache/' \
    --exclude='**/Service Worker/CacheStorage/' \
    --exclude='**/GrShaderCache/' \
    --exclude='**/ShaderCache/' \
    "$HOME_U/.config/BraveSoftware/" "$DEST/04-optional/brave/BraveSoftware/" \
    | tee -a "$LOG" || true
fi

log "==> 04-optional/iso"
ISO=$(ls -1t "$PROJECTS/appsynergy-linux/desktop/out"/appsynergy-linux-*.iso 2>/dev/null | head -1 || true)
if [[ -n "${ISO:-}" && -f "$ISO" ]]; then
  rsync -aH --info=progress2 "$ISO" "$DEST/04-optional/iso/" | tee -a "$LOG"
  log "  iso: $(basename "$ISO")"
else
  log "  no ISO found under appsynergy-linux/desktop/out"
fi

# CLI locations note (reinstall preferred)
{
  echo "grok_bin=$(command -v grok || true)"
  echo "claude_bin=$(command -v claude || true)"
  ls -la "$(command -v grok 2>/dev/null)" 2>/dev/null || true
  ls -la "$(command -v claude 2>/dev/null)" 2>/dev/null || true
} >"$DEST/05-manifest/cli-paths.txt"

# --- MANIFEST + RESTORE ---
{
  echo "AppSynergy reimage backup"
  echo "created: $TS"
  echo "host: $(hostname) $(uname -r)"
  echo "dest: $DEST"
  echo
  echo "=== sizes ==="
  du -sh "$DEST"/* 2>/dev/null
  echo
  echo "=== total ==="
  du -sh "$DEST"
} | tee "$DEST/MANIFEST.txt" | tee -a "$LOG"

cat >"$DEST/RESTORE.md" <<'EOF'
# Restore after AppSynergy install

## Order
1. Install from USB ISO (`appsynergy-install`), reboot, LUKS unlock, login.
2. Mount this BACKUP USB.
3. Restore secrets first, then projects, then agents, then optional.

## Secrets
```fish
set B /run/media/imma/BACKUP
cp -a $B/00-secrets/ssh/. ~/.ssh/
chmod 700 ~/.ssh; chmod 600 ~/.ssh/* 2>/dev/null
cp -a $B/00-secrets/config-appsynergy/. ~/.config/appsynergy/
cp -a $B/00-secrets/config-tea/. ~/.config/tea/
cp -a $B/00-secrets/gitconfig ~/.gitconfig 2>/dev/null
cp -a $B/00-secrets/mellify.key ~/projects/ 2>/dev/null
mkdir -p ~/projects; cp -a $B/00-secrets/projects-secrets/. ~/projects/secrets/
# WireGuard (if present): sudo cp -a $B/00-secrets/etc-wireguard/. /etc/wireguard/
```

## Projects
```fish
set B /run/media/imma/BACKUP
mkdir -p ~/projects
rsync -aH $B/01-projects/ ~/projects/
# Or re-clone remotes from git-status-snapshot.txt and only copy local-only bits
```

## Grok
```fish
set B /run/media/imma/BACKUP
# Reinstall grok CLI first, then:
mkdir -p ~/.grok
rsync -aH $B/02-agents/grok/ ~/.grok/
# Resume: cd ~/projects/<name> && grok  # then resume latest session from UI/history
```

## Claude
```fish
set B /run/media/imma/BACKUP
# Reinstall claude CLI first, then:
mkdir -p ~/.claude
rsync -aH $B/02-agents/claude/ ~/.claude/
# memories under ~/.claude/projects/*/memory ; latest jsonl per project
```

## Desktop
```fish
set B /run/media/imma/BACKUP
rsync -aH $B/03-desktop/fish/ ~/.config/fish/
cp -a $B/03-desktop/kscreenlockerrc ~/.config/ 2>/dev/null
mkdir -p ~/.local/share/wallpapers
rsync -aH $B/03-desktop/wallpapers/ ~/.local/share/wallpapers/
# optional plasma configs from 03-desktop/
```

## Brave (optional)
```fish
# fully quit Brave first
rsync -aH $B/04-optional/brave/BraveSoftware/ ~/.config/BraveSoftware/
```

## Not on this stick (by design)
- /opt/nativelink (53G) — restore later if needed
- CUDA/NVIDIA stack
- cargo target/, rustup, npm caches — regenerate
- kernel src tree — packages on appsynergy repo / rebuild

## Post-install software (already mostly on ISO)
See packages-target on installed system. Then: reinstall grok + claude CLIs, tea auth, cosign key, optional NativeLink.
EOF

log "=== backup complete ==="
du -sh "$DEST"/* | tee -a "$LOG"
du -sh "$DEST" | tee -a "$LOG"
