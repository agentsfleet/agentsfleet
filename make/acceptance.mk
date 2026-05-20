# =============================================================================
# ACCEPTANCE — dry page lanes + live-e2e backend spec + auth portability gate
# =============================================================================

.PHONY: live-e2e-all live-e2e-auth _e2e _e2e_backend _e2e_smoke _e2e_backend_smoke _dry_website _dry_website_smoke dry dry-app dry-smoke dry-app-smoke

# Smoke backend filter — substring handed to `zig build -Dtest-filter` via
# _test-integration-full's TEST_FILTER. Must match real `test "integration: …"`
# declarations in src/; the readiness tests exercise both the DB-unhealthy and
# Redis-degraded paths, so they make a fast, real backend smoke.
BACKEND_E2E_SMOKE_FILTER ?= integration: ready decision

_e2e_backend: _test-integration-full
	@echo "✓ [zombied] live-e2e-all backend lane passed (full integration suite, no filter)"

_e2e_backend_smoke:
	@echo "→ [zombied] Running live-e2e backend smoke lane (readiness subset)..."
	@TEST_FILTER="$(BACKEND_E2E_SMOKE_FILTER)" $(MAKE) _test-integration-full
	@echo "✓ [zombied] live-e2e backend smoke lane passed"

_e2e: _e2e_backend
	@echo "✓ [zombied] _e2e passed"

_e2e_smoke: _e2e_backend_smoke
	@echo "✓ [zombied] _e2e_smoke passed"

live-e2e-all:  ## Run the full Zig integration suite unfiltered vs real Postgres + Redis (no -Dtest-filter)
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
