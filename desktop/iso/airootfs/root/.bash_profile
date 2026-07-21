# AppSynergy live root login
[[ -f ~/.bashrc ]] && . ~/.bashrc
appsynergy-banner 2>/dev/null || true
# Keep archiso automated serial/script hook if present
[[ -x ~/.automated_script.sh ]] && ~/.automated_script.sh
