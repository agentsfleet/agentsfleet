# syntax=docker/dockerfile:1
FROM mirror.gcr.io/library/debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4

# A session CA is public trust material; credentials never enter this image.
RUN --mount=type=secret,id=session-ca,required=false \
    set -eu; \
    mkdir -p /usr/local/share/ca-certificates; \
    if [ -f /run/secrets/session-ca ]; then \
      cp /run/secrets/session-ca /usr/local/share/ca-certificates/session-proxy.crt; \
    fi; \
    if command -v update-ca-certificates >/dev/null; then update-ca-certificates; fi; \
    apt-get update && apt-get install --yes --no-install-recommends \
      bubblewrap ca-certificates e2fsprogs iproute2 nftables util-linux \
    && update-ca-certificates && rm -rf /var/lib/apt/lists/*
