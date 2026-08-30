# Playbook — CI Rust Base Image

**Updated:** Aug 30, 2026
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

The Alpine floor is a patch series rather than a floating `alpine3`, because
this image decides what compiles the shipped binary and a base that moves under
a release is a different binary nobody chose. The repository holds GitHub
Actions to the same rule (`audits/gh-actions-runtime.sh`).

## Build and publish

```bash
cd playbooks/operations/ci_rust_images

./build_and_push.sh --no-push          # this arch only, loaded locally
./build_and_push.sh                    # both arches, pushed to GHCR
./build_and_push.sh --revision r2      # iterate without moving a pinned tag
```

`--no-push` builds a single architecture because `docker buildx --load` cannot
accept a multi-platform manifest. That is a docker limitation, not a choice.

## Moving the Rust version

1. Move `channel` in `rustd/rust-toolchain.toml` and the mise tools config together.
2. Move `RUST_VERSION` in `versions.env` to match.
3. `./build_and_push.sh` — it refuses if step 1 and step 2 disagree.
4. Move `BUILDER_IMAGE`'s default in `make/build.mk` only if the tag shape changed;
   it is derived from `versions.env`, so a version move needs no edit there.

## What this does NOT do

It does not cache compiled crates. That is `CARGO_TARGET_DIR`'s job, and
`make/build.mk` gives each architecture its own directory under
`rustd/target/musl-<arch>/` so the two never collide — which is what the old
recipe's `rm -rf target/dist` existed to prevent, at the cost of a cold compile
on every single run. The image saves the package install; the per-arch target
directory saves the compile. The second is the larger number.
