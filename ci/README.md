# ci/ — self-hosted Gitea Actions runners (OVH k3s only)

Two act_runner 0.2.13 Deployments in namespace `ci` on the OVH skylake node. Host
exec mode, uid 1000 `build`, no docker/DinD. Each has **capacity: 1** and hard
limits **4 CPU / 8 Gi** so builds cannot choke production.

| Runner | Label | Image | Job |
|--------|-------|-------|-----|
| `k3s-arch-host` | `arch-host:host` | `appsynergy-ci-runner:0.2.13-2` | `.gitea/workflows/ci.yml` → `scripts/check.sh`; sdx gate + musl release |
| `k3s-osxcross` | `osxcross-host:host` | `appsynergy-ci-osxcross:0.1.0` | `.gitea/workflows/osxcross.yml` → `aarch64-apple-darwin` |

| Path | Role |
|------|------|
| `runner/Containerfile` | Arch image: check.sh deps + nodejs + pinned act_runner |
| `runner/entrypoint.sh` | register once into `/data/.runner`, then `act_runner daemon` |
| `runner-osxcross/Containerfile` | multi-stage OSXCross + rust aarch64-apple-darwin + act_runner |
| `runner-osxcross/sdk/` | **gitignored** MacOSX SDK tarball (packaged once from LAN MacBook) |
| `k8s/` | apply in filename order; `2x`/`3x`/`4x` are arch-host, `21`/`31`/`41` are osxcross |

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
# imagePullPolicy is IfNotPresent, so a rebuilt image needs a NEW TAG — pushing
# over 0.2.13-1 would leave the node running the copy it already has.
# -f is required: docker looks for `Dockerfile`, this repo writes `Containerfile`.
docker build -f ci/runner/Containerfile \
  -t git.appsynergy.io/imabee/appsynergy-ci-runner:0.2.13-2 ci/runner
sdx run --with gitea=kv/gitea -- bash -c \
  'printf %s "$GITEA_TOKEN" | docker login git.appsynergy.io -u imabee --password-stdin'
docker push git.appsynergy.io/imabee/appsynergy-ci-runner:0.2.13-2
docker logout git.appsynergy.io
# kubectl runs on the node, not here: pipe the manifest over ssh.
ssh root@144.217.66.212 kubectl apply -f - < ci/k8s/40-deployment.yaml
ssh root@144.217.66.212 kubectl -n ci rollout status deploy/act-runner

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

## What lives where on the runner

Two of these look interchangeable and are not. A project that parks build output
under `/data/cache` is writing into act_runner's own store.

| Path | Owner |
|------|-------|
| `/data/cache` | **act_runner's**: `cache.dir` in the ConfigMap, plus `bolt.db` |
| `/data/work` | act_runner job workspaces; `workdir_parent`, pruned per job |
| `/data/build` | where a project puts its own build cache (sdx: `CARGO_TARGET_DIR`) |
| `/cache/cargo` | `CARGO_HOME` — registry index and crate sources |
| `/var/cache/pacman/pkg` | **empty by design**, and not a fault to fix: `check.sh` builds with `makepkg -d`, so pacman is never invoked, and uid 1000 on a read-only rootfs could not install a package anyway |

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

## OSXCross runner (aarch64-apple-darwin)

SDK is packaged **once** from the LAN MacBook (`imma@192.168.101.14`, Xcode.app),
never from OVH Apple downloads. CI jobs never contact the Mac.

```bash
# 1. SDK tarball already on the build host under ci/runner-osxcross/sdk/
#    (scp from Mac after gen_sdk_package.sh — agent does this)

# 2. Build + push
podman build -t git.appsynergy.io/imabee/appsynergy-ci-osxcross:0.1.0 ci/runner-osxcross
podman login git.appsynergy.io && podman push git.appsynergy.io/imabee/appsynergy-ci-osxcross:0.1.0
podman image inspect --format '{{index .RepoDigests 0}}' \
  git.appsynergy.io/imabee/appsynergy-ci-osxcross:0.1.0

# 3. Register + apply (token never echoed)
curl -sS -X POST -H "Authorization: token $GITEA_TOKEN" \
  https://git.appsynergy.io/api/v1/repos/imabee/appsynergy-linux/actions/runners/registration-token \
  | jq -rj .token \
  | kubectl -n ci create secret generic act-runner-osxcross-reg --from-file=token=/dev/stdin
kubectl apply -f ci/k8s/21-configmap-act-runner-osxcross.yaml \
              -f ci/k8s/31-pvcs-osxcross.yaml \
              -f ci/k8s/41-deployment-osxcross.yaml
kubectl -n ci rollout status deploy/act-runner-osxcross
```

Verify: logs show `k3s-osxcross` / `osxcross-host`; workflow `osxcross.yml` goes green.

## Roll back

`kubectl delete ns ci` takes every Deployment, PVC (reclaim Delete) and NetworkPolicy.
Then delete runners in Gitea (Repo → Settings → Actions → Runners), or they linger
`offline` and jobs queue against a label nothing serves.

Partial: `kubectl -n ci delete deploy/act-runner-osxcross pvc/act-runner-osxcross-work pvc/act-runner-osxcross-cargo-cache`
