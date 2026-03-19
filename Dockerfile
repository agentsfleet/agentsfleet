# syntax=docker/dockerfile:1
# UseZombie — runtime image from prebuilt host cross-compiled linux binaries.
# Supports linux/amd64 and linux/arm64 via TARGETARCH.

FROM debian:trixie-slim AS prebuilt
ARG TARGETARCH
COPY dist/zombied-linux-amd64 /tmp/zombied-linux-amd64
COPY dist/zombied-linux-arm64 /tmp/zombied-linux-arm64
RUN set -eux; \
    mkdir -p /out; \
    case "$TARGETARCH" in \
      amd64) cp /tmp/zombied-linux-amd64 /out/zombied ;; \
      arm64) cp /tmp/zombied-linux-arm64 /out/zombied ;; \
      *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    chmod +x /out/zombied

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    bubblewrap \
    ca-certificates \
    git \
    wget \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=prebuilt /out/zombied /usr/local/bin/zombied
COPY config ./config
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s CMD wget -qO- http://localhost:3000/healthz || exit 1
CMD ["zombied", "serve"]
