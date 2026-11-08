---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Build the Rust CI base image

## What this does

Builds and pushes `ghcr.io/agentsfleet/ci-rust-alpine` for amd64 and arm64.
The image bakes the musl build packages, `git`, `pkgconf`, `ca-certificates` and the `rustfmt`, `clippy` and `llvm-tools` components.
It does not cache compiled crates.
`make/build.mk` gives each architecture its own `rustd/target/musl-<arch>/` directory for that.

| Consumer | How it gets the tag |
|---|---|
| `make/build.mk` `_dist-daemons` (and `build`) | `BUILDER_IMAGE`, derived from `versions.env` |
| `make candidate-native-linux` in `build-candidate.yml` | `BUILDER_IMAGE` |
| `warm-cache.yml` | the same `sed` over `versions.env` |

The tag is `<RUST_VERSION>-alpine<ALPINE_SERIES>`, derived and never pasted.
The ast-grep rule `wf-builder-literal`, run by `check-gh-actions-valid`, refuses a literal builder tag in a workflow.
The rule `wf-action-pin` refuses a moving action ref, and Dependabot moves the pins.

## Before you begin

- Run `gh auth login` with the `write:packages` scope.
- Install Docker Desktop or `docker-buildx-plugin`.
- Human: authenticate to the GitHub Container Registry (GHCR) once.

## What is pinned

- `versions.env` holds `RUST_VERSION`, `ALPINE_SERIES`, `RUSTUP_VERSION`, `RUSTUP_SHA256_X86_64_MUSL` and `RUSTUP_SHA256_AARCH64_MUSL`.
- The base is plain Alpine, and rustup installs `RUST_VERSION`, because `rust:*-alpine` tags lag a rustc release by weeks.
- `ALPINE_SERIES` names a patch series, such as `3.24`, so musl and gcc float by patch while rustc stays exact.
- `fetch-shas` reads the checksum upstream publishes at `<installer-url>.sha256`, downloads the binary, and refuses when they disagree.
- That check catches a bad transfer, not a compromised `static.rust-lang.org`; rustup publishes no signature.
- Never paste a checksum by hand.

## Steps

1. Change into the directory.

   ```bash
   cd playbooks/operations/ci_rust_images
   ```

2. Run the subcommand you need.

   ```bash
   ./build_and_push.sh --no-push          # this architecture only, loaded locally
   ./build_and_push.sh                    # both architectures, pushed to GHCR
   ./build_and_push.sh --revision r2      # new tag suffix, pinned tag untouched
   ./build_and_push.sh fetch-shas         # refresh the rustup-init checksums
   ```

   ```text
     ✓ ghcr.io/agentsfleet/ci-rust-alpine:<RUST_VERSION>-alpine<ALPINE_SERIES>
   ```

   `--no-push` builds one architecture because `docker buildx --load` cannot load a multi-platform manifest.
   Push both architectures; `--platform` on a foreign host compiles under emulation.

Nothing rebuilds the image on merge.
Any change to `Dockerfile.alpine` or `versions.env` needs `./build_and_push.sh` before the Pull Request (PR) merges.
Use `--revision r2` once a merged commit names the tag; overwrite only a tag nothing on `main` names.

## Move the Rust version

1. Move `channel` in `rustd/rust-toolchain.toml`.
   Move any Rust version in your own mise config too; its `RUSTUP_TOOLCHAIN` export overrides the file.
2. Move `rust-version` in `rustd/Cargo.toml` to the same value.
3. Move `RUST_VERSION` in `versions.env` to the same value.
4. Run `./build_and_push.sh`.
   It refuses when steps 1 to 3 disagree.
5. Edit `BUILDER_IMAGE` in `make/build.mk` only if the tag shape changed.
6. Update the Rust version in `README.md`.
   Write other mentions as "the pin", not a number.
7. Confirm both architectures are on the new tag.

   ```bash
   docker manifest inspect ghcr.io/agentsfleet/ci-rust-alpine:<RUST_VERSION>-alpine<ALPINE_SERIES>
   ```

   ```text
   <varies: one manifest each for linux/amd64 and linux/arm64>
   ```

Steps 6 and 7 are not gated; check them by hand.
`RUSTUP_VERSION` moves when rustup releases, and only through `fetch-shas`.
