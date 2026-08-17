# ci/ — GitHub Actions image

Workflows live in `.github/workflows/`. They run on `ubuntu-latest` with an
`archlinux:latest` container (makepkg needs Arch). `check.yml` is
`scripts/check.sh`. `packages.yml` builds any/ packages, pulls the last kernel
from the `repo-x86_64` Release, signs, and publishes. Neither job builds an ISO.

`gha/Containerfile` is an optional pin of that Arch userland. Publish to
`ghcr.io/appsynergy-io/appsynergy-linux-ci:<tag>` and point the workflows at it
when you want to stop `pacman -Syu` on every run.

```bash
docker build -f ci/gha/Containerfile -t ghcr.io/appsynergy-io/appsynergy-linux-ci:1 ci/gha
```

The image runs as uid 1000 `build`. Do not run `makepkg` as root.
