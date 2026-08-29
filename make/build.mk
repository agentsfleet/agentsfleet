# =============================================================================
# BUILD & REGISTRY — container builds and pushes
# =============================================================================

.PHONY: build build-dev push-dev push build-linux-alpine _docker_login _prepare_prebuilt_linux_binaries sync-version check-version

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

# Both architectures, built the way the release builds them: inside a
# musl-native Alpine container, so the musl target is the host target and no
# cross linker has to be configured on a developer's machine.
_prepare_prebuilt_linux_binaries:
	mkdir -p dist
	@for arch in amd64:x86_64 arm64:aarch64; do \
	  out="$${arch%%:*}"; platform="$${arch##*:}"; \
	  echo "→ [image] building the daemon for linux/$$out..."; \
	  docker run --rm --platform "linux/$$out" \
	    -v "$(CURDIR):/w" -w /w/rustd rust:1.98-alpine \
	    sh -c 'apk add --no-cache build-base perl cmake go linux-headers >/dev/null && cargo build --profile dist --bin agentsfleetd' \
	    || exit 1; \
	  cp rustd/target/dist/agentsfleetd "dist/agentsfleetd-rs-linux-$$out"; \
	  chmod +x "dist/agentsfleetd-rs-linux-$$out"; \
	  rm -rf rustd/target/dist; \
	  echo "✓ [image] dist/agentsfleetd-rs-linux-$$out ($$platform)"; \
	done

build: _prepare_prebuilt_linux_binaries ## Build production container (uses prebuilt linux binaries)
	$(call _buildx,Dockerfile,$(_PROD_TAGS),)

# One Dockerfile, two tag sets. `Dockerfile.dev` has not existed for some time,
# so this target could only ever fail — while `push-dev` beneath it built the
# same image from `Dockerfile` and worked. Development and production differ in
# what they are TAGGED and where they deploy, never in what is in the image;
# a second Dockerfile would be a second thing to keep true.
build-dev: _prepare_prebuilt_linux_binaries  ## Build development container (multi-arch)
	$(call _buildx,Dockerfile,$(_DEV_TAGS),)

build-linux-alpine:  ## Compile inside Alpine with musl-native OpenSSL; asserts zero NEEDED + no INTERP (mirrors CI)
	@echo "→ Building aarch64-linux inside Alpine (native ARM, static OpenSSL)..."
	@docker run --rm --platform linux/arm64 \
		-v "$(CURDIR):/src:ro" -w /tmp/build \
		mirror.gcr.io/library/alpine:3.21 \
		sh -c '\
			apk add --no-cache openssl-dev openssl-libs-static ca-certificates xz wget binutils >/dev/null 2>&1 && \
			ARCH=$$(uname -m); \
			mkdir -p /usr/lib/$${ARCH}-linux-gnu /usr/include/$${ARCH}-linux-gnu && \
			ln -sf /usr/lib/libssl.a /usr/lib/$${ARCH}-linux-gnu/libssl.a && \
			ln -sf /usr/lib/libcrypto.a /usr/lib/$${ARCH}-linux-gnu/libcrypto.a && \
			ln -sf /usr/include/openssl /usr/include/$${ARCH}-linux-gnu/openssl && \
			cp -a /src/. . && \
			case $$ARCH in x86_64) ZIG_ARCH=x86_64;; aarch64) ZIG_ARCH=aarch64;; *) echo "unsupported arch $$ARCH"; exit 1;; esac; \
			ZIG_URL="https://ziglang.org/download/0.15.2/zig-$$ZIG_ARCH-linux-0.15.2.tar.xz"; \
			echo "  fetching zig 0.15.2 for $$ZIG_ARCH..." && \
			(cd /tmp && wget -q "$$ZIG_URL" -O zig.tar.xz && tar xf zig.tar.xz && cp zig-*/zig /usr/local/bin/ && cp -r zig-*/lib /usr/local/lib/zig) && \
			echo "  compiling agentsfleetd (aarch64-linux, static OpenSSL)..." && \
			zig build -Doptimize=ReleaseSafe -Dtarget=aarch64-linux && \
			for bin in zig-out/bin/agentsfleetd; do \
				test -f "$$bin" || { echo "FAIL: $$bin not found"; exit 1; }; \
				if readelf -d "$$bin" 2>/dev/null | grep -q " (NEEDED)"; then \
					echo "FAIL: $$bin has dynamic NEEDED entries"; \
					readelf -d "$$bin" | grep NEEDED; \
					exit 1; \
				fi; \
				if readelf -l "$$bin" 2>/dev/null | grep -q "INTERP"; then \
					echo "FAIL: $$bin has INTERP (dynamic linker) section"; \
					exit 1; \
				fi; \
				echo "✓ $$bin: fully static (zero NEEDED, no INTERP)"; \
			done'

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
