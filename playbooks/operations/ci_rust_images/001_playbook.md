# Playbook — CI Rust Base Image

Builds and publishes `ghcr.io/agentsfleet/ci-rust-slim`, the host-runner
replacement for the `rustd` UNIT lane.

| Image | Arch | Consumers |
|---|---|---|
| `ghcr.io/agentsfleet/ci-rust-slim` | amd64 | `test.yml` (`test-unit-rustd`) |

The integration lane is NOT a consumer and the image carries nothing for it —
no Docker client, no compose plugin, no `psql`. §"The integration lane is NOT a
drop-in" below is why: that lane cannot run from inside a job container at all,
so provisioning it here would put a Docker daemon in the image to serve nobody.

Sibling of `../ci_zig_images/`, which this follows deliberately — the build
script, the `versions.env` pin file, and the revision-suffix discipline are all
that playbook's, so an operator who has published one can publish the other.

## Why this playbook exists

The `rustd` lanes run bare on `ubuntu-latest`. Every job therefore repeats a
fixed setup before it compiles anything: resolve the rustup channel, download
`cargo-llvm-cov` through `taiki-e/install-action`, run `setup-bun`, and apt-fetch
whatever the lane's datastore client needs. None of that varies between runs, and
none of it is cached by the artifact cache, which holds compiler output rather
than tools.

An image fixes the tools once. It does not fix compilation — see below, because
conflating the two is the mistake this section exists to prevent.

## What this image is NOT for

**Build artifacts.** An image layer is immutable: `rustd/target` cannot live in
it, and a `cargo build` inside a fresh container starts from nothing. The image
removes SETUP time; the cache removes COMPILE time. They are different halves and
neither substitutes for the other.

The cache half lives in the workflows as `Swatinem/rust-cache`, which is what
lets `test-unit-rustd` and `test-integration-rustd` share one set of compiler
artifacts instead of holding a cache each.

## Sequence

1. Confirm the pin (`versions.env` `RUST_VERSION` == `rustd/rust-toolchain.toml`
   channel). `build_and_push.sh` refuses to build otherwise.
2. Authenticate to GHCR.
3. Build + push.
4. Smoke-verify the pushed tag.
5. Only then point a workflow at it.

## 1. Authenticate to GHCR

```bash
# Needs write:packages. The script also picks this up from `gh auth token`.
export GHCR_TOKEN='...'
```

## 2. Build + push

```bash
cd playbooks/operations/ci_rust_images
./build_and_push.sh                       # → ghcr.io/agentsfleet/ci-rust-slim:1.98.0
./build_and_push.sh --revision r2         # → :1.98.0-r2
./build_and_push.sh --no-push             # local --load, nothing published
```

### Iterating without breaking pinned consumers

Workflows pin a full tag including the revision suffix. Re-publishing the same
tag with different contents is what made a rebuilt Zig image silently lose
`docker compose`; bump `--revision` instead and move the workflow pin in the same
commit.

## 3. Smoke-verify the pushed tag

Every claim the Dockerfile makes, checked from outside it. Run before pointing
any workflow at a new tag.

```bash
TAG=ghcr.io/agentsfleet/ci-rust-slim:1.98.0

# The toolchain is the pinned one, with the components the lane runs.
docker run --rm "$TAG" rustc --version          # → rustc 1.98.0
docker run --rm "$TAG" cargo fmt --version
docker run --rm "$TAG" cargo clippy --version

# The tools the lane's two make targets shell out to.
docker run --rm "$TAG" make --version
docker run --rm "$TAG" git --version

# The linker exists. A Rust image without `cc` compiles and then fails at link
# time with a message that reads like a project defect.
docker run --rm "$TAG" cc --version

# A REAL compile, not a version string. Every check above passes on an image
# that cannot build anything — the `CARGO_TARGET_DIR=""` defect the Dockerfile
# records was invisible to exactly this suite. `aws-lc-sys` is the target
# because it is the one dependency that compiles C through `cmake`, which the
# bare runner has always supplied for free and a container must carry itself;
# a workspace crate that happens not to pull it would go green proving nothing.
# Read-only mount and a container-side target dir, so verifying an image cannot
# write into the checkout.
docker run --rm -v "$PWD:/work:ro" -w /work/rustd -e CARGO_TARGET_DIR=/tmp/t "$TAG" \
  cargo build -p aws-lc-sys

# Readable and writable as a NON-root uid, which is what a runner may pick.
# This is the check that catches a toolchain installed under /root.
docker run --rm --user 1001:1001 "$TAG" cargo --version
```

