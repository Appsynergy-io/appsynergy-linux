# Distribution practice audit — 2026-08-06

Audit of appsynergy-linux against what a Linux distribution has to do to be
trustworthy and rebuildable. Findings are ranked by what breaks if ignored, each
with the evidence that produced it and the fix. Severity is about *distribution*
risk, not code style.

The through-line: **no published artifact can currently be traced to, or rebuilt
from, a commit** — and the infrastructure to fix that is largely already owned and
simply not connected to anything that ships.

There is a 15-year two-tier CA in Vault, `cosign` installed on the servers, and a
secrets manager that already holds credentials by reference. None of it touches a
package. Meanwhile the repo serves unsigned packages under `SigLevel = TrustAll`,
the kernel source is an untracked directory on one workstation, and no pipeline
checks any of it. The gap is not missing capability; it is capability that stops
at the edge of the build.

Read S1–S5 as one story: distribute the trust root, sign against it, make what is
signed rebuildable, then enforce all three mechanically. S6–S11 are hygiene.

**Findings are recorded as-found and left unedited; each now carries a `Status`
line.** S1, S2, S4 and S5 are resolved — the through-line above no longer holds:
packages and the database are GPG-signed, clients enforce
`Required DatabaseRequired`, and CI gates every push. Remediation units and the
live-host runbook: [AUDIT-REMEDIATION.md](AUDIT-REMEDIATION.md).

## S1 — A chain of trust exists and its root is distributed to nothing

There is a real two-tier PKI in Vault, and it is already in production use:

```
O=AppSynergy, CN=AppSynergy Root CA          (pki / pki_root — 403, access-restricted)
  └─ CN=AppSynergy Intermediate CA           (pki_int on vault-k2, valid to 2041-05-14)
       └─ CN=k2.imabee.com                   (Vault's own TLS, expires 2027-05-18)
```

The root CA is in **no machine's trust store** — not this workstation, not either
server — and **nothing in this repository ships it**. There is no
`appsynergy-ca-certificates` package.

The consequence is that every client of an internal TLS service must either
disable verification or pin out-of-band. `sdx` pins by `sha256:d549b0a4…`, which
is sound, but it is compensating for a missing anchor rather than using one. Any
ad-hoc tooling falls back to `curl -k`; this audit did so about six times purely
to read public CA endpoints. Estate-wide, TLS verification for internal services
is effectively off despite a perfectly good CA existing to enable it.

This is the cheapest high-value fix in this document, and it is squarely a
packaging problem — which is what this repository is for.

**Fix:** an `appsynergy-ca-certificates` package installing the root **and** the
intermediate to `/etc/ca-certificates/trust-source/anchors/` with
`update-ca-trust` in `post_install`; add it to both target package lists so every
variant gets it. Shipping the intermediate too keeps validation working against
services that present an incomplete chain (vault-k2 currently serves only its
leaf). Then drop `-k` everywhere and let pinning be defence in depth rather than
the only defence.

**Issuance policy (locked):** every end-entity certificate — code-signing, host
TLS, mTLS — is issued from `pki_int`, never from the root. The root key signs
exactly one thing in its lifetime: the intermediate. The root's public
certificate is distributed as a trust anchor; its private key is never an
operational signer. If the intermediate is ever compromised, it is revoked and
re-issued under the same root and machines re-trust nothing; if the root were an
operational signer, the same event would mean re-anchoring every machine.

