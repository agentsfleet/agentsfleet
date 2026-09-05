# Playbook — CI Rust Base Image

**Updated:** Sep 5, 2026
**Owner:** Agent (build), Human (one-time GHCR auth)
**Prerequisite:** `gh auth login` with `write:packages`, Docker Desktop or `docker-buildx-plugin`.

## Why this playbook exists

The static daemon is compiled inside a musl-native Alpine container, so the musl
target is the host target and no developer has to configure a cross linker. That
part was always right. What was wrong is that the build ran the stock
`rust:*-alpine` image and `apk add`ed its build dependencies **inside a
throwaway container**, so the same packages were fetched and installed again on
every build, of every architecture, with nothing cached between them.

Baking them into a published image makes them layers: pulled once, reused by
every build after. The same move `ci_zig_images` made for the Zig lanes, for the
same reason.

## One image, not two

| Image | Arch | Consumers |
|---|---|---|
| `ghcr.io/agentsfleet/ci-rust-alpine` | amd64 + arm64 | `make/build.mk` (`_dist-daemons`), `deploy-dev-build.yml`, `release.yml` |

A second image, `ci-rust-slim` (bookworm), was built here on Aug 29 for the lint
and unit lanes and **never published** — its own inventory named `test.yml` as
the consumer while that workflow ran on the host runner with rustup and
referenced no container at all. Rather than publish a second base with a second
Rust pin, this image absorbed what those lanes need: `git`, `pkgconf`,
`ca-certificates`, and the three components `rustd/rust-toolchain.toml` names.

That keeps ONE place the Rust version is written. Two images meant two pins that
nothing checked against each other, which is the same "second compiler nobody
chose" that `versions.env` exists to prevent — one level up.

**The tag is DERIVED, never pasted.** `make/build.mk` computes it from
`versions.env`, and both workflows compute it with the same `sed`.
`check-gh-actions-valid` fails on a literal, and asserts the reachable property:
not "does the pasted tag match" but "is there a pasted tag at all". A stale
literal still runs — it just compiles on a toolchain the repository has moved
off.

| Image | Arch | Consumed by |
| --- | --- | --- |
| `ghcr.io/agentsfleet/ci-rust-alpine` | amd64 + arm64 | `make/build.mk` (`_dist-daemons`, and therefore `build`, `build-dev` and `make up`); the release lane's musl job |

Both architectures are published deliberately. `docker run --platform` on a
foreign host runs the entire compile under QEMU, and that emulation costs more
than the packages this image exists to cache — so each host must be able to pull
a native one.

## What is pinned, and why

`versions.env` holds `RUST_VERSION` and `ALPINE_SERIES`, and `build_and_push.sh`
refuses to build when `RUST_VERSION` disagrees with `rustd/rust-toolchain.toml`.
An image compiling with a different rustc than every other lane is a second
compiler nobody chose, and the binary it produces is not the one the tests
graded.

The Alpine floor names a patch series rather than a floating `alpine3`, so a
major release cannot arrive unannounced. Be precise about what that does and
does not buy: `alpine:3.24` is itself a moving tag — it resolves to 3.24.1 today
and will resolve to 3.24.2 — so the musl, gcc and binutils layer floats by patch
while the compiler does not. Closing that would mean pinning `alpine@sha256:`
and paying a commit per Alpine patch. `ci_zig_images` makes the same trade, and
the input that decides the shipped binary is the compiler, which is pinned
exactly. The repository holds GitHub Actions to the stricter rule
(`audits/gh-actions-runtime.sh`), where the cost of a digest is one line.

## Why the base is Alpine and not `rust:*-alpine`

docker-library publishes its `rust` alpine variants weeks behind a rustc
release. 1.98.1 shipped Sep 1, 2026 and the newest `rust:*-alpine` was still
1.98.0 four days later, so `FROM rust:${RUST_VERSION}-alpine${ALPINE_SERIES}`
could not be built on the day the repository moved its pin. Waiting is not a
plan, and pinning the base to the older tag while installing the newer
toolchain into it puts two compilers in one image — the exact thing
`versions.env` exists to prevent.

So the base is plain Alpine and rustup installs `RUST_VERSION` into it. That
leaves `versions.env` the only place the compiler is named, and it decouples the
repository's toolchain from another project's release cadence.

