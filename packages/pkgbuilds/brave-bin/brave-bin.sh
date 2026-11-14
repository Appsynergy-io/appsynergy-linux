#!/usr/bin/bash
# Launcher: optional per-user flags, one per line, in ~/.config/brave-flags.conf.
conf="${XDG_CONFIG_HOME:-$HOME/.config}/brave-flags.conf"
flags=()
if [[ -f $conf ]]; then
  while IFS= read -r line; do
    [[ $line =~ ^[[:space:]]*(#|$) ]] || flags+=("$line")
  done < "$conf"
fi
export CHROME_VERSION_EXTRA='stable'
exec /opt/brave-bin/brave "${flags[@]}" "$@"
