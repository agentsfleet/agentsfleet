# =============================================================================
# BUILD & REGISTRY — container builds and pushes
# =============================================================================

.PHONY: build build-dev push-dev push build-linux-alpine _prepare_prebuilt_linux_binaries sync-version check-version

VERSION ?= $(shell cat VERSION 2>/dev/null || echo "0.1.0")
GIT_COMMIT := $(if $(GITHUB_SHA),$(shell echo $(GITHUB_SHA) | cut -c1-7),$(shell git rev-parse --short HEAD 2>/dev/null || echo "dev"))
SERVICE_NAME := zombied
DOCKER_REGISTRY ?= ghcr.io
IMAGE_REPO ?= $(DOCKER_REGISTRY)/usezombie/$(SERVICE_NAME)
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

_prepare_prebuilt_linux_binaries:
	mkdir -p dist
	zig build -Doptimize=ReleaseSafe -Dtarget=x86_64-linux
	cp zig-out/bin/zombied dist/zombied-linux-amd64
	cp zig-out/bin/zombied-executor dist/zombied-executor-linux-amd64
	chmod +x dist/zombied-linux-amd64 dist/zombied-executor-linux-amd64
	zig build -Doptimize=ReleaseSafe -Dtarget=aarch64-linux
	cp zig-out/bin/zombied dist/zombied-linux-arm64
	cp zig-out/bin/zombied-executor dist/zombied-executor-linux-arm64
	chmod +x dist/zombied-linux-arm64 dist/zombied-executor-linux-arm64

build: _prepare_prebuilt_linux_binaries ## Build production container (uses prebuilt linux binaries)
	$(call _buildx,Dockerfile,$(_PROD_TAGS),)

build-dev:  ## Build development container (multi-arch)
	$(call _buildx,Dockerfile.dev,$(_DEV_TAGS),)

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
			echo "  compiling zombied (aarch64-linux, static OpenSSL)..." && \
			zig build -Doptimize=ReleaseSafe -Dtarget=aarch64-linux && \
			for bin in zig-out/bin/zombied zig-out/bin/zombied-executor; do \
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

sync-version: ## Propagate VERSION → build.zig.zon, zombiectl/package.json, zombiectl/src/cli.js
	@set -e; \
	V="$$(cat VERSION)"; \
	perl -i -pe 's/\.version = "[^"]+"/.version = "'"$$V"'"/;' build.zig.zon; \
	perl -i -pe 's/"version": "[^"]+"/"version": "'"$$V"'"/;' zombiectl/package.json; \
	perl -i -pe 's/^(export const VERSION = ")[^"]+"/$${1}'"$$V"'";/' zombiectl/src/cli.js; \
	echo "✓ version $$V synced → build.zig.zon, zombiectl/package.json, zombiectl/src/cli.js"

check-version: ## Verify build.zig.zon, zombiectl/package.json, and zombiectl/src/cli.js match VERSION
	@set -e; \
	V="$$(cat VERSION)"; \
	FAIL=0; \
	grep -q "\.version = \"$$V\"" build.zig.zon \
		|| { printf 'DRIFT  build.zig.zon: %s\n' "$$(grep '\.version' build.zig.zon | head -1 | xargs)"; FAIL=1; }; \
	grep -q "\"version\": \"$$V\"" zombiectl/package.json \
		|| { printf 'DRIFT  zombiectl/package.json: %s\n' "$$(grep '"version"' zombiectl/package.json | head -1 | xargs)"; FAIL=1; }; \
	grep -q "^export const VERSION = \"$$V\"" zombiectl/src/cli.js \
		|| { printf 'DRIFT  zombiectl/src/cli.js: %s\n' "$$(grep '^export const VERSION' zombiectl/src/cli.js | head -1 | xargs)"; FAIL=1; }; \
	[ "$$FAIL" = "0" ] && echo "✓ all versions match $$V" || { echo "Run: make sync-version"; exit 1; }

_docker_login:
	@if [ -n "$(GITHUB_TOKEN)" ]; then \
		echo "$(GITHUB_TOKEN)" | docker login ghcr.io -u "$(GITHUB_ACTOR)" --password-stdin; \
	elif [ -n "$(DOCKER_USER)" ] && [ -n "$(DOCKER_PASS)" ]; then \
		echo "$(DOCKER_PASS)" | docker login $(DOCKER_REGISTRY) -u "$(DOCKER_USER)" --password-stdin; \
	else \
		echo "Error: No credentials. Set GITHUB_TOKEN or DOCKER_USER/DOCKER_PASS." >&2; exit 1; \
	fi
