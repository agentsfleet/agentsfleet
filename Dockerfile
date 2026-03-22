# UseZombie — runtime container
# Binary must be pre-built before docker build:
#   CI:    binaries job produces dist/zombied-linux-{amd64,arm64}
#   Local: zig build -Dtarget=x86_64-linux -Doptimize=ReleaseSafe
#          mkdir -p dist && cp zig-out/bin/zombied dist/zombied-linux-amd64

FROM debian:trixie-slim
ARG TARGETARCH=amd64
RUN apt-get update && apt-get install -y --no-install-recommends \
    bubblewrap \
    ca-certificates \
    git \
    wget \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY dist/zombied-linux-${TARGETARCH} /usr/local/bin/zombied
RUN chmod +x /usr/local/bin/zombied
COPY config ./config
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s CMD wget -qO- http://localhost:3000/healthz || exit 1
CMD ["/usr/local/bin/zombied", "serve"]
