# =============================================================================
# BUILD & REGISTRY — container builds and pushes
# =============================================================================

.PHONY: build build-dev push-dev push _prepare_prebuilt_linux_binaries

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
	zig build -Doptimize=ReleaseSafe -Dtarget=x86_64-linux-gnu
	cp zig-out/bin/zombied dist/zombied-linux-amd64
	chmod +x dist/zombied-linux-amd64
	zig build -Doptimize=ReleaseSafe -Dtarget=aarch64-linux-gnu
	cp zig-out/bin/zombied dist/zombied-linux-arm64
	chmod +x dist/zombied-linux-arm64

build: _prepare_prebuilt_linux_binaries ## Build production container (always uses prebuilt linux binaries)
	$(call _buildx,Dockerfile,$(_PROD_TAGS),)

build-dev:  ## Build development container (multi-arch)
	$(call _buildx,Dockerfile.dev,$(_DEV_TAGS),)

push: _docker_login _prepare_prebuilt_linux_binaries ## Push production image (always uses prebuilt linux binaries)
	$(call _buildx,Dockerfile,$(_PROD_TAGS),--push)

push-dev: _docker_login  ## Push development image to registry (uses prebuilt linux binaries)
	$(call _buildx,Dockerfile,$(_DEV_TAGS),--push)

_docker_login:
	@if [ -n "$(GITHUB_TOKEN)" ]; then \
		echo "$(GITHUB_TOKEN)" | docker login ghcr.io -u "$(GITHUB_ACTOR)" --password-stdin; \
	elif [ -n "$(DOCKER_USER)" ] && [ -n "$(DOCKER_PASS)" ]; then \
		echo "$(DOCKER_PASS)" | docker login $(DOCKER_REGISTRY) -u "$(DOCKER_USER)" --password-stdin; \
	else \
		echo "Error: No credentials. Set GITHUB_TOKEN or DOCKER_USER/DOCKER_PASS." >&2; exit 1; \
	fi
