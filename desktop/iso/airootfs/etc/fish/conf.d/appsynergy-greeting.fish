status is-interactive; or exit 0
set -q APPSYNERGY_GREETING_DONE; and exit 0
set -gx APPSYNERGY_GREETING_DONE 1
isatty stdout; or exit 0

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
