# ci/ — GitHub Actions image

One workflow, `.github/workflows/ci.yml`, on `ubuntu-latest` with an
`archlinux:latest` container (makepkg needs Arch). Job `check` is
`scripts/check.sh` on every PR and push to `main`. Job `publish` needs `check`,
runs on `main` only, builds any/ packages, pulls the last kernel from the
`repo-x86_64` Release, signs, and publishes; it uploads nothing when the
published db already matches. Neither job builds an ISO. `check.sh` fails if a
second workflow file appears.

`gha/Containerfile` is an optional pin of that Arch userland. Publish to
`ghcr.io/appsynergy-io/appsynergy-linux-ci:<tag>` and point the workflows at it
when you want to stop `pacman -Syu` on every run.

```bash
docker build -f ci/gha/Containerfile -t ghcr.io/appsynergy-io/appsynergy-linux-ci:1 ci/gha
```

The image runs as uid 1000 `build`. Do not run `makepkg` as root.
