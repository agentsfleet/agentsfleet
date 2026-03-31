# Zig + pg.zig + Static OpenSSL on Linux

How to produce fully static Linux binaries when your Zig project uses pg.zig
with OpenSSL — and why the naive approach silently produces broken binaries.

---

## The Failure Mode

You build your Zig service on a Debian/Ubuntu host, deploy to another Linux host
or container, and get:

```
bash: ./zombied: No such file or directory
```

The binary exists. The error is from the kernel, not the shell. The real cause:

```
$ readelf -l zombied | grep INTERP
      [Requesting program interpreter: /lib/ld-musl-x86_64.so.1]

$ ldd zombied
        linux-musl: /lib/ld-musl-x86_64.so.1 (0x...)
        libssl.so.3 => not found
        libcrypto.so.3 => not found
```

The binary is **not** static. It was linked against musl's dynamic linker
(`/lib/ld-musl-x86_64.so.1`) but ships on a glibc host where that path doesn't
exist. The kernel returns ENOENT for the missing interpreter, not for the binary.

---

## Root Cause Chain

1. Zig target `x86_64-linux` defaults to the **musl** ABI.
2. `pg.zig` calls `linkSystemLibrary("ssl")` + sets `link_libc = true` when
   OpenSSL is enabled.
3. On Debian bookworm, `libssl-dev` installs both `.so` (glibc-compiled shared)
   and `.a` (glibc-compiled static) under `/usr/lib/x86_64-linux-gnu/`.
4. Zig's linker prefers `.so` over `.a` → the binary gets a musl INTERP header
   but dynamically links glibc's libssl.
5. On any glibc host (Fly.io bookworm, Debian Trixie bare-metal):
   `/lib/ld-musl-x86_64.so.1` doesn't exist → kernel returns ENOENT.
6. On any Alpine/musl host: musl's dynamic linker exists, but `libssl.so.3`
   (glibc-compiled) isn't present → segfault or "not found".

### Diagnostic commands

```bash
# Is the binary static or dynamic?
file ./zombied
# "statically linked" → good; "dynamically linked" → broken

# What dynamic libraries does it reference?
ldd ./zombied
# "not a dynamic executable" → good
# any other output → broken

# Check the dynamic linker path embedded in the binary
readelf -l ./zombied | grep INTERP
# empty → good; any output → broken

# List all dynamic library dependencies
readelf -d ./zombied | grep NEEDED
# empty → good; any output → broken
```

---

## Approaches That Don't Work

### Approach 1: Remove `.so` symlinks in bookworm

Force Zig to use the `.a` by removing the shared library symlinks:

```bash
rm /usr/lib/x86_64-linux-gnu/libssl.so
rm /usr/lib/x86_64-linux-gnu/libcrypto.so
```

**Why it fails:** bookworm's `.a` archives (`libssl.a`, `libcrypto.a`) are
compiled against glibc. When linked into a musl-target binary, the linker
reports undefined symbols:

```
undefined reference to `__fprintf_chk'
undefined reference to `getcontext'
undefined reference to `fopen64'
undefined reference to `makecontext'
undefined reference to `setcontext'
```

These are glibc-internal symbols that musl doesn't provide. The `.a` is
glibc-only; it can't be linked into a musl binary.

### Approach 2: Switch target to `x86_64-linux-gnu`

Use the glibc ABI explicitly:

```bash
zig build -Dtarget=x86_64-linux-gnu
```

**Why it fails:** Zig's linker uses `--no-allow-shlib-undefined` by default.
The glibc-shared `libcrypto.so` itself references dynamic symbols (`dlopen`,
`pthread_*`, `fstat`) that aren't available in a statically-linked glibc
context. The linker rejects this.

---

## The Fix: Build Inside Alpine

Alpine Linux uses musl as its native libc. Its `openssl-libs-static` package
provides `.a` archives compiled against musl — no glibc symbols, no ABI mismatch.

### Dockerfile / CI container

```dockerfile
FROM mirror.gcr.io/library/alpine:3.21

RUN apk add --no-cache \
    git \
    openssl-dev \
    openssl-libs-static \
    ca-certificates \
    xz \
    binutils

