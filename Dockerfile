# agentsfleet — runtime container
#
# The binary must be pre-built before docker build. It is static, so this image
# compiles nothing and carries no toolchain:
#   CI:    the binaries jobs produce dist/agentsfleetd-rs-linux-{amd64,arm64}
#   Local: cargo build --profile dist --target x86_64-unknown-linux-musl
#
# WHY DISTROLESS.
#
# The image carries the daemon and what the daemon needs, which is a CA bundle
# and a clock. `static-debian12` is 6MB and provides both. The base it replaces
# was 275MB, measured, because it also carried bubblewrap, git, openssl and
# wget — none of which this program calls. bubblewrap is the RUNNER's sandbox
# and runs on baremetal worker hosts from
# deploy/baremetal/agentsfleet-runner.service; the other three were carried for
# reasons that no longer exist.
#
# The daemon spawns no child process — there is no `Command::new` in the
# workspace — so a shell and a package manager have nothing to be present for.
# An image with neither is an accurate description of what this program needs,
# not a constraint being worked around.
#
# Readiness is Fly's. deploy/fly/agentsfleetd-{dev,prod}/fly.toml declare
# `[checks.readiness]` as an HTTP GET of /readyz every 15s, so a HEALTHCHECK
# here would be a second implementation of that check — and would need a shell
# and an HTTP client in the image purely to run it.
FROM gcr.io/distroless/static-debian12:nonroot
ARG TARGETARCH=amd64

# OCI metadata — drives the GitHub Container Registry package page. Points at
# the user docs (the package README is otherwise unrelated) and links the
# package to the repo.
LABEL org.opencontainers.image.title="agentsfleet agentsfleetd" \
      org.opencontainers.image.description="agentsfleet control-plane daemon (agentsfleetd) that runs your agents. Docs: https://docs.agentsfleet.net" \
      org.opencontainers.image.url="https://docs.agentsfleet.net" \
      org.opencontainers.image.documentation="https://docs.agentsfleet.net" \
      org.opencontainers.image.source="https://github.com/agentsfleet/agentsfleet"

COPY --chmod=0555 dist/agentsfleetd-rs-linux-${TARGETARCH} /usr/local/bin/agentsfleetd

EXPOSE 3000
CMD ["/usr/local/bin/agentsfleetd", "serve"]
