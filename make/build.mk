# =============================================================================
# BUILD & REGISTRY — container builds and pushes
# =============================================================================

.PHONY: build build-dev push-dev push _docker_login dist-daemons image-check sync-version check-version

VERSION ?= $(shell cat VERSION 2>/dev/null || echo "0.1.0")
# The commit, computed once and EXPORTED. It tags the image below, and
# `rustd/crates/afd_api/build.rs` reads it for the commit `/healthz` reports —
# so the tag on an image and the field inside it come from one variable rather
# than from two that can disagree. Before the export, a container build tagged
# `:VERSION-abc1234` and served `"commit":"unknown"`: the build context carries
# no `.git`, and nothing told the build script what `make` already knew.
GIT_COMMIT := $(if $(GITHUB_SHA),$(shell echo $(GITHUB_SHA) | cut -c1-7),$(shell git rev-parse --short HEAD 2>/dev/null || echo "dev"))
export GIT_COMMIT
SERVICE_NAME := agentsfleetd
DOCKER_REGISTRY ?= ghcr.io
IMAGE_REPO ?= $(DOCKER_REGISTRY)/agentsfleet/$(SERVICE_NAME)
_IMAGE := $(IMAGE_REPO)
PLATFORMS ?= linux/amd64,linux/arm64
_DEV_TAGS := --tag $(_IMAGE):$(VERSION)-dev --tag $(_IMAGE):$(VERSION)-dev-$(GIT_COMMIT) --tag $(_IMAGE):dev-latest
_PROD_TAGS := --tag $(_IMAGE):$(VERSION) --tag $(_IMAGE):$(VERSION)-$(GIT_COMMIT) --tag $(_IMAGE):latest

# Internal: shared buildx command
# Usage: $(call _buildx,<dockerfile>,<tags>,<extra-flags>)
define _buildx
	@DOCKER_BUILDKIT=1 docker buildx build \
		. \
		--platform $(PLATFORMS) \
		-f $(1) \
		$(2) \
		$(3)
endef