**Status — RESOLVED (`1805903`, PR #1), with the fix above corrected (U9).**
`appsynergy-ca-certificates` ships and is pulled in by `build-repo.sh`,
`build-iso.sh`, `stage-rescue-payload.sh` and the installer. The fix as written —
root **and** intermediate into `anchors/` — was itself the defect: an anchor is an
independent trust root, so anything the intermediate signed was trusted outside
the intended chain and a compromised intermediate equalled a compromised root,
contradicting the issuance policy directly above. Since `1-3` only the Root is in
`anchors/`; the Intermediate installs to the `trust-source/` top level, which
`update-ca-trust(8)` gives **neutral trust** — known to the system for chain
construction, trusted for nothing. That still fixes the leaf-only case (vault-k2).

## S2 — Packages are unsigned, and signing must anchor to the chain above

`SigLevel = Optional TrustAll` in `appsynergy-mirrorlist/appsynergy.conf`, live on
all three machines. No `appsynergy-keyring` package exists. No `.sig` file exists
anywhere in the tree or the published repo.

Anyone who compromises the Gitea account, or MITMs `git.appsynergy.io`, can serve
a package that runs arbitrary code as root on every AppSynergy machine via
`post_install`. `TrustAll` means pacman does not merely lack a key — it is
instructed not to care. The `404`s on `.sig` fetches during every install are
this gap being audible.

**Constraint that shapes the fix:** pacman verifies **OpenPGP** via GPGME. An
X.509 certificate from `pki_int` cannot be used for `SigLevel` directly. This is a
protocol boundary, not a preference, so the answer is layered rather than
either/or:

| Layer | Mechanism | Why |
|-------|-----------|-----|
| pacman signature | GPG key, shipped in `appsynergy-keyring` | the only thing libalpm understands |
| release attestation | `cosign sign-blob` with a code-signing cert issued from `pki_int` | identity bound to a CA, with expiry and revocation |
| transport | Let's Encrypt on `git.appsynergy.io` (already correct for a public host) | browsers must validate it |

The bridge matters most. A bare GPG key shipped in a keyring is trusted purely
because it arrived — it asserts its own legitimacy. Anchor it instead: publish a
release manifest naming the authorised GPG fingerprint, `cosign`-signed with a
cert from `pki_int`. Verification then chains to the AppSynergy Root CA, and the
keyring stops being self-asserting. `cosign` v2.5.0 is already installed and
already in `packages-target-server.txt` — the intent was there, unwired.

**Fix order:** ship `appsynergy-ca-certificates` (S1) → issue the code-signing
cert from `pki_int` → generate the GPG key, private half in Vault → ship
`appsynergy-keyring` → sign with `makepkg --sign` / `repo-add --sign` → only then
flip `SigLevel = Required DatabaseRequired`. Flipping `SigLevel` before the
keyring lands locks every machine out of the repo.

**Status — RESOLVED (PRs #1, #4, #5, #9).** `appsynergy-keyring` ships key
`3B90D92D1E28E9E060D5C53D15D4351CF0D36AD1`; `build-repo.sh` signs every package
and `repo-add --sign`s the db by default; `publish-repo.sh` hard-fails on a
missing `.sig` (`ALLOW_UNSIGNED=1` the only override) and tail-calls
`verify-repo.sh`. The mirrorlist drop-in is `SigLevel = Required DatabaseRequired`
and `1-6` migrates legacy inline TrustAll sections on upgrade; the installer never
writes TrustAll. Both production servers enforce it. The only surviving TrustAll is
`desktop/iso/pacman.conf`, the build-time `file://` staging repo. **Still open:**
the cosign/`pki_int` attestation bridge — the keyring remains self-asserting.

## S3 — No published package can be rebuilt from this repository

`KDIR="${KDIR:-/home/imma/src/linux-cachyos/linux-cachyos}"` in every kernel
build script. That path is an untracked checkout on one workstation, and no
CachyOS version or commit is pinned anywhere in-tree.

If that machine is lost, no shipped kernel can be reproduced — including the two
currently running in production. There is no way to answer "what source produced
`linux-appsynergy-server-skylake 7.1.5-3`?"

**Fix:** pin the upstream `pkgver`/commit in-tree; fetch it in the PKGBUILD's
`source=()` with a checksum rather than assuming a sibling directory.

**Status — PARTIAL (PR #1).** Provenance is pinned: `kernel/upstream/PIN` records
`UPSTREAM_COMMIT=74d5bae` / `PKGVER=7.1.5` alongside committed patches and both
shipped `.config`s, the build scripts abort on pin mismatch unless
`PIN_OVERRIDE=1`, and `check.sh` stage `kernel-pin` asserts it. **Still open:** the
path half — `KDIR` still defaults to `/home/imma/src/linux-cachyos/linux-cachyos`,
no kernel PKGBUILD fetches source with a checksum, and `kernel-pin` returns 0
silently when `$KDIR/.git` is absent, so CI never exercises it.

## S4 — PKGBUILDs read from `$startdir`, which defeats verification

`appsynergy-branding`, `appsynergy-branding-desktop` and `appsynergy-wallpapers`
all declare `source=()` and `sha256sums=()`, then read files out of `${startdir}`
in `package()`.

makepkg therefore verifies nothing: there are no declared inputs to checksum. The
build depends on the state of the working tree rather than on anything recorded.
It also breaks `makepkg --source`, clean-chroot builds (`extra-x86_64-build`), and
any CI that does not run inside a git checkout at exactly the right path.

**Fix:** declare the real files in `source=()` with `sha256sums`, and use
`$srcdir` in `package()` — the pattern `appsynergy-mirrorlist` already follows.

**Status — RESOLVED (PR #1).** All three declare a generated payload tarball with a
real `sha256sums` and unpack from `$srcdir`; `packages/scripts/make-srctars.sh`
regenerates them deterministically and `check.sh` stage `tarball-sums` fails on
drift. No `$startdir` read remains.

## S5 — No CI, no lint, no gate

No `.github/`, `.gitea/`, `Makefile` or `justfile`. `shellcheck` is not installed.
Only the installer has tests (63); no package has one.

Every release is "run a script on the daily-driver workstation". All three defects
found on 2026-08-06 — the dead `pacman.conf.d` drop-in, the
`appsynergy-branding-*` glob also matching `-desktop-`, and publishing the
database before its packages — are mechanically detectable and would have been
caught by a modest pipeline.

**Fix:** a pipeline that runs `shellcheck`, `namcap` on every built package,
`cargo test`, builds all `any/` packages in a clean chroot, and finishes with
`verify-repo.sh`. Nothing publishes unless it passes.

**Status — RESOLVED (PRs #2, #10, #11, #13).** `scripts/check.sh` is the release
gate, 8 stages: `shellcheck` (discovers every script), `cargo-test`,
`tarball-sums`, `makepkg-all`, `namcap`, `kernel-pin`, `kernel-netfilter`,
`branding-glob`. `.gitea/workflows/ci.yml` runs exactly it on every push and PR,
`runs-on: arch-host`, on an act_runner pod on the skylake k3s cluster (`ci/`).

## S6 — Build host, release host and daily driver are one machine

`build-repo.sh` scrapes `/home/imma/src/linux-cachyos/...` for kernels; the same
workstation stages, signs (once signing exists) and publishes.

A compromise of the desktop is a compromise of the distribution. There is also no
clean-room guarantee: builds inherit whatever is installed that day.

**Fix:** builds in a container/chroot; ideally a dedicated builder. This is what
makes S3 and S4 actually hold.

**Status — PARTIAL (PRs #10, #11).** Verification left the daily driver: the
act_runner pod on the OVH k3s node builds and namcaps all six `any/` packages.
**Still open:** release did not. Signing uses the workstation's `GPGKEY`,
publishing its `GITEA_TOKEN`, and kernel packages still come from its `$KDIR`.

## S7 — No release identity

Zero git tags. No changelog. `pkgrel` is the only version signal, and nothing ties
a published package to a commit. "What shipped last Tuesday" is unanswerable.

**Fix:** tag releases; embed the git SHA into each package (a
`/usr/share/appsynergy/BUILDINFO`, or in `pkgdesc`); keep a changelog that names
the packages a release moved.

**Status — PARTIAL (PR #1).** Tag `v2026.08.06` exists and `CHANGELOG.md` names the
packages each release moved. **Still open:** no git SHA is embedded in package
metadata, so a package on a host still cannot name the commit that built it.

## S8 — Developer paths still hardcoded in ~10 places

`CLAUDE.md` says not to reintroduce absolute paths; they are still present:

| Path | Site |
|------|------|
| `/home/imma/src/linux-cachyos/...` | `build-repo.sh`, both kernel build scripts, `install-linux-appsynergy*.sh`, `build-iso.sh` |
| `/home/imma/projects/appsynergy-rs/ops/k3s/config.yaml` | `stage-k3s.sh` — a silent cross-repo dependency |
| `/home/imma/projects/combly`, `/beetv-rs` | `run-post-bios-bench.sh` |
| `file:///home/imma/projects/appsynergy-linux/...` | `desktop/iso/pacman.conf` (known; asserted by `build-iso.sh`) |

`stage-k3s.sh` is the worst: an ISO build silently depends on another checkout
being present, and produces a different image if it is not.

**Fix:** env vars with no default, failing loudly when unset, or vendor the input.

**Status — PARTIAL, 1 of 4 rows.** Row 2 fixed: `stage-k3s.sh` now requires an
explicit `K3S_CONFIG_SRC` or writes an inline default, so the ISO no longer
silently depends on another checkout. Rows 1, 3 and 4 stand — row 1 is now 7 sites
(both kernel build scripts, both install scripts, `build-repo.sh`, `build-iso.sh`,
`check.sh`).

## S9 — Publishing is destructive; there is no rollback

`publish-repo.sh` replaces the database in place. There is no way to serve
yesterday's repository. If a bad package ships, every machine that syncs gets it
and recovery is manual on each host.

**Fix:** date-stamped repo snapshots with `latest` as a pointer, so rollback is
repointing rather than rebuilding. Keep the previous N package versions published
so `pacman -U` of a known-good version always works.

**Status — PARTIAL (PR #1).** `publish-repo.sh` leaves a dated
`appsynergy.db-YYYYmmdd-HHMM` snapshot after each publish and never deletes package
files, so `pacman -U` of an older version still works. **Still open:** the live db
is replaced in place, there is no `latest` pointer to repoint, and no retention
policy — rollback is still manual on each host.

## S10 — 111 MB of git history is mostly rejected artwork

`.git` is 111 MB. The largest committed blobs are wallpaper candidates that were
never chosen — 16 MB, 11.9 MB, 6.2 MB, and so on, roughly 50 MB of
`brand-review/` in history permanently.

Review scratch is not product. Only the chosen master belongs in the tree, and it
now lives correctly in `appsynergy-wallpapers`.

**Fix:** keep future candidates out of git (separate assets store); accept the
existing history unless a rewrite is worth the disruption.

**Status — STILL TRUE.** `.git` is 113 MB; no history rewrite was attempted.
`brand-review/` is review-only and never read at runtime.

## S11 — One flat repo serves two products

`[appsynergy]` carries desktop kernels, server kernels and graphical packages in
one namespace. Package split and the installer's variant gate make it impossible
for a server to *acquire* a desktop package by dependency, but not impossible for
a human to install one by hand.

**Fix:** only if the hard wall is wanted — separate `appsynergy`/`appsynergy-desktop`
repos. Not obviously worth doubling the publish surface at three machines.

**Status — STILL TRUE, and still not worth it.** One flat repo, one namespace. The
identity split (`appsynergy-branding` vs `appsynergy-branding-desktop` +
`appsynergy-wallpapers`) keeps graphical packages off servers by audience rather
than by repo.

## Unused capability, stated plainly

The estate already owns a 15-year two-tier CA and has `cosign` installed on the
servers. Neither is doing anything beyond Vault's own TLS. Other things that
chain should be carrying and is not:

- **SecureBoot** — `kernel/docs/` records "Enroll SecureBoot keys" as a BIOS step,
  but kernels are unsigned and no key is issued from the chain.
- **Machine identity / mTLS** — host-to-host trust is SSH keys only; Vault can
  issue short-lived certs per host from `pki_int`.
- **Artifact provenance** — `cosign` is installed and listed for the server
  variant, signing nothing.

**Status.** The CA is now doing work: `appsynergy-ca-certificates` distributes the
Root as a trust anchor to every variant, so internal TLS can be verified instead of
pinned or `-k`'d. SecureBoot, mTLS host identity and `cosign` provenance are all
still unwired.

## Order of work

1. ~~**S1 `appsynergy-ca-certificates`**~~ — done; Root anchored, Intermediate neutral.
2. **S2 keyring + signing** — GPG half done and enforced; the `pki_int` anchoring of the keyring is not.
3. **S3/S4 pinned, declared sources** — S4 done; S3 pinned but still builds from a sibling checkout.
4. ~~**S5 CI**~~ — done; `check.sh` runs on every push.
5. **S9 repo snapshots** — dated snapshots exist; no `latest` pointer, so rollback stays manual.
6. S6, S7, S8, S10, S11 as capacity allows — S6/S7/S8 partially advanced, S10/S11 untouched.
