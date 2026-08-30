# =============================================================================
# DEV — local development
# =============================================================================

.PHONY: up down seed-models _clean _ensure-local-daemon

VERSION ?= $(shell cat VERSION 2>/dev/null || echo "0.1.0")
LOCAL_UNAME_M := $(shell uname -m)
ifeq ($(LOCAL_UNAME_M),arm64)
LOCAL_DOCKER_ARCH := arm64
LOCAL_DIST_PLATFORM := aarch64
else ifeq ($(LOCAL_UNAME_M),aarch64)
LOCAL_DOCKER_ARCH := arm64
LOCAL_DIST_PLATFORM := aarch64
else
LOCAL_DOCKER_ARCH := amd64
LOCAL_DIST_PLATFORM := x86_64
endif

# The binary the image COPYs, under the name the Dockerfile reads. That name is
# the whole reason this rule exists: the local path used to `zig build` into
# `dist/agentsfleetd-linux-$(LOCAL_DOCKER_ARCH)` while the Dockerfile — once the
# image started carrying the Rust daemon — read
# `dist/agentsfleetd-rs-linux-$(TARGETARCH)`. Two names for one slot, so
# `make up` built one file and the image build looked for another and failed at
# COPY. It survived locally only while a stale artifact from an earlier
# `make _dist-daemons` happened to be sitting in dist/.
LOCAL_DAEMON_BINARY := dist/agentsfleetd-rs-linux-$(LOCAL_DOCKER_ARCH)

# A real file target with real prerequisites, not a phony that rebuilds blindly:
# `make up` in a loop then costs one `stat` sweep rather than a release
# cross-compile, and a touched crate still rebuilds. The find is measured at
# 28ms against a 675ms make parse.
RUSTD_SOURCES := $(shell find rustd/crates -name '*.rs' 2>/dev/null) rustd/Cargo.toml rustd/Cargo.lock

$(LOCAL_DAEMON_BINARY): $(RUSTD_SOURCES)
	@echo "Preparing the local daemon for linux/$(LOCAL_DOCKER_ARCH)..."
	@$(MAKE) --no-print-directory _dist-daemons \
	  DIST_ARCH_PAIRS=$(LOCAL_DOCKER_ARCH):$(LOCAL_DIST_PLATFORM)

up: $(LOCAL_DAEMON_BINARY) ## Start all services and tail app logs
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

# Where a locally-composed daemon answers, and the container whose resident set
# is its own. Named here because dev.mk is what stands the stack up; the lanes
# that measure it read these rather than each spelling out a port.
LOCAL_DAEMON_URL := http://127.0.0.1:3000
LOCAL_DAEMON_CONTAINER := agentsfleetd-api
LOCAL_DAEMON_READY_TIMEOUT_SEC := 90

# Boot the stack and do not return until it answers. `up` alone is not enough:
# `docker compose up -d` returns when the container is STARTED, and the image
# carries no healthcheck to wait on (it is distroless — see docker-compose.yml),
# so a lane that ran straight after it would probe a socket nothing is listening
# on yet and report the daemon missing. This is the wait that makes
# `make test-parity LOCAL=1` a single command.
_ensure-local-daemon:
	@FOLLOW_LOGS=0 $(MAKE) --no-print-directory up
	@echo "→ [dev] Waiting for $(LOCAL_DAEMON_URL)/healthz (up to $(LOCAL_DAEMON_READY_TIMEOUT_SEC)s)..."
	@deadline=$$(( $$(date +%s) + $(LOCAL_DAEMON_READY_TIMEOUT_SEC) )); \
	until curl --silent --fail --max-time 2 "$(LOCAL_DAEMON_URL)/healthz" >/dev/null 2>&1; do \
	  if [ "$$(date +%s)" -ge "$$deadline" ]; then \
	    echo "✗ [dev] $(LOCAL_DAEMON_URL) never answered."; \
	    echo "  Most likely the daemon refused boot on missing environment."; \
	    echo "  Check:   docker compose logs agentsfleetd"; \
	    echo "  Fix:     provision ~/Projects/agentsfleet/.env (docs/AUTH.md §Where secrets live),"; \
	    echo "           then re-run. AUTH_SESSION_CODE_PEPPER and the OIDC/provider knobs"; \
	    echo "           come from the LOCAL_DEV vault entry; never hand-write them."; \
	    exit 1; \
	  fi; \
	  sleep 1; \
	done
	@echo "✓ [dev] the local daemon is answering at $(LOCAL_DAEMON_URL)"

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

# `zig-cache` / `.zig-cache` are Zig's DEFAULTS. Most targets override them via
# ZIG_LOCAL_CACHE_DIR, but not all did — anything that shelled out without
# passing the variable through landed back in `.zig-cache` (693 MB in one
# worktree). Both are removed, and the configured cache too, which grew unbounded
# (7.4 GB in one worktree, and the local cache is per-worktree by design). Both
# are still removed: a bare `zig build` typed by hand, with none of the lane
# environment set, still lands in the default path.
#
# The guard matters because ZIG_LOCAL_CACHE_DIR is defined in make/test.mk; an
# unset or empty value must not turn this into `rm -rf` against the worktree.
_clean:
	@rm -rf zig-out zig-cache .zig-cache
	@if [ -n "$(strip $(ZIG_LOCAL_CACHE_DIR))" ]; then \
	  rm -rf "$(ZIG_LOCAL_CACHE_DIR)"; \
	  echo "Removed local Zig cache: $(ZIG_LOCAL_CACHE_DIR)"; \
	fi