# The daemon, built the way the release builds it: inside a musl-native Alpine
# container, so the musl target is the host target and no cross linker has to
# be configured on a developer's machine. The static assert runs INSIDE the
# container because that is where readelf is known to exist; a binary with a
# NEEDED entry or an INTERP section fails here, same as in the release job.
#
# DIST_ARCH_PAIRS narrows the build: `image-check` passes only the host's pair,
# because proving "the daemon runs in the image" needs one architecture and the
# other one would arrive through QEMU at emulation speed.
DIST_ARCH_PAIRS ?= amd64:x86_64 arm64:aarch64
LOCAL_TARGETARCH := $(shell uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')

dist-daemons:  ## Build the static daemon for linux (both arches; asserts zero NEEDED + no INTERP)
	mkdir -p dist
	@for arch in $(DIST_ARCH_PAIRS); do \
	  out="$${arch%%:*}"; platform="$${arch##*:}"; \
	  echo "→ [image] building the daemon for linux/$$out..."; \
	  docker run --rm --platform "linux/$$out" \
	    -v "$(CURDIR):/w" -w /w/rustd rust:1.98-alpine \
	    sh -c 'set -e; \
	      apk add --no-cache build-base perl cmake go linux-headers >/dev/null; \
	      cargo build --profile dist --bin agentsfleetd; \
	      bin=target/dist/agentsfleetd; \
	      if readelf -d "$$bin" | grep -q " (NEEDED)"; then echo "FAIL: dynamic NEEDED entries"; exit 1; fi; \
	      if readelf -l "$$bin" | grep -q INTERP; then echo "FAIL: INTERP present"; exit 1; fi; \
	      echo "  static: zero NEEDED, no INTERP"' \
	    || exit 1; \
	  cp rustd/target/dist/agentsfleetd "dist/agentsfleetd-rs-linux-$$out"; \
	  chmod +x "dist/agentsfleetd-rs-linux-$$out"; \
	  rm -rf rustd/target/dist; \
	  echo "✓ [image] dist/agentsfleetd-rs-linux-$$out ($$platform)"; \
	done

image-check: ## Build the production image for the host arch and prove the daemon runs in it (VERIFY: runs beside test-integration-rustd)
	@# Reuse an existing dist binary — the expensive half is the fat-LTO build,
	@# and a VERIFY step that rebuilds it every run would never be run. A stale
	@# binary is refreshed explicitly: `make dist-daemons`.
	@test -f "dist/agentsfleetd-rs-linux-$(LOCAL_TARGETARCH)" \
	  || $(MAKE) dist-daemons DIST_ARCH_PAIRS="$(LOCAL_TARGETARCH):$$(uname -m)"
	docker build --build-arg TARGETARCH=$(LOCAL_TARGETARCH) -t agentsfleetd-image-check .
	@echo "→ [image] the daemon answers inside the image..."
	docker run --rm agentsfleetd-image-check /usr/local/bin/agentsfleetd --version
	@echo "→ [image] and there is no shell to answer with..."
	@if docker run --rm --entrypoint /bin/sh agentsfleetd-image-check -c true >/dev/null 2>&1; then \
	  echo "✗ /bin/sh exists in the image — the base is not distroless"; exit 1; \
	else \
	  echo "✓ no shell in the image"; \
	fi

build: dist-daemons ## Build production container (uses prebuilt linux binaries)
	$(call _buildx,Dockerfile,$(_PROD_TAGS),)

# One Dockerfile, two tag sets. `Dockerfile.dev` has not existed for some time,
# so this target could only ever fail — while `push-dev` beneath it built the
# same image from `Dockerfile` and worked. Development and production differ in
# what they are TAGGED and where they deploy, never in what is in the image;
# a second Dockerfile would be a second thing to keep true.
build-dev: dist-daemons  ## Build development container (multi-arch)
	$(call _buildx,Dockerfile,$(_DEV_TAGS),)

push: _docker_login ## Push production image (expects prebuilt binaries in dist/)
	$(call _buildx,Dockerfile,$(_PROD_TAGS),--push)

push-dev: _docker_login  ## Push development image to registry (uses prebuilt linux binaries)
	$(call _buildx,Dockerfile,$(_DEV_TAGS),--push)

sync-version: ## Propagate VERSION → build.zig.zon + cli/package.json (cli.js reads pkg.version at runtime)
	@set -e; \
	V="$$(cat VERSION)"; \
	perl -i -pe 's/\.version = "[^"]+"/.version = "'"$$V"'"/;' build.zig.zon; \
	perl -i -pe 's/"version": "[^"]+"/"version": "'"$$V"'"/;' cli/package.json; \
	perl -i -pe 's/^version = "[^"]+"/version = "'"$$V"'"/;' rustd/Cargo.toml; \
	echo "✓ version $$V synced → build.zig.zon, cli/package.json, rustd/Cargo.toml (cli.js reads it at runtime)"

check-version: ## Verify build.zig.zon, cli/package.json and rustd/Cargo.toml match VERSION
	@set -e; \
	V="$$(cat VERSION)"; \
	FAIL=0; \
	grep -q "\.version = \"$$V\"" build.zig.zon \
		|| { printf 'DRIFT  build.zig.zon: %s\n' "$$(grep '\.version' build.zig.zon | head -1 | xargs)"; FAIL=1; }; \
	grep -q "\"version\": \"$$V\"" cli/package.json \
		|| { printf 'DRIFT  cli/package.json: %s\n' "$$(grep '"version"' cli/package.json | head -1 | xargs)"; FAIL=1; }; \
	grep -q "^version = \"$$V\"" rustd/Cargo.toml \
		|| { printf 'DRIFT  rustd/Cargo.toml [workspace.package]: %s\n' "$$(grep -m1 '^version =' rustd/Cargo.toml | xargs)"; FAIL=1; }; \
	[ "$$FAIL" = "0" ] && echo "✓ all versions match $$V" || { echo "Run: make sync-version"; exit 1; }

_docker_login:
	@if [ -n "$(GITHUB_TOKEN)" ]; then \
		echo "$(GITHUB_TOKEN)" | docker login ghcr.io -u "$(GITHUB_ACTOR)" --password-stdin; \
	elif [ -n "$(DOCKER_USER)" ] && [ -n "$(DOCKER_PASS)" ]; then \
		echo "$(DOCKER_PASS)" | docker login $(DOCKER_REGISTRY) -u "$(DOCKER_USER)" --password-stdin; \
	else \
		echo "Error: No credentials. Set GITHUB_TOKEN or DOCKER_USER/DOCKER_PASS." >&2; exit 1; \
	fi
