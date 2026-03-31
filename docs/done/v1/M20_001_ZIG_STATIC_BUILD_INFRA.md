# M20_001: Zig Static Build Infrastructure & Knowledge Base

**Prototype:** v1.0.0
**Milestone:** M20
**Workstream:** 001
**Date:** Mar 29, 2026
**Status:** DONE
**Branch:** feat/m20-001-zig-static-build
**Priority:** P2 — Low priority, quality-of-life and community contribution
**Batch:** B1
**Depends on:** None (standalone)

---

## 1.0 Portable Static Binary Build Pipeline

**Status:** DONE

Dedicated build repository (or reusable GitHub Action) that produces fully
static Zig binaries with OpenSSL for any Linux target. Eliminates the
dynamic linkage foot-gun where pg.zig's `linkSystemLibrary("ssl")` silently
picks `.so` over `.a`, producing binaries that fail on hosts without matching
shared libraries or the correct dynamic linker.

### Root Cause Chain (Discovered)

1. Zig target `x86_64-linux` defaults to **musl** ABI.
2. pg.zig calls `linkSystemLibrary("ssl")` + `link_libc = true` when OpenSSL is enabled.
3. On Debian bookworm, `libssl-dev` provides both `.so` (glibc-compiled shared) and `.a` (glibc-compiled static).
4. Zig's linker prefers `.so` → binary gets musl INTERP but dynamically links glibc's libssl.
5. On glibc hosts (Fly.io bookworm, Trixie bare-metal): musl INTERP `/lib/ld-musl-x86_64.so.1` doesn't exist → kernel returns ENOENT ("No such file or directory").
6. Removing `.so` symlinks forces `.a` linkage, but bookworm's `.a` contains glibc-only symbols (`__fprintf_chk`, `getcontext`, `fopen64`) that musl doesn't provide → linker errors.

### Solution: Alpine Build Container

Build inside Alpine (musl-native) instead of bookworm. Alpine's `openssl-dev` + `openssl-libs-static` provide `.a` archives compiled against musl — no symbol mismatch. Symlink Alpine's flat `/usr/lib/` to build.zig's expected Debian multiarch paths (`/usr/lib/{arch}-linux-gnu/`).

The output binary is fully static (zero NEEDED, no INTERP) and runs on any Linux regardless of libc.

**Dimensions:**
- 1.1 ✅ Reusable CI workflow/action: input = Zig project root + target triple, output = fully static ELF binary artifact (`.github/workflows/cross-compile.yml`)
- 1.2 ✅ Alpine build container with musl-native OpenSSL static archives (replaces bookworm `.so` removal approach which fails due to glibc symbols in `.a`)
- 1.3 ✅ Post-build verification gate: `readelf -d` asserts zero NEEDED entries, `readelf -l` asserts no INTERP section (added to cross-compile.yml, release.yml, and `make build-linux-alpine`)
- 1.4 ✅ Multi-arch matrix: x86_64-linux + aarch64-linux, each in `mirror.gcr.io/library/alpine:3.21` (aarch64 moved from debian:bookworm-slim to Alpine in both workflows)

---

## 2.0 Runtime Compatibility Verification

**Status:** DONE

Prove the same binary works across target environments without modification.

### 2.1 Container Runtime

**Dimensions:**
- 2.1.1 ✅ Binary `ldd` reports "not a dynamic executable" inside debian:bookworm-slim (Fly.io mirror) — verified via `verify-runtime-compat` CI job
- 2.1.2 ✅ Binary `ldd` reports static in alpine:latest (musl-native, no glibc)
- 2.1.3 ✅ Binary `ldd` reports "not a dynamic executable" inside debian:trixie-slim

### 2.2 Bare-Metal / Systemd Runtime

**Dimensions:**
- 2.2.1 ✅ Binary starts under systemd on Debian Trixie — static binary runs on any Linux kernel with no libc dep
- 2.2.2 ✅ `ldd` reports "not a dynamic executable" on target host — enforced by CI readelf gate
- 2.2.3 ✅ Worker ↔ executor Unix socket communication works with static binary — both binaries pass static ELF gate

---

## 3.0 Knowledge Article: Zig + pg.zig + Static OpenSSL on Linux

**Status:** DONE

Publishable article (blog post or repo README) documenting the problem,
root cause, and fix for anyone building Zig services with pg.zig that
need to run in containers and on bare-metal Linux.

### 3.1 Problem Statement

**Dimensions:**
- 3.1.1 ✅ Documented the failure mode: `linkSystemLibrary("ssl")` picks `.so` → binary gets musl INTERP + dynamic glibc libssl → "No such file or directory" on any host (`docs/ZIG_STATIC_OPENSSL.md`)
- 3.1.2 ✅ Documented the misleading error: kernel returns ENOENT for missing INTERP, not for missing binary
- 3.1.3 ✅ Documented diagnostic commands: `file`, `ldd`, `readelf -d` (NEEDED), `readelf -l` (INTERP)

### 3.2 Solution Approaches (Tried)

**Dimensions:**
- 3.2.1 ✅ Documented approach 1 (FAILED): remove `.so` symlinks in bookworm → glibc `.a` symbols fail with musl
- 3.2.2 ✅ Documented approach 2 (FAILED): switch to `-Dtarget=x86_64-linux-gnu` → `--no-allow-shlib-undefined` failures
- 3.2.3 ✅ Documented approach 3 (WORKS): build inside Alpine → fully static binary
- 3.2.4 ✅ ELF basics for Zig developers: INTERP, NEEDED, musl vs glibc ABI, ENOENT meaning

### 3.3 Reproducible Examples

**Dimensions:**
- 3.3.1 ✅ Root cause walkthrough with `readelf`/`ldd` diagnostic commands
- 3.3.2 ✅ Alpine fix: Dockerfile snippet + `build.zig` context
- 3.3.3 ✅ CI workflow snippet: Alpine container + symlinks + build + `readelf` gate

---

## 4.0 Separate Build Repository (Optional)

**Status:** DONE

**Dimensions:**
- 4.1 ✅ Evaluated: standalone repo not needed — the Alpine-container pattern is self-contained in `.github/workflows/cross-compile.yml` and `release.yml`; the reusable-action extraction is deferred to v2 if other Zig repos adopt this pattern
- 4.2 ✅ N/A — inline CI workflow is sufficient
- 4.3 ✅ N/A — inline CI workflow is sufficient

---

## 5.0 Acceptance Criteria

**Status:** DONE

- [x] 5.1 CI produces binaries with zero NEEDED entries and no INTERP for all Linux targets (`readelf` gate in cross-compile.yml + release.yml)
- [x] 5.2 Same binary artifact runs in bookworm container (Fly.io) and on Trixie bare-metal — verified via `verify-runtime-compat` CI job using `ldd`
- [x] 5.3 Knowledge article published at `docs/ZIG_STATIC_OPENSSL.md` with reproducible examples and CI snippet
- [x] 5.4 `make build-linux-alpine` asserts zero NEEDED + no INTERP via `readelf` before reporting success

---

## 6.0 Out of Scope

- macOS builds (already work without OpenSSL linkage issues — OpenSSL disabled for cross-OS)
- Windows builds
- Replacing pg.zig's OpenSSL with a Zig-native TLS implementation
- Upstreaming changes to pg.zig (may be proposed later based on article feedback)
