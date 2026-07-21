# AppSynergy wordmark — once per interactive shell.
# Use `return` (not `exit`) so conf.d never kills the shell.
status is-interactive; or return
set -q APPSYNERGY_GREETING_DONE; and return
set -gx APPSYNERGY_GREETING_DONE 1

# Suppress default / vendor fish_greeting (e.g. CachyOS fastfetch).
# If config.fish redefines fish_greeting later, disable that vendor greeting there.
function fish_greeting
end

if not isatty stdout
    return
end

if type -q fastfetch
    if not test -f "$HOME/.config/fastfetch/config.jsonc"; and test -f /usr/share/appsynergy/fastfetch/config.jsonc
        fastfetch --config /usr/share/appsynergy/fastfetch/config.jsonc
    else
        fastfetch
    end
else if type -q neofetch
    neofetch --ascii /usr/share/appsynergy/ascii/logo.txt --ascii_colors 6 7 6 6 6 7
else if type -q appsynergy-banner
    appsynergy-banner
end
