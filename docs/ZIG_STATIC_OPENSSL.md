# Zig + Static OpenSSL on Linux: The Error That Isn't About What You Think

> Originally written as an internal reference. Reformatted for sharing.

---

You build your Zig service. You deploy it. You get this:

```
bash: ./zombied: No such file or directory
```

The binary is there. `ls -la` confirms it. You double-check the path. Still "No such file or directory."

This is one of those errors where the shell is technically right and completely misleading at the same time.

---

## What's Actually Happening

The kernel's `execve` syscall reads the `INTERP` program header from the ELF binary before executing it. That header names the dynamic linker — something like `/lib/ld-musl-x86_64.so.1`. The kernel needs to exec that linker first to load your binary.

If the interpreter path doesn't exist: `ENOENT`. The shell reports it as "No such file or directory" — but it's talking about the *interpreter*, not your binary.

```bash
# Diagnose it:
readelf -l ./zombied | grep INTERP
# [Requesting program interpreter: /lib/ld-musl-x86_64.so.1]
```

You built a binary with musl's dynamic linker embedded. Deployed it to a glibc host (Fly.io bookworm, Debian bare-metal). `/lib/ld-musl-x86_64.so.1` doesn't exist there. `ENOENT`.

---

## How You Got Here: The Root Cause Chain

1. **Zig target `x86_64-linux` defaults to the musl ABI.**
2. `pg.zig` calls `linkSystemLibrary("ssl")` when OpenSSL is enabled.
3. On Debian bookworm, `libssl-dev` installs *shared* `.so` files (glibc-compiled) under `/usr/lib/x86_64-linux-gnu/`.
4. Zig's linker prefers `.so` over `.a` — so your binary gets musl's `INTERP` header but links glibc's `libssl.so.3` dynamically.
5. Broken on glibc hosts: musl interpreter missing. Broken on Alpine/musl hosts: glibc `libssl.so.3` missing.

The binary works on exactly zero deployment targets.

---

## Two Approaches That Don't Work

**Remove the `.so` symlinks on Debian.** This forces the linker to use `.a` — but Debian's static archives are glibc-compiled. When linked into a musl-target binary you get:

```
undefined reference to `__fprintf_chk'
undefined reference to `getcontext'
```

glibc-internal symbols. musl doesn't have them.

**Switch to `x86_64-linux-gnu`.** Now you're targeting glibc explicitly. But `libcrypto.so` references `dlopen`, `pthread_*`, `fstat` — symbols that aren't available in a static glibc context. Zig's linker rejects it.

---

## The Fix: Build Inside Alpine

Alpine uses musl as its native libc. Its `openssl-libs-static` package ships `.a` archives compiled against musl. No ABI mismatch. No glibc symbols.

```dockerfile
FROM mirror.gcr.io/library/alpine:3.21

RUN apk add --no-cache \
    git openssl-dev openssl-libs-static \
    ca-certificates xz binutils

# Alpine stores libs flat under /usr/lib/ — symlink to the path build.zig expects.
ARG ARCH=x86_64
RUN mkdir -p /usr/lib/${ARCH}-linux-gnu /usr/include/${ARCH}-linux-gnu && \
    ln -sf /usr/lib/libssl.a     /usr/lib/${ARCH}-linux-gnu/libssl.a && \
    ln -sf /usr/lib/libcrypto.a  /usr/lib/${ARCH}-linux-gnu/libcrypto.a && \
    ln -sf /usr/include/openssl  /usr/include/${ARCH}-linux-gnu/openssl
```

```bash
zig build -Doptimize=ReleaseSafe -Dtarget=x86_64-linux
```

The output: zero `NEEDED` entries, no `INTERP` section. Runs on any Linux, any libc.

---

## Verify Before You Ship

Two checks. Both must pass before a binary ships:

```bash
# 1. Structural check (fast, no Docker needed)
readelf -d ./zombied | grep NEEDED      # must be empty
readelf -l ./zombied | grep INTERP      # must be empty

# 2. Runtime check (catches edge cases the structural check misses)
docker run --rm -v $(pwd)/dist:/test:ro debian:bookworm-slim \
  ldd /test/zombied
# → "not a dynamic executable"

docker run --rm -v $(pwd)/dist:/test:ro debian:trixie-slim \
  ldd /test/zombied
# → "not a dynamic executable"

docker run --rm -v $(pwd)/dist:/test:ro mirror.gcr.io/library/alpine:3.21 \
  ldd /test/zombied
# → "Not a valid dynamic program"
```

We run this in CI as `verify-runtime-compat` on every push and every release. If it doesn't pass locally in a Docker container, it won't pass in prod.

---

## For GitHub Actions (ARM too)

aarch64 runners can't run JS actions inside Alpine containers. Use the bare ARM runner and invoke Alpine via `docker run`:

```yaml
jobs:
  build-linux:
    strategy:
      matrix:
        include:
          - { target: x86_64-linux,  os: ubuntu-latest,    arch: x86_64  }
          - { target: aarch64-linux, os: ubuntu-24.04-arm, arch: aarch64 }
    runs-on: ${{ matrix.os }}
    container:
      # Only works on x86_64; aarch64 uses the bare runner + docker run below
      image: ${{ matrix.arch == 'x86_64' && 'mirror.gcr.io/library/alpine:3.21' || '' }}
```

For aarch64 specifically:

```yaml
- name: Build aarch64-linux inside Alpine
  run: |
    docker run --rm --platform linux/arm64 \
      -v "$GITHUB_WORKSPACE:/src:ro" \
      -v "$GITHUB_WORKSPACE/zig-out:/zig-out" \
      mirror.gcr.io/library/alpine:3.21 \
      sh -c 'apk add --no-cache openssl-dev openssl-libs-static ca-certificates xz wget binutils && ...'
```

---

## The Short Version (Twitter/X thread)

**Thread: The "No such file or directory" error that is not about your binary**

1/ You deploy a Zig binary. `bash: ./zombied: No such file or directory`. The file exists. ls confirms it. What's happening?

2/ `execve` reads the ELF `INTERP` header before running your binary. That header names the dynamic linker. If the linker path doesn't exist on the target host: `ENOENT`. Shell blames your binary. Kernel blames the interpreter.

3/ Root cause: Zig's `x86_64-linux` target defaults to musl ABI. But `libssl-dev` on Debian ships glibc `.so` files. Zig's linker picks `.so` over `.a`. Your binary gets musl's INTERP but links glibc's libssl. Broken everywhere.

4/ The fix: build inside Alpine. Alpine's `openssl-libs-static` ships musl-native `.a` archives. No ABI mismatch. The output binary has zero NEEDED entries, no INTERP section. Runs on any Linux.

5/ Two symlinks needed: Alpine stores libs under `/usr/lib/` flat. Zig's build script looks under `/usr/lib/x86_64-linux-gnu/`. Symlink them. That's it.

6/ Always run `readelf -d ./binary | grep NEEDED` + `readelf -l ./binary | grep INTERP` before shipping. Both must be empty. If they're not, the binary is dynamic and will fail somewhere.

---

## Local Verification

```bash
make build-linux-alpine
```

Runs the Alpine build inside Docker and asserts `readelf` passes before reporting success. Same as CI.
