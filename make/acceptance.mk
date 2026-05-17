# =============================================================================
# ACCEPTANCE — dry page lanes + live-e2e backend spec + auth portability gate
# =============================================================================

.PHONY: live-e2e-all live-e2e-auth _e2e _e2e_backend _e2e_smoke _e2e_backend_smoke _dry_website _dry_website_smoke dry dry-app dry-smoke dry-app-smoke _zig_test_filter

BACKEND_E2E_FILTER_1 ?= integration: beginApiRequest enforces max in-flight limit
BACKEND_E2E_FILTER_2 ?= integration: endApiRequest decrements in-flight counter deterministically
BACKEND_E2E_FILTER_3 ?= integration: start-run queue failure compensation removes only SPEC_QUEUED row
BACKEND_E2E_FILTER_4 ?= integration: retry queue failure compensation restores state and removes retry transition
BACKEND_E2E_SMOKE_FILTER ?= integration: beginApiRequest enforces max in-flight limit

_zig_test_filter:
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@redis_tls_test_url="$$TEST_REDIS_TLS_URL"; \
	 if [ -z "$$redis_tls_test_url" ] && [ -n "$$REDIS_URL" ]; then \
	   case "$$REDIS_URL" in \
	     rediss://*) redis_tls_test_url="$$REDIS_URL" ;; \
	   esac; \
	 fi; \
	 env -u TEST_REDIS_TLS_URL \
	 ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 $${redis_tls_test_url:+TEST_REDIS_TLS_URL="$$redis_tls_test_url"} \
	 zig build -Dtest-filter="$$TEST_FILTER" test

_e2e_backend:
	@echo "→ [zombied] Running live-e2e-all backend spec lane..."
	@TEST_FILTER="$(BACKEND_E2E_FILTER_1)" $(MAKE) _zig_test_filter
	@TEST_FILTER="$(BACKEND_E2E_FILTER_2)" $(MAKE) _zig_test_filter
	@TEST_FILTER="$(BACKEND_E2E_FILTER_3)" $(MAKE) _zig_test_filter
	@TEST_FILTER="$(BACKEND_E2E_FILTER_4)" $(MAKE) _zig_test_filter
	@echo "✓ [zombied] live-e2e-all backend spec lane passed"

_e2e_backend_smoke:
	@echo "→ [zombied] Running live-e2e backend spec smoke lane..."
	@TEST_FILTER="$(BACKEND_E2E_SMOKE_FILTER)" $(MAKE) _zig_test_filter
	@echo "✓ [zombied] live-e2e backend spec smoke lane passed"

_e2e: _e2e_backend
	@echo "✓ [zombied] _e2e passed"

_e2e_smoke: _e2e_backend_smoke
	@echo "✓ [zombied] _e2e_smoke passed"

live-e2e-all:  ## Run live spec end-to-end backend scenarios (4 filtered Zig integration tests vs real Postgres + Redis)
	@$(MAKE) _e2e

_dry_website:  ## Internal: run website Playwright dry suite (page render, no login)
	@echo "→ [website] Running Playwright dry pass..."
	@cd ui/packages/website && bun run test:e2e
	@echo "✓ [website] Dry pass passed"

_dry_website_smoke:  ## Internal: run website Playwright dry smoke
	@echo "→ [website] Running Playwright dry smoke..."
	@cd ui/packages/website && bun run test:e2e:smoke
	@echo "✓ [website] Dry smoke passed"

dry-app:  ## Run app dry lane — Vitest + Playwright page renders, no Clerk auth
	@echo "→ [app] Running dry lane (no login)..."
	@cd ui/packages/app && bun run qa
	@echo "✓ [app] Dry lane passed"

dry-app-smoke:  ## Run app dry smoke lane — fast Vitest + Playwright smoke, no Clerk auth
	@echo "→ [app] Running dry smoke lane (no login)..."
	@cd ui/packages/app && bun run qa:smoke
	@echo "✓ [app] Dry smoke lane passed"

dry: _e2e _dry_website dry-app  ## Run full dry lanes — backend live-e2e + website Playwright + app Playwright (no Clerk auth)
	@echo "✓ All dry lanes passed"

dry-smoke: _e2e_smoke _dry_website_smoke dry-app-smoke  ## Run smoke dry lanes — fast backend + website + app, no Clerk auth
	@echo "✓ All dry smoke lanes passed"

live-e2e-auth:  ## Portability gate — compile + run src/auth/** in isolation (proves no hidden cross-module deps)
	@echo "→ [zombied] Running src/auth/ portability gate..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-auth --summary all
	@echo "✓ [zombied] src/auth/ compiles + tests pass in isolation (portability contract holds)"