# Alpine stores libs flat under /usr/lib/ rather than the Debian multiarch
# path that build.zig expects.  Symlink to satisfy the path lookup.
ARG ARCH=x86_64
RUN mkdir -p /usr/lib/${ARCH}-linux-gnu /usr/include/${ARCH}-linux-gnu && \
    ln -sf /usr/lib/libssl.a     /usr/lib/${ARCH}-linux-gnu/libssl.a && \
    ln -sf /usr/lib/libcrypto.a  /usr/lib/${ARCH}-linux-gnu/libcrypto.a && \
    ln -sf /usr/include/openssl  /usr/include/${ARCH}-linux-gnu/openssl
```

```bash
zig build -Doptimize=ReleaseSafe -Dtarget=x86_64-linux
```

The output binary is fully static: zero `NEEDED` entries, no `INTERP` section.
It runs on any Linux regardless of libc.

### GitHub Actions (matrix for both architectures)

```yaml
jobs:
  build-linux:
    strategy:
      matrix:
        include:
          - target: x86_64-linux
            os: ubuntu-latest
            arch: x86_64
          - target: aarch64-linux
            os: ubuntu-24.04-arm
            arch: aarch64
    runs-on: ${{ matrix.os }}
    container:
      image: mirror.gcr.io/library/alpine:3.21
    steps:
      - name: Install build dependencies
        run: |
          apk add --no-cache git openssl-dev openssl-libs-static ca-certificates xz binutils
          mkdir -p /usr/lib/${{ matrix.arch }}-linux-gnu /usr/include/${{ matrix.arch }}-linux-gnu
          ln -sf /usr/lib/libssl.a    /usr/lib/${{ matrix.arch }}-linux-gnu/libssl.a
          ln -sf /usr/lib/libcrypto.a /usr/lib/${{ matrix.arch }}-linux-gnu/libcrypto.a
          ln -sf /usr/include/openssl /usr/include/${{ matrix.arch }}-linux-gnu/openssl

      - uses: actions/checkout@v6
      - uses: mlugg/setup-zig@v2
        with:
          version: 0.15.2

      - name: Build
        run: zig build -Doptimize=ReleaseSafe -Dtarget=${{ matrix.target }}

      - name: Assert static binary (zero NEEDED, no INTERP)
        run: |
          for bin in zig-out/bin/zombied zig-out/bin/zombied-executor; do
            if readelf -d "$bin" | grep -q " (NEEDED)"; then
              echo "FAIL: $bin has dynamic NEEDED entries"
              exit 1
            fi
            if readelf -l "$bin" | grep -q "INTERP"; then
              echo "FAIL: $bin has dynamic linker INTERP"
              exit 1
            fi
            echo "✓ $bin: fully static"
          done
```

---

## Runtime Verification

After building, verify the binary runs across all target environments:

```bash
# In bookworm-slim (Fly.io container runtime)
docker run --rm -v $(pwd)/dist:/test:ro debian:bookworm-slim \
  ldd /test/zombied
# → "not a dynamic executable"

# In alpine:latest (musl-native)
docker run --rm -v $(pwd)/dist:/test:ro alpine:latest \
  ldd /test/zombied
# → "Not a valid dynamic program" (Alpine's musl ldd phrasing)

# In trixie-slim (bare-metal worker target)
docker run --rm -v $(pwd)/dist:/test:ro debian:trixie-slim \
  ldd /test/zombied
# → "not a dynamic executable"
```

---

## ELF Concepts for Zig Developers

| ELF field | What it means | Danger signal |
|---|---|---|
| `INTERP` program header | Path to the dynamic linker the kernel must exec to load the binary | Any value — means the binary is dynamic |
| `NEEDED` dynamic entry | Shared library that must be present at runtime | Any value — means the binary is dynamic |
| ABI (musl vs glibc) | Which C library the `.a` or `.so` was compiled against | Mixing ABIs → undefined symbols at link time |

**Why `ENOENT` instead of a clear error:** The kernel `execve` syscall uses the
`INTERP` path to find the dynamic linker, then execs the linker. If the
interpreter path doesn't exist, `execve` returns `ENOENT`. The shell reports
"No such file or directory" — about the *interpreter*, not the binary.

---

## Local Verification (mirrors CI)

```bash
make build-linux-alpine
```

Runs the Alpine build inside Docker and asserts `readelf` passes before
reporting success. Equivalent to what CI runs on every push.
