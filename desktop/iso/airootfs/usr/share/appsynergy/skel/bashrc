# AppSynergy Linux
alias ls='ls --color=auto'
alias ll='ls -lah'
export EDITOR="${EDITOR:-vim}"
export HISTCONTROL=ignoredups:erasedups
export HISTSIZE=5000
export HISTFILESIZE=20000
PS1='\[\e[1;38;2;74;155;184m\]\u@\h\[\e[0m\]:\w\$ '

if [[ $- == *i* && -z ${APPSYNERGY_GREETING_DONE:-} && -t 1 ]]; then
  export APPSYNERGY_GREETING_DONE=1
  if command -v fastfetch >/dev/null 2>&1 && [[ -f /usr/share/appsynergy/fastfetch/config.jsonc ]]; then
    fastfetch --config /usr/share/appsynergy/fastfetch/config.jsonc
  elif command -v appsynergy-banner >/dev/null 2>&1; then
    appsynergy-banner
  fi
fi
