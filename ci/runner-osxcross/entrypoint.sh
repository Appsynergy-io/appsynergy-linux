#!/usr/bin/bash
# Register once, then run the daemon forever.
#
# Registration state is $RUNNER_FILE on the work PVC. It is the reason the
# Deployment is strategy: Recreate — two act_runner processes sharing one .runner
# race the same registration and the losing one is deregistered server-side.
set -uo pipefail

: "${CONFIG_FILE:=/etc/act_runner/config.yaml}"
: "${RUNNER_FILE:=/data/.runner}"
: "${GITEA_INSTANCE_URL:?GITEA_INSTANCE_URL unset}"
: "${RUNNER_NAME:=$(hostname)}"
: "${RUNNER_LABELS:=arch-host:host}"

if [[ ! -f "$RUNNER_FILE" ]]; then
  # Token arrives only as an env var from secretKeyRef; never echoed, never
  # written anywhere but $RUNNER_FILE (which stores a derived token, not this one).
  : "${GITEA_RUNNER_REGISTRATION_TOKEN:?registration token unset and $RUNNER_FILE absent}"
  act_runner register --no-interactive \
    --config "$CONFIG_FILE" \
    --instance "$GITEA_INSTANCE_URL" \
    --token "$GITEA_RUNNER_REGISTRATION_TOKEN" \
    --name "$RUNNER_NAME" \
    --labels "$RUNNER_LABELS" || exit 1
fi

exec act_runner daemon --config "$CONFIG_FILE"
