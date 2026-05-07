# Playbook — CI Zig Base Images

**Updated:** May 07, 2026
**Owner:** Agent (build), Human (one-time GHCR auth)
**Prerequisite:** `gh auth login` with `write:packages`, Docker Desktop or `docker-buildx-plugin`.

## Why this playbook exists

`mlugg/setup-zig` (and direct `wget` from `ziglang.org/download`) hangs intermittently when invoked from inside Alpine and Debian CI containers — the GitHub-runner Linux egress to those CDNs has been flaky enough to gate releases. Pre-baking Zig + the static-OpenSSL setup into GHCR images turns every CI lane that needs Zig into a pure `container: image: ...` step with zero per-job network fetch.

The three images this playbook publishes are:

| Image                                         | Arch              | Replaces in CI                                                                                       |
| --------------------------------------------- | ----------------- | ---------------------------------------------------------------------------------------------------- |
| `ghcr.io/usezombie/ci-zig-alpine`             | amd64 + arm64     | `cross-compile.yml` (both lanes), `release.yml` (Alpine job), `deploy-dev.yml` (Alpine job)          |
| `ghcr.io/usezombie/ci-zig-debian-trixie`      | amd64             | `memleak.yml`                                                                                        |
| `ghcr.io/usezombie/ci-zig-ubuntu`             | amd64             | `test.yml`, `bench.yml`, `lint.yml` (lint-zig), `qa.yml`, `qa-smoke.yml`, `test-integration.yml`     |

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
./playbooks/013_ci_zig_images/build_and_push.sh fetch-shas 0.15.2
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

**Default — all three images, multi-arch where applicable, pushed to `ghcr.io/usezombie`:**

```bash
./playbooks/013_ci_zig_images/build_and_push.sh build
```

Tags produced:

```
ghcr.io/usezombie/ci-zig-alpine:0.15.2          (linux/amd64 + linux/arm64 manifest)
ghcr.io/usezombie/ci-zig-debian-trixie:0.15.2   (linux/amd64)
ghcr.io/usezombie/ci-zig-ubuntu:0.15.2          (linux/amd64)
```

### Iterating without breaking pinned consumers

When a base packaging change ships (e.g. you add a package to the Alpine apk
list) but `ZIG_VERSION` is unchanged, bump the **revision** so consumers can
pin to the new tag explicitly:

```bash
./build_and_push.sh build --revision r2
# → ghcr.io/usezombie/ci-zig-alpine:0.15.2-r2  (and the other two)
```

Consumers (workflow YAMLs) should always pin to the full `<version>[-<rev>]`
tag — never `latest` — so a bad image rebuild can never silently break CI.

### Building a single image

```bash
./build_and_push.sh build --image alpine
./build_and_push.sh build --image debian-trixie
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
ZIG_VERSION="$(grep '^ZIG_VERSION=' playbooks/013_ci_zig_images/versions.env | cut -d= -f2)"

# alpine — confirm zig + static OpenSSL symlinks
docker run --rm --platform linux/amd64 \
  ghcr.io/usezombie/ci-zig-alpine:"$ZIG_VERSION" \
  sh -c 'zig version && ls -l /usr/lib/x86_64-linux-gnu/libssl.a'

docker run --rm --platform linux/arm64 \
  ghcr.io/usezombie/ci-zig-alpine:"$ZIG_VERSION" \
  sh -c 'zig version && ls -l /usr/lib/aarch64-linux-gnu/libssl.a'

# debian-trixie — confirm zig + valgrind
docker run --rm \
  ghcr.io/usezombie/ci-zig-debian-trixie:"$ZIG_VERSION" \
  sh -c 'zig version && valgrind --version'

# ubuntu — confirm zig + libssl + docker-cli
docker run --rm \
  ghcr.io/usezombie/ci-zig-ubuntu:"$ZIG_VERSION" \
  sh -c 'zig version && dpkg -s libssl-dev | head -2 && docker --version'
```

All three commands should print `0.15.2` (or whatever `versions.env` says) and exit 0.

---

## 4. Make GHCR packages public (one-time, human)

GHCR packages default to private, even when the repo is public. After the
first push, visit each package on GitHub:

```
https://github.com/usezombie?tab=packages
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
|                                                                | `usezombie` org                          |                                                                         |
| `--load is incompatible with multi-platform`                   | You passed `--no-push` to a multi-arch   | The script handles this by narrowing to `linux/amd64`. If you patched   |
|                                                                | image and bypassed the auto-narrowing    | the script, restore the narrowing branch.                               |
| `unsupported TARGETARCH=…` during alpine build                 | Docker built for an arch the Dockerfile  | Only `linux/amd64` and `linux/arm64` are supported.                     |
|                                                                | does not symlink                         |                                                                         |

---

## Lanes still on `mlugg/setup-zig`

Two lanes intentionally retain `mlugg/setup-zig`:

- **`test-integration.yml` → `test-integration`** — runs `docker compose up -d postgres redis` and `docker compose exec`. Doing that from inside a container needs `/var/run/docker.sock` mounted plus host-path-aware compose config. The bare-runner + `mlugg` path is not in the original "containers hang" failure mode, so left as-is.
- **macOS lanes** in `cross-compile.yml` and `release.yml` — Apple does not allow macOS in containers; `mlugg/setup-zig` works fine on the macOS runner network and is unaffected by the Linux-CDN hangs.
