# Playbook — CI Zig Base Images

**Updated:** May 07, 2026
**Owner:** Agent (build), Human (one-time GHCR auth)
**Prerequisite:** `gh auth login` with `write:packages`, Docker Desktop or `docker-buildx-plugin`.

## Why this playbook exists

`mlugg/setup-zig` (and direct `wget` from `ziglang.org/download`) hangs intermittently when invoked from inside Alpine and Debian CI containers — the GitHub-runner Linux egress to those CDNs has been flaky enough to gate releases. Pre-baking Zig + the static-OpenSSL setup into GHCR images turns every CI lane that needs Zig into a pure `container: image: ...` step with zero per-job network fetch.

The three images this playbook publishes are:

| Image                                         | Arch              | Replaces in CI                                                                                       |
| --------------------------------------------- | ----------------- | ---------------------------------------------------------------------------------------------------- |
| `ghcr.io/agentsfleet/ci-zig-alpine`             | amd64 + arm64     | `cross-compile.yml` (both lanes), `release.yml` (Alpine job), `deploy-dev.yml` (Alpine job), `make/test-integration.mk` (`RUNNER_CI_IMAGE`, the local macOS kernel lane) |
| `ghcr.io/agentsfleet/ci-zig-ubuntu`             | amd64             | `test.yml`, `bench.yml`, `lint.yml` (lint-zig), `qa.yml`, `qa-smoke.yml`, `test-integration.yml`     |

**Current revision: `r4`** — both images carry `bubblewrap`. The runner spawns
every sandboxed lease through `bwrap`, so an image without it makes each
real-sandbox proof resolve `error.BwrapUnavailable` and `SkipZigTest` —
silently, on every run. That is how a sandbox missing `/run/systemd/resolve`
shipped and broke every lease for a week (M167).

A third image, `ci-zig-debian-trixie`, stopped being built in M181_001: it
existed for `memleak.yml`, which `b9163ed32` deleted with the rest of the Zig
lanes. Its Dockerfile and its `--image` arm are gone from this playbook, so
nothing here rebuilds it.

**The published package STAYS. Do not delete it** (Indy, 2026-08-31). Eleven
versions sit under `ghcr.io/agentsfleet/ci-zig-debian-trixie`, last pushed
2026-07-12, and deleting a published package is irreversible: a digest someone
pinned elsewhere, or a workflow on an old branch, resolves to nothing
afterwards and there is no undo. An orphaned image costs storage; a deleted one
can break a checkout nobody thought to check. Retiring the build path is the
reversible half and is all that was done. The Dockerfile is in git history if
the lane ever returns.

`bwrap` needs to create namespaces, which Docker's default seccomp profile
refuses. Lanes that execute a sandbox therefore need `--privileged` (the
`test-integration.yml` kernel job and the local macOS lane both have it) or
`--security-opt seccomp=unconfined` (the `test.yml` coverage lane). The
unprivileged unit lanes only compose argv, so the binary alone is enough there.

**Status:** images are live in GHCR (public) and every Zig-using workflow has been rewritten to consume them. The Zig + OpenSSL toolchain is no longer fetched per-job — every CI lane that needs Zig pulls the relevant `ci-zig-*` image and runs `make` directly.

---

## Sequence

```
0. (once per Zig version bump)  fetch-shas
1. (per build)                  authenticate to GHCR
2. (per build)                  buildx + push three images
3. (post-publish)               smoke-verify each tag
```

**Human vs Agent split:**

| Step                                       | Owner | Why                                              |
| ------------------------------------------ | ----- | ------------------------------------------------ |
| `gh auth login` (`write:packages` scope)   | Human | Browser OAuth, one-time per machine              |
| `fetch-shas` for new Zig version           | Agent | Read-only fetch from ziglang.org index.json      |
| `build` (multi-arch + push to GHCR)        | Agent | Fully scriptable                                 |
| First-time GHCR repo visibility (public)   | Human | Defaults to private; flip to public in GitHub UI |

---

## 0. fetch-shas — refresh `versions.env` (only when bumping Zig)

```bash
./playbooks/operations/ci_zig_images/build_and_push.sh fetch-shas 0.16.0
```

Pulls `https://ziglang.org/download/index.json`, extracts the four
`x86_64-linux`, `aarch64-linux`, `x86_64-macos`, `aarch64-macos` SHA256s,
and rewrites `versions.env`. Commit the resulting diff.

The Dockerfiles fetch from `pkg.machengine.org` first (`zigmirror.hryx.net` and
`ziglang.org/download` are fallbacks); the SHA256 in `versions.env` is the
trust anchor regardless of which mirror serves the bytes.

---

## 1. Authenticate to GHCR

The script picks up credentials in this order:

1. `GHCR_TOKEN` env var (PAT with `write:packages`)
2. `gh auth token` (the script calls it automatically if `GHCR_TOKEN` is unset)

Username defaults to `gh api user --jq .login`; override with `GHCR_USER` if needed.

```bash
# If you don't already have a GHCR-scoped token in the environment:
gh auth refresh -h github.com -s write:packages
```

---

## 2. Build + push

**Default — all three images, multi-arch where applicable, pushed to `ghcr.io/agentsfleet`:**

```bash
./playbooks/operations/ci_zig_images/build_and_push.sh build
```

Tags produced:

```
ghcr.io/agentsfleet/ci-zig-alpine:0.16.0          (linux/amd64 + linux/arm64 manifest)
ghcr.io/agentsfleet/ci-zig-ubuntu:0.16.0          (linux/amd64)
```

### Iterating without breaking pinned consumers

When a base packaging change ships (e.g. you add a package to the Alpine apk
list) but `ZIG_VERSION` is unchanged, bump the **revision** so consumers can
pin to the new tag explicitly:

```bash
./build_and_push.sh build --revision r4
# → ghcr.io/agentsfleet/ci-zig-alpine:0.16.0-r4  (and the other two)
```

A revision bump is only landed once the tag is pushed AND every consumer is
repinned in the same change — `grep -rn 'ci-zig-' .github/workflows/ make/`
enumerates them. Leaving a lane on the old revision is how two images drift
into service and the next reader has to work out which lanes carry which
packages.

Consumers (workflow YAMLs) should always pin to the full `<version>[-<rev>]`
tag — never `latest` — so a bad image rebuild can never silently break CI.

### Building a single image

```bash
./build_and_push.sh build --image alpine
./build_and_push.sh build --image ubuntu
```

### Local-only build (no push)

`--no-push` swaps `--push` for `--load`, which docker buildx requires to be
single-arch — the script automatically narrows multi-arch to `linux/amd64`
when `--no-push` is set.

```bash
./build_and_push.sh build --image ubuntu --no-push
```

### Custom registry

```bash
./build_and_push.sh build --registry ghcr.io/myfork
```

---

## 3. Smoke-verify each pushed tag

Run from any host with Docker (the script does not do this itself — it's a
post-publish sanity check the operator runs once per release):

```bash
ZIG_VERSION="$(grep '^ZIG_VERSION=' playbooks/operations/ci_zig_images/versions.env | cut -d= -f2)"

# alpine — confirm zig + static OpenSSL symlinks
docker run --rm --platform linux/amd64 \
  ghcr.io/agentsfleet/ci-zig-alpine:"$ZIG_VERSION" \
  sh -c 'zig version && ls -l /usr/lib/x86_64-linux-gnu/libssl.a'

docker run --rm --platform linux/arm64 \
  ghcr.io/agentsfleet/ci-zig-alpine:"$ZIG_VERSION" \
  sh -c 'zig version && ls -l /usr/lib/aarch64-linux-gnu/libssl.a'

# ubuntu — confirm zig + kcov + python3 + make + docker-cli + compose
docker run --rm --platform linux/amd64 \
  ghcr.io/agentsfleet/ci-zig-ubuntu:"$ZIG_VERSION" \
  sh -lc 'zig version && kcov --version && python3 --version && make --version | head -n 1 && docker --version && docker compose version'
```

`docker compose version` is in that list because it once vanished without the
Dockerfile changing. `docker.io` used to bundle compose v2 and no longer does,
so a rebuild from an unchanged Dockerfile produced an image whose compose was
gone, and `test-integration` died on `unknown shorthand flag: 'd' in -d`. The
package is named explicitly now, and this check is what proves it stayed.
Verify a capability the lanes depend on, not merely the toolchain.

For a revisioned publish (e.g. `--revision r4`), substitute
`"$ZIG_VERSION"-r4` for `"$ZIG_VERSION"` in the tag above.

`bwrap` needs both halves checked — the binary alone proves nothing, since a
present-but-unusable `bwrap` still fails every sandbox spawn:

```bash
# binary present (alpine and ubuntu only)
docker run --rm ghcr.io/agentsfleet/ci-zig-alpine:"$ZIG_VERSION"-r4 bwrap --version

# and it can actually unshare — needs --privileged, as the lanes that spawn
# sandboxes have. Without it Docker's seccomp profile refuses the namespace
# and this prints "No permissions to creating new namespace".
docker run --rm --privileged \
  ghcr.io/agentsfleet/ci-zig-alpine:"$ZIG_VERSION"-r4 \
  bwrap --unshare-all --ro-bind / / -- /bin/busybox echo SANDBOX_OK
```

All three commands should print `0.16.0` (or whatever `versions.env` says) and exit 0.

---

## 4. Make GHCR packages public (one-time, human)

GHCR packages default to private, even when the repo is public. After the
first push, visit each package on GitHub:

```
https://github.com/agentsfleet?tab=packages
```

Click each `ci-zig-*` package → **Package settings** → **Change visibility** → **Public**.

Subsequent pushes inherit visibility; this is one-click per image.

---

## Troubleshooting

| Symptom                                                        | Cause                                    | Fix                                                                     |
| -------------------------------------------------------------- | ---------------------------------------- | ----------------------------------------------------------------------- |
| `FAIL: zig … download failed from every mirror`                | All three Zig CDNs unreachable from your | Wait + retry; or temporarily build from a region with better routing.   |
|                                                                | network at build time                    |                                                                         |
| `denied: installation not allowed to Create organization …`    | Your GitHub user is not a member of the  | Ask an org admin for access, or push to your fork (`--registry`).       |
|                                                                | `agentsfleet` org                          |                                                                         |
| `--load is incompatible with multi-platform`                   | You passed `--no-push` to a multi-arch   | The script handles this by narrowing to `linux/amd64`. If you patched   |
|                                                                | image and bypassed the auto-narrowing    | the script, restore the narrowing branch.                               |
| `unsupported TARGETARCH=…` during alpine build                 | Docker built for an arch the Dockerfile  | Only `linux/amd64` and `linux/arm64` are supported.                     |
|                                                                | does not symlink                         |                                                                         |

---

## Lanes still on `mlugg/setup-zig`

Two lanes intentionally retain `mlugg/setup-zig`:

- **`test-integration.yml` → `test-integration`** — runs `docker compose up -d postgres redis` and `docker compose exec`. Doing that from inside a container needs `/var/run/docker.sock` mounted plus host-path-aware compose config. The bare-runner + `mlugg` path is not in the original "containers hang" failure mode, so left as-is.
- **macOS lanes** in `cross-compile.yml` and `release.yml` — Apple does not allow macOS in containers; `mlugg/setup-zig` works fine on the macOS runner network and is unaffected by the Linux-CDN hangs.