## 4. Point a workflow at it

Only after §3 passes against the exact tag.

```yaml
jobs:
  test-unit-rustd:
    runs-on: ubuntu-latest
    container:
      image: ghcr.io/agentsfleet/ci-rust-slim:1.98.0
    steps:
      - uses: actions/checkout@v6
      # `rustup show`, `taiki-e/install-action@cargo-llvm-cov` and
      # `oven-sh/setup-bun` all come OUT — the image already carries them.
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: rustd
          shared-key: rustd
```

### The integration lane is NOT a drop-in, and mounting the socket does not fix it

Mounting `/var/run/docker.sock` into the job container is the obvious move and
it does not work here. Compose would then start Postgres and Redis as siblings
**on the host**, while `make/test-infra.mk:97-98` connects to
`postgres://…@localhost:$(COMPOSE_PG_PORT)` and
`rediss://…@localhost:$(COMPOSE_REDIS_PORT)` — and inside a job container
`localhost` is that container's own loopback, where nothing is listening. The
lane dies at connection time and reads as a datastore fault. Compose's relative
bind mounts and `TEST_REDIS_CA_CERT` have the same problem from the other
direction: they resolve against the host filesystem, where the job container's
checkout does not exist.

So `test.yml`'s unit job is the one to containerise first. Moving the
integration lane needs a separate decision that this playbook does not make for
you — convert it to GitHub Actions `services:` and drop compose, or leave it on
the bare runner. Until then it keeps its current shape and only takes the cache
change.

## Bumping the Rust version

Three files carry the number and all three move together, or a developer shell
and CI silently compile with different compilers:

1. `rustd/rust-toolchain.toml` — `channel`
2. the mise tools config — `rust`
3. `versions.env` — `RUST_VERSION`

`build_and_push.sh` checks 1 against 3 and refuses to build on a mismatch. It
cannot see 2; that one is on the operator.

Then rebuild, re-verify (§3), and move the workflow pins in the same commit.

## Status

**Built, measured, and deliberately NOT published. Do not wire a workflow to
this.**

The image exists to remove per-job setup, and the setup it removes was finally
measured against a real run of `test-unit-rustd`:

| step | time |
|---|---|
| Set up job | 2s |
| `actions/checkout@v6` | 3s |
| **Show the pinned toolchain** | **11s** |
| `Swatinem/rust-cache@v2` | 0s |
| Lint the Rust workspace | 69s |
| Test the Rust workspace | 190s |
| total | 275s |

Eleven seconds is the whole prize. The image costs a 454 MB pull and a 1.21 GB
unpack on every job to win it, so adopting it would make the lane SLOWER, and
would additionally buy a GHCR publish step, a third place the Rust version is
pinned, and a rebuild on every version bump.

The earlier framing here — "measured as steps rather than seconds" — declined
to measure, and the measurement disagrees with the conclusion it justified.
Ninety-four per cent of the job is the 190s test and the 69s lint; that is
where lane time actually is, and no base image touches it.

Kept rather than deleted for one reason: the first cut of this directory could
not build at all (`rustup-init` takes one value per `--component`, and the
install line passed three bare words after a single flag), and a repository
should not carry a file that fails on its own line 74. It is correct now, and
it is parked. Reopening this needs a NEW measurement, not a new opinion.

**The image is not published.** This directory and the workflow caching
change landed together; pointing the workflows at the image is a separate commit
that must not merge before §2 and §3 have been done by someone who can push to
GHCR. A workflow referencing an unpublished image fails every job on the branch.
