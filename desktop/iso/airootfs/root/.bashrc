# shellcheck shell=bash
alias ls="ls --color=auto"
alias ll="ls -la"
export EDITOR=vim
export HISTCONTROL=ignoredups
PS1="\[\e[1;36m\]appsynergy-live\[\e[0m\]:\w\# "

if [[ \$- == *i* && -z \${APPSYNERGY_GREETING_DONE:-} && -t 1 ]]; then
  export APPSYNERGY_GREETING_DONE=1
  if command -v fastfetch >/dev/null 2>&1; then
    fastfetch --config /usr/share/appsynergy/fastfetch/config.jsonc 2>/dev/null || fastfetch
  elif command -v appsynergy-banner >/dev/null 2>&1; then
    appsynergy-banner
  fi
fi
