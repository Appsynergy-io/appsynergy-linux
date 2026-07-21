# AppSynergy interactive greeting — wordmark via fastfetch/neofetch/banner
[[ $- == *i* ]] || return 0
[[ -n ${APPSYNERGY_GREETING_DONE:-} ]] && return 0
[[ -n ${SSH_ORIGINAL_COMMAND:-} ]] && return 0
[[ -t 1 ]] || return 0

export APPSYNERGY_GREETING_DONE=1

if command -v fastfetch >/dev/null 2>&1; then
  # Prefer packaged config if user has none
  if [[ ! -f ${XDG_CONFIG_HOME:-$HOME/.config}/fastfetch/config.jsonc \
     && -f /usr/share/appsynergy/fastfetch/config.jsonc ]]; then
    fastfetch --config /usr/share/appsynergy/fastfetch/config.jsonc
  else
    fastfetch
  fi
elif command -v neofetch >/dev/null 2>&1; then
  neofetch --ascii /usr/share/appsynergy/ascii/logo.txt --ascii_colors 6 7 6 6 6 7
elif command -v appsynergy-banner >/dev/null 2>&1; then
  appsynergy-banner
fi
