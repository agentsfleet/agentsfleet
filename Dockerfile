# UseZombie — Zig binary build + runtime
# Multi-stage: build with Zig toolchain, deploy static binary only
# Supports linux/amd64 and linux/arm64 via TARGETARCH

FROM alpine:3.21 AS build
ARG TARGETARCH
WORKDIR /build
RUN apk add --no-cache curl xz git
RUN ZIG_ARCH=$(case "$TARGETARCH" in amd64) echo x86_64;; arm64) echo aarch64;; esac) && \
    curl -L "https://ziglang.org/download/0.15.2/zig-linux-${ZIG_ARCH}-0.15.2.tar.xz" | tar xJ && \
    ln -s /build/zig-linux-${ZIG_ARCH}-0.15.2/zig /usr/local/bin/zig
COPY build.zig build.zig.zon ./
COPY src ./src
COPY config ./config
RUN zig build -Doptimize=ReleaseSafe

FROM alpine:3.21
RUN apk add --no-cache git bubblewrap
WORKDIR /app
COPY --from=build /build/zig-out/bin/zombied /usr/local/bin/zombied
COPY config ./config
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s CMD wget -qO- http://localhost:3000/healthz || exit 1
CMD ["zombied", "serve"]
