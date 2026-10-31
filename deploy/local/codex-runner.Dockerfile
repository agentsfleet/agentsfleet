# syntax=docker/dockerfile:1
FROM mirror.gcr.io/library/debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4

# A session CA is public trust material; credentials never enter this image.
RUN --mount=type=secret,id=session-ca,required=false \
    set -eu; \
    set --; \
    if [ -f /run/secrets/session-ca ]; then \
      set -- -o Acquire::https::CaInfo=/run/secrets/session-ca; \
    fi; \
    apt-get "$@" update && apt-get "$@" install --yes --no-install-recommends \
      bubblewrap ca-certificates e2fsprogs iproute2 nftables util-linux \
    && update-ca-certificates && rm -rf /var/lib/apt/lists/*
