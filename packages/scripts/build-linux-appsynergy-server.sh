#!/usr/bin/bash
# Build BOTH host-max server kernels (skylake + tigerlake) for the installer ISO.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
echo "==> Building skylake + tigerlake max-performance server kernels"
"$ROOT/build-linux-appsynergy-server-flavor.sh" skylake
"$ROOT/build-linux-appsynergy-server-flavor.sh" tigerlake
echo "==> Both server kernels staged."