The bootstrap is not invented here. It follows rust-lang/docker-rust's own
`Dockerfile-alpine.template` — same archive URL shape, same `--profile minimal`,
same `--default-host` — and it is the shape `ci_zig_images` already uses next
door: a pinned installer plus a per-architecture SHA256.

`RUSTUP_VERSION` and the two `RUSTUP_SHA256_*` values pin that installer.
Refresh them with `./build_and_push.sh fetch-shas`, which reads the checksum
upstream publishes at `<installer-url>.sha256`, downloads the binary, and
**refuses when the two disagree**. What that buys is real but bounded: a
truncated, cached, or tampered-in-transit body fails loudly instead of becoming
the new pin. It is not a signature. A wholly compromised static.rust-lang.org
would serve a matching pair, and rustup publishes nothing to check against that.

Hashing our own download and calling the result a trust anchor would have been
strictly weaker — it proves only that the file did not change after we fetched
it, which is the one thing nobody was worried about. Do not paste a checksum by
hand either; `fetch-shas` is the path that checks.

## The published tag and this directory can drift

Nothing rebuilds `ci-rust-alpine` on merge. The workflows `docker run` the tag,
`make/build.mk` pulls it, and neither notices that the `Dockerfile.alpine` in
the repository is not the one the tag was built from. So an edit to this
directory after the image was pushed leaves every lane compiling inside the old
image while the diff shows the new recipe.

The rule that follows: **any change to `Dockerfile.alpine` or `versions.env`
means re-running `./build_and_push.sh` before the PR merges.** Use
`--revision r2` when the tag is already consumed by a merged commit; overwriting
is only safe while the tag is new and nothing on the default branch names it.

## Build and publish

```bash
cd playbooks/operations/ci_rust_images

./build_and_push.sh --no-push          # this arch only, loaded locally
./build_and_push.sh                    # both arches, pushed to GHCR
./build_and_push.sh --revision r2      # iterate without moving a pinned tag
./build_and_push.sh fetch-shas         # refresh the rustup-init checksums
```

`--no-push` builds a single architecture because `docker buildx --load` cannot
accept a multi-platform manifest. That is a docker limitation, not a choice.

## Moving the Rust version

1. Move `channel` in `rustd/rust-toolchain.toml`. If your own mise config names a
   Rust version, move it too — mise exports `RUSTUP_TOOLCHAIN`, which overrides
   the file, and that config is not something this repository can gate.
2. Move `rust-version` in `rustd/Cargo.toml` with them — the workspace floor is
   equal to the pin, not trailing it, so cargo fails loudly on the wrong rustc
   rather than compiling quietly.
3. Move `RUST_VERSION` in `versions.env` to match.
4. `./build_and_push.sh` — it refuses if steps 1, 2 and 3 disagree.
5. Move `BUILDER_IMAGE`'s default in `make/build.mk` only if the tag shape changed;
   it is derived from `versions.env`, so a version move needs no edit there. The
   same goes for the workflows, which compute the tag with the same `sed`.
6. Update the version the README states, and the literal in
   `scripts/check_builder_pin_test.sh` whose test is named for being *currently
   correct* — a stale literal there leaves the test passing while no longer
   testing what it says. Nothing else: every other mention of the number is
   prose that rots, and belongs written as "the pin" instead.
7. Re-run `./build_and_push.sh` and confirm `docker manifest inspect` shows both
   architectures on the new tag. The image is not a build product of CI; if you
   do not push it, nothing does.

Steps 1-3 are gated: `build_and_push.sh` refuses to build when `versions.env`,
`rust-toolchain.toml`, and `Cargo.toml` do not all name the same version.
Steps 6 and 7 are not gated — they are the ones to check by hand.

`RUSTUP_VERSION` moves on its own cadence, whenever rustup itself releases.
It is the installer, not the compiler; `fetch-shas` is the only way to move it.

## What this does NOT do

It does not cache compiled crates. That is `CARGO_TARGET_DIR`'s job, and
`make/build.mk` gives each architecture its own directory under
`rustd/target/musl-<arch>/` so the two never collide — which is what the old
recipe's `rm -rf target/dist` existed to prevent, at the cost of a cold compile
on every single run. The image saves the package install; the per-arch target
directory saves the compile. The second is the larger number.
