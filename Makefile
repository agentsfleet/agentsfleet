# =============================================================================
# USEZOMBIE MAKEFILE - MODULAR STRUCTURE
# =============================================================================

include make/dev.mk
include make/quality.mk
include make/test.mk
include make/build.mk
.DEFAULT_GOAL := help

help:  ## Show all available Makefile targets
	@echo "usezombie"
	@echo ""
	@echo "Development:"
	@echo "  up                 Start all services and tail app logs"
	@echo "  down               Stop all services, remove volumes, and cleanup"
	@echo "  dev                Alias for 'make up'"
	@echo ""
	@echo "Quality & Testing:"
	@echo "  lint                  Run code quality checks (zombied Zig fmt check + ZLint + website + app + zombiectl + actionlint)"
	@echo "  test-unit             Run unit lanes: zombied (Zig unit only) + zombiectl + website + app"
	@echo "  test-integration      Run full real integration suite once (DB + Redis, CI canonical gate)"
	@echo "  test-integration-db   Run DB-backed integration suite only (local debugging helper)"
	@echo "  test-integration-redis Run Redis-backed integration suite only (local debugging helper)"
	@echo "  test                  Run test-unit + backend/API e2e"
	@echo "  memleak               Run Zig memory leak gate (platform-aware)"
	@echo "  bench                 Run API benchmark (set BENCH_MODE=bench|soak|profile)"
	@echo "  qa                    Run full QA lanes (backend e2e + website + app Playwright)"
	@echo "  qa-smoke              Run smoke QA lanes (backend e2e + website smoke + app smoke)"
	@echo "  doctor                Run zombied doctor (connectivity + config check)"
	@echo ""
	@echo "Build & Deploy:"
	@echo "  build                 Build production container (uses prebuilt linux binaries)"
	@echo "  build-dev          Build development container"
	@echo "  push-dev           Push development image to registry"
	@echo "  push                  Push production image (uses prebuilt linux binaries)"
	@echo "  sync-version          Propagate VERSION → build.zig.zon + zombiectl/package.json"
	@echo "  check-version         Verify all version files match VERSION (CI drift gate)"
