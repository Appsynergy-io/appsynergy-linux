# AppSynergy interactive greeting — brand colors, vertically centered logo
[[ $- == *i* ]] || return 0
[[ -n ${APPSYNERGY_GREETING_DONE:-} ]] && return 0
[[ -n ${SSH_ORIGINAL_COMMAND:-} ]] && return 0
[[ -t 1 ]] || return 0
export APPSYNERGY_GREETING_DONE=1
if command -v fastfetch >/dev/null 2>&1 && [[ -f /usr/share/appsynergy/fastfetch/config.jsonc ]]; then
  fastfetch --config /usr/share/appsynergy/fastfetch/config.jsonc --logo-padding-top 8
elif command -v appsynergy-banner >/dev/null 2>&1; then
  appsynergy-banner
fi
