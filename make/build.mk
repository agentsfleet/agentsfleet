# =============================================================================
# BUILD & REGISTRY — container builds and pushes
# =============================================================================

.PHONY: build build-dev push-dev push _docker_login _dist-daemons _builder-image sync-version check-version

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
# DIST_ARCH_PAIRS narrows the build — pass the host's pair alone when one
# architecture is enough, since the other arrives through QEMU at emulation
# speed: `make _dist-daemons DIST_ARCH_PAIRS="arm64:aarch64"`.
DIST_ARCH_PAIRS ?= amd64:x86_64 arm64:aarch64

# The builder image, baked once. Docker's own layer cache makes this a no-op
# after the first run, so it is an honest prerequisite rather than a cost.
# Versions live beside the Dockerfile, in the playbook that owns publishing it —
# the same shape `ci_zig_images` uses, so there is one place a base moves.
BUILDER_DIR := playbooks/operations/ci_rust_images
BUILDER_RUST_VERSION := $(shell sed -n 's/^RUST_VERSION=//p' $(BUILDER_DIR)/versions.env)
BUILDER_ALPINE_SERIES := $(shell sed -n 's/^ALPINE_SERIES=//p' $(BUILDER_DIR)/versions.env)
BUILDER_IMAGE ?= ghcr.io/agentsfleet/ci-rust-alpine:$(BUILDER_RUST_VERSION)-alpine$(BUILDER_ALPINE_SERIES)

# Pull the published builder; bake it locally only when the registry does not
# have it. Either way it happens once and every build after reuses the layers.
_builder-image:
	@docker image inspect $(BUILDER_IMAGE) >/dev/null 2>&1 && exit 0; \
	docker pull -q $(BUILDER_IMAGE) >/dev/null 2>&1 && exit 0; \
	echo "→ [image] baking the musl builder locally (once)..."; \
	docker build -q \
	  --build-arg RUST_VERSION=$(BUILDER_RUST_VERSION) \
	  --build-arg ALPINE_SERIES=$(BUILDER_ALPINE_SERIES) \
	  -f $(BUILDER_DIR)/Dockerfile.alpine \
	  -t $(BUILDER_IMAGE) $(BUILDER_DIR) >/dev/null

_dist-daemons: _builder-image
	mkdir -p dist
	@for arch in $(DIST_ARCH_PAIRS); do \
	  out="$${arch%%:*}"; platform="$${arch##*:}"; \
	  echo "→ [image] building the daemon for linux/$$out..."; \
	  docker run --rm --platform "linux/$$out" \
	    -e CARGO_TARGET_DIR="/w/rustd/target/musl-$$out" \
	    -v "$(CURDIR):/w" -w /w/rustd $(BUILDER_IMAGE) \
	    sh -c 'set -e; \
	      cargo build --profile dist --bin agentsfleetd; \
	      bin="$$CARGO_TARGET_DIR/dist/agentsfleetd"; \
	      if readelf -d "$$bin" | grep -q " (NEEDED)"; then echo "FAIL: dynamic NEEDED entries"; exit 1; fi; \
	      if readelf -l "$$bin" | grep -q INTERP; then echo "FAIL: INTERP present"; exit 1; fi; \
	      echo "  static: zero NEEDED, no INTERP"' \
	    || exit 1; \
	  cp "rustd/target/musl-$$out/dist/agentsfleetd" "dist/agentsfleetd-rs-linux-$$out"; \
	  chmod +x "dist/agentsfleetd-rs-linux-$$out"; \
	  echo "✓ [image] dist/agentsfleetd-rs-linux-$$out ($$platform)"; \
	done

build: _dist-daemons ## Build production container (uses prebuilt linux binaries)
	$(call _buildx,Dockerfile,$(_PROD_TAGS),)

# One Dockerfile, two tag sets. `Dockerfile.dev` has not existed for some time,
# so this target could only ever fail — while `push-dev` beneath it built the
# same image from `Dockerfile` and worked. Development and production differ in
# what they are TAGGED and where they deploy, never in what is in the image;
# a second Dockerfile would be a second thing to keep true.
build-dev: _dist-daemons  ## Build development container (multi-arch)
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
