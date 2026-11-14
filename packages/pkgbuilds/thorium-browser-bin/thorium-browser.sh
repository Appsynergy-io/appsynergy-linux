#!/usr/bin/bash
# Launcher: optional per-user flags, one per line, in ~/.config/thorium-flags.conf.
conf="${XDG_CONFIG_HOME:-$HOME/.config}/thorium-flags.conf"
flags=()
if [[ -f $conf ]]; then
  while IFS= read -r line; do
    [[ $line =~ ^[[:space:]]*(#|$) ]] || flags+=("$line")
  done < "$conf"
fi
exec /opt/thorium-browser/thorium-browser "${flags[@]}" "$@"
