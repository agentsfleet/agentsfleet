# =============================================================================
# DEV — local development
# =============================================================================

.PHONY: up down seed-models _clean _prepare_local_agentsfleetd_binary

VERSION ?= $(shell cat VERSION 2>/dev/null || echo "0.1.0")
LOCAL_UNAME_M := $(shell uname -m)
ifeq ($(LOCAL_UNAME_M),arm64)
LOCAL_DOCKER_ARCH := arm64
LOCAL_ZIG_TARGET := aarch64-linux
else ifeq ($(LOCAL_UNAME_M),aarch64)
LOCAL_DOCKER_ARCH := arm64
LOCAL_ZIG_TARGET := aarch64-linux
else
LOCAL_DOCKER_ARCH := amd64
LOCAL_ZIG_TARGET := x86_64-linux
endif

up: _prepare_local_agentsfleetd_binary ## Start all services and tail app logs
	@echo "Starting agentsfleet..."
	@TARGETARCH=$(LOCAL_DOCKER_ARCH) docker compose up -d --build
	@echo ""
	@echo "Services:"
	@echo "  API:       http://localhost:3000"
	@echo "  Postgres:  localhost:5432"
	@echo ""
	@if [ "$${FOLLOW_LOGS:-1}" = "1" ]; then \
		TARGETARCH=$(LOCAL_DOCKER_ARCH) docker compose logs -f agentsfleetd; \
	fi

down:  ## Stop all services, remove volumes, and cleanup
	@echo "Stopping all services..."
	@docker compose down --volumes
	@$(MAKE) _clean --no-print-directory
	@echo "Cleanup complete."

# One target for both the first fill and the monthly refresh — an empty catalogue
# emits INSERTs, a populated one emits UPSERTs for drift only, so the refresh path
# is exercised from day one instead of rotting as a rarely-run branch.
#
# Emit-and-review by default: rates are billing data, so nothing reaches the
# database until the diff has been read and APPLY=1 passed. Reads DATABASE_URL to
# diff against the live catalogue; unset means fresh-install mode.
seed-models:  ## Seed/refresh core.model_library from the curated allowlist (APPLY=1 to write)
	@node scripts/seed-models.mjs $(if $(APPLY),--apply,)

_prepare_local_agentsfleetd_binary:
	@mkdir -p dist
	@echo "Preparing local agentsfleetd binary for linux/$(LOCAL_DOCKER_ARCH) ($(LOCAL_ZIG_TARGET))..."
	@zig build -Doptimize=ReleaseSafe -Dtarget=$(LOCAL_ZIG_TARGET)
	@cp zig-out/bin/agentsfleetd dist/agentsfleetd-linux-$(LOCAL_DOCKER_ARCH)
	@chmod +x dist/agentsfleetd-linux-$(LOCAL_DOCKER_ARCH)

_clean:
	@rm -rf zig-out zig-cache .zig-cache
