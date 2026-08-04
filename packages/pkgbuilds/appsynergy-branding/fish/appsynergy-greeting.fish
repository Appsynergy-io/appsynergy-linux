# AppSynergy wordmark — once; brand teal; vertically padded via fastfetch config.
status is-interactive; or return
set -q APPSYNERGY_GREETING_DONE; and return
set -gx APPSYNERGY_GREETING_DONE 1

# Kill vendor fish_greeting (CachyOS etc.) so only our fetch runs.
function fish_greeting
end

if not isatty stdout
    return
end

if type -q fastfetch; and test -f /usr/share/appsynergy/fastfetch/config.jsonc
    # Explicit pad in case an older user config overrides the system file
    fastfetch --config /usr/share/appsynergy/fastfetch/config.jsonc --logo-padding-top 8
else if type -q appsynergy-banner
    appsynergy-banner
end
