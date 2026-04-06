# =============================================================================
# DEV — local development
# =============================================================================

.PHONY: dev up down _clean env _prepare_local_zombied_binary

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

up: _prepare_local_zombied_binary ## Start all services and tail app logs
	@echo "Starting UseZombie..."
	@TARGETARCH=$(LOCAL_DOCKER_ARCH) docker compose up -d --build
	@echo ""
	@echo "Services:"
	@echo "  API:       http://localhost:3000"
	@echo "  Postgres:  localhost:5432"
	@echo ""
	@if [ "$${FOLLOW_LOGS:-1}" = "1" ]; then \
		TARGETARCH=$(LOCAL_DOCKER_ARCH) docker compose logs -f zombied; \
	fi

dev: up  ## Alias for 'make up'

down:  ## Stop all services, remove volumes, and cleanup
	@echo "Stopping all services..."
	@docker compose down --volumes
	@$(MAKE) _clean --no-print-directory
	@echo "Cleanup complete."

env:  ## Generate .env from Proton Pass vault (ENV=local|dev|prod)
	@pass-cli inject -i .env.$(or $(ENV),local).tpl -o .env -f
	@chmod 600 .env
	@echo "✔ Generated .env from .env.$(or $(ENV),local).tpl"

_prepare_local_zombied_binary:
	@mkdir -p dist
	@echo "Preparing local zombied binary for linux/$(LOCAL_DOCKER_ARCH) ($(LOCAL_ZIG_TARGET))..."
	@zig build -Doptimize=ReleaseSafe -Dtarget=$(LOCAL_ZIG_TARGET)
	@cp zig-out/bin/zombied dist/zombied-linux-$(LOCAL_DOCKER_ARCH)
	@chmod +x dist/zombied-linux-$(LOCAL_DOCKER_ARCH)

_clean:
	@rm -rf zig-out zig-cache .zig-cache
