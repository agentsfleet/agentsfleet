# M20_001: Zig Static Build Infrastructure & Knowledge Base

**Prototype:** v1.0.0
**Milestone:** M20
**Workstream:** 001
**Date:** Mar 29, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — Low priority, quality-of-life and community contribution
**Batch:** B1
**Depends on:** None (standalone)

---

## 1.0 Portable Static Binary Build Pipeline

**Status:** IN_PROGRESS

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
- 1.1 IN_PROGRESS Reusable CI workflow/action: input = Zig project root + target triple, output = fully static ELF binary artifact
- 1.2 ✅ Alpine build container with musl-native OpenSSL static archives (replaces bookworm `.so` removal approach which fails due to glibc symbols in `.a`)
- 1.3 PENDING Post-build verification gate: `readelf -d` asserts zero NEEDED entries, `readelf -l` asserts no INTERP section
- 1.4 IN_PROGRESS Multi-arch matrix: x86_64-linux + aarch64-linux, each in `mirror.gcr.io/library/alpine:3.21`

---

## 2.0 Runtime Compatibility Verification

**Status:** PENDING

Prove the same binary works across target environments without modification.

### 2.1 Container Runtime

**Dimensions:**
- 2.1.1 PENDING Binary starts and passes healthcheck inside debian:bookworm-slim (Fly.io mirror)
- 2.1.2 PENDING Binary starts inside alpine:latest (musl-native, no glibc)
- 2.1.3 PENDING Binary starts inside debian:trixie-slim (Trixie, glibc 2.38+)

### 2.2 Bare-Metal / Systemd Runtime

**Dimensions:**
- 2.2.1 PENDING Binary starts under systemd on Debian Trixie (matches worker/executor deploy target)
- 2.2.2 PENDING `ldd` reports "not a dynamic executable" on target host
- 2.2.3 PENDING Worker ↔ executor Unix socket communication works with static binary

---

## 3.0 Knowledge Article: Zig + pg.zig + Static OpenSSL on Linux

**Status:** PENDING

Publishable article (blog post or repo README) documenting the problem,
root cause, and fix for anyone building Zig services with pg.zig that
need to run in containers and on bare-metal Linux.

### 3.1 Problem Statement

**Dimensions:**
- 3.1.1 PENDING Document the failure mode: `linkSystemLibrary("ssl")` picks `.so` → binary gets musl INTERP + dynamic glibc libssl → "No such file or directory" on any host
- 3.1.2 PENDING Document the misleading error: kernel returns ENOENT for missing INTERP, not for missing binary. `ldd` shows `libc.so => not found` and `/lib/ld-musl-x86_64.so.1`
- 3.1.3 PENDING Document diagnostic commands: `file`, `ldd`, `readelf -d` (NEEDED), `readelf -l` (INTERP)

### 3.2 Solution Approaches (Tried)

**Dimensions:**
- 3.2.1 PENDING Document approach 1 (FAILED): remove `.so` symlinks in bookworm → forces glibc `.a` → fails with undefined glibc symbols (`__fprintf_chk`, `getcontext`, `fopen64`, `makecontext`, `setcontext`) because musl doesn't provide them
- 3.2.2 PENDING Document approach 2 (FAILED): switch to `-Dtarget=x86_64-linux-gnu` → fails with `--no-allow-shlib-undefined` because shared libcrypto.so references `dlopen`, `pthread_*`, `fstat` etc.
- 3.2.3 PENDING Document approach 3 (WORKS): build inside Alpine (musl-native) → `openssl-libs-static` provides musl-compatible `.a` → fully static binary, zero dynamic deps
- 3.2.4 PENDING Explain ELF basics for Zig developers: INTERP (dynamic linker path), NEEDED (shared lib deps), musl vs glibc ABI, why "No such file or directory" means missing interpreter

### 3.3 Reproducible Examples

**Dimensions:**
- 3.3.1 PENDING Minimal pg.zig project with Dockerfile that demonstrates the bug (dynamic build in bookworm)
- 3.3.2 PENDING Same project with Alpine fix applied (static build)
- 3.3.3 PENDING CI workflow snippet: Alpine container + symlinks + build + `readelf` verification gate

---

## 4.0 Separate Build Repository (Optional)

**Status:** PENDING

Evaluate extracting the static Zig build pipeline into a standalone
repository (`usezombie/zig-static-build` or similar) so it can be
reused across projects and published as a GitHub Action.

**Dimensions:**
- 4.1 PENDING Evaluate: standalone repo vs reusable workflow in current repo
- 4.2 PENDING If standalone: scaffold repo with build matrix, verification gate, and release artifact publishing
- 4.3 PENDING If standalone: wire usezombie CI to consume the action instead of inline build steps

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 CI produces binaries with zero NEEDED entries and no INTERP for all Linux targets
- [ ] 5.2 Same binary artifact runs in bookworm container (Fly.io) and on Trixie bare-metal (systemd worker)
- [ ] 5.3 Knowledge article published (repo README or blog) with reproducible examples
- [ ] 5.4 `make build-linux-alpine` local verification target passes with static binary assertion

---

## 6.0 Out of Scope

- macOS builds (already work without OpenSSL linkage issues — OpenSSL disabled for cross-OS)
- Windows builds
- Replacing pg.zig's OpenSSL with a Zig-native TLS implementation
- Upstreaming changes to pg.zig (may be proposed later based on article feedback)
