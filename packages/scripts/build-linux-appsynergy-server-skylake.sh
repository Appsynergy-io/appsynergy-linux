#!/usr/bin/bash
# Skylake max-performance server kernel (E3-1270 v6 / igb)
exec "$(cd "$(dirname "$0")" && pwd)/build-linux-appsynergy-server-flavor.sh" skylake "$@"
