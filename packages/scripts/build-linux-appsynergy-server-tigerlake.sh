#!/usr/bin/bash
# Tiger Lake max-performance server kernel (i7-1185G7 / igc)
exec "$(cd "$(dirname "$0")" && pwd)/build-linux-appsynergy-server-flavor.sh" tigerlake "$@"
