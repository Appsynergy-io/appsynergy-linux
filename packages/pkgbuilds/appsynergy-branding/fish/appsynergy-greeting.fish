# AppSynergy wordmark — once, brand colors via logo-color + fastfetch config.
status is-interactive; or return
set -q APPSYNERGY_GREETING_DONE; and return
set -gx APPSYNERGY_GREETING_DONE 1

function fish_greeting
end

if not isatty stdout
    return
end

if type -q fastfetch; and test -f /usr/share/appsynergy/fastfetch/config.jsonc
    fastfetch --config /usr/share/appsynergy/fastfetch/config.jsonc
else if type -q appsynergy-banner
    appsynergy-banner
else if type -q neofetch
    neofetch --ascii /usr/share/appsynergy/ascii/logo.txt --ascii_colors 6 7
end
