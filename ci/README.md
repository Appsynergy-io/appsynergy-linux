# ci/ — self-hosted Gitea Actions runner

act_runner 0.2.13 on the OVH k3s node, namespace `ci`, one replica, `capacity: 1`. Runs
`.gitea/workflows/ci.yml`, which is `scripts/check.sh` and nothing else. Label
`arch-host:host` — host exec mode, jobs run in the pod's own Arch userland as uid 1000
`build`. No docker, no DinD: k3s is the only runtime here.

| Path | Role |
|------|------|
| `runner/Containerfile` | Arch image: check.sh deps + nodejs (`actions/checkout@v4` is a JS action) + pinned act_runner |
| `runner/entrypoint.sh` | register once into `/data/.runner`, then `act_runner daemon` |
| `k8s/` | apply in filename order: namespace, netpol, config, PVCs, deployment |

## Registry

`curl -sS -o /dev/null -w '%{http_code}\n' https://git.appsynergy.io/v2/` — 401 means the
container registry is enabled and wants auth, the expected good case.

401 → push to `git.appsynergy.io/imabee/appsynergy-ci-runner`. Anything else (404/501)
→ registry off: `podman save | zstd`, upload as a Gitea generic package, on the node
`zstd -d | k3s ctr images import -` (`imagePullPolicy: IfNotPresent` makes it win).

The package is private (anonymous `GET /v2/.../manifests/<tag>` → 401), so the kubelet
needs `gitea-registry` — created below, referenced by `k8s/40-deployment.yaml`.

## Build, push, deploy

```bash
podman build -t git.appsynergy.io/imabee/appsynergy-ci-runner:0.2.13-1 ci/runner
podman login git.appsynergy.io && podman push git.appsynergy.io/imabee/appsynergy-ci-runner:0.2.13-1
podman image inspect --format '{{index .RepoDigests 0}}' \
  git.appsynergy.io/imabee/appsynergy-ci-runner:0.2.13-1     # repin the Deployment to this digest

kubectl apply -f ci/k8s/00-namespace.yaml
# Pull credentials for the private package. --docker-password reads the PAT from the
# environment, so the value is never typed, echoed, or stored outside the Secret.
kubectl -n ci create secret docker-registry gitea-registry \
  --docker-server=git.appsynergy.io --docker-username=imabee \
  --docker-password="$GITEA_TOKEN"
# Token is minted, piped, consumed: never echoed, never written to a file, never in
# shell history. $GITEA_TOKEN is a PAT loaded by reference (sdx / `set -a; . …`).
curl -sS -X POST -H "Authorization: token $GITEA_TOKEN" \
  https://git.appsynergy.io/api/v1/repos/imabee/appsynergy-linux/actions/runners/registration-token \
  | jq -rj .token \
  | kubectl -n ci create secret generic act-runner-reg --from-file=token=/dev/stdin
kubectl apply -f ci/k8s/                                     # netpol, config, PVCs, deployment
```

PVCs stay `Pending` until the pod schedules — `local-path` is WaitForFirstConsumer.

## Verify

```bash
kubectl -n ci rollout status deploy/act-runner
kubectl -n ci logs deploy/act-runner --tail=20     # expect: runner k3s-arch-host, labels [arch-host]
curl -sS -H "Authorization: token $GITEA_TOKEN" \
  https://git.appsynergy.io/api/v1/repos/imabee/appsynergy-linux/actions/runners \
  | jq '.runners[]|{name,status,labels}'
```

Status `idle` + label `arch-host` = the next push runs the gate. Registration is
one-shot; afterwards `kubectl -n ci delete secret act-runner-reg`.

## Roll back

`kubectl delete ns ci` takes the Deployment, PVCs (reclaim Delete) and NetworkPolicies.
Then delete the runner in Gitea (Repo → Settings → Actions → Runners), or it lingers
`offline` and jobs queue against a label nothing serves.
