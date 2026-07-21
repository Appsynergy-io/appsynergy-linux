# AppSynergy Linux
alias ls='ls --color=auto'
alias ll='ls -lah'
export EDITOR="${EDITOR:-vim}"
export HISTCONTROL=ignoredups:erasedups
export HISTSIZE=5000
export HISTFILESIZE=20000
PS1='\[\e[1;36m\]\u@\h\[\e[0m\]:\w\$ '

if [[ $- == *i* && -z ${APPSYNERGY_GREETING_DONE:-} && -t 1 ]]; then
  export APPSYNERGY_GREETING_DONE=1
  if command -v fastfetch >/dev/null 2>&1; then
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
fi
