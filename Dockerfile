# UseZombie — Zig binary build + runtime
# Multi-stage: build with Zig toolchain, deploy static binary only
# Supports linux/amd64 and linux/arm64 via TARGETARCH

FROM debian:trixie-slim AS build
ARG TARGETARCH
ARG ZIG_VERSION=0.15.2
WORKDIR /build
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    xz-utils \
 && rm -rf /var/lib/apt/lists/*
RUN set -eux; \
    ZIG_ARCH=$(case "$TARGETARCH" in amd64) echo x86_64;; arm64) echo aarch64;; *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1;; esac); \
    ZIG_URL="https://ziglang.org/download/${ZIG_VERSION}/zig-${ZIG_ARCH}-linux-${ZIG_VERSION}.tar.xz"; \
    curl -fL --retry 5 --retry-all-errors --retry-delay 2 -o zig.tar.xz "$ZIG_URL"; \
    test "$(wc -c < zig.tar.xz)" -gt 10000000; \
    tar -xJf zig.tar.xz; \
    rm -f zig.tar.xz; \
    ln -s /build/zig-${ZIG_ARCH}-linux-${ZIG_VERSION}/zig /usr/local/bin/zig
COPY build.zig build.zig.zon ./
COPY src ./src
COPY config ./config
COPY schema ./schema
RUN zig build -Doptimize=ReleaseSafe

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    bubblewrap \
    ca-certificates \
    git \
    wget \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /build/zig-out/bin/zombied /usr/local/bin/zombied
COPY config ./config
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s CMD wget -qO- http://localhost:3000/healthz || exit 1
CMD ["zombied", "serve"]
