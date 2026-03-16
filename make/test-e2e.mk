# =============================================================================
# TEST-E2E — backend API e2e, website/app Playwright, QA, smoke
# =============================================================================

.PHONY: test-e2e _test_e2e _test_e2e_backend _test_e2e_smoke _test_e2e_backend_smoke _qa_website _qa_website_smoke qa qa_app qa-smoke qa_app_smoke _zig_test_filter

BACKEND_E2E_FILTER_1 ?= integration: beginApiRequest enforces max in-flight limit
BACKEND_E2E_FILTER_2 ?= integration: endApiRequest decrements in-flight counter deterministically
BACKEND_E2E_FILTER_3 ?= integration: start-run queue failure compensation removes only SPEC_QUEUED row
BACKEND_E2E_FILTER_4 ?= integration: retry queue failure compensation restores state and removes retry transition
BACKEND_E2E_SMOKE_FILTER ?= integration: beginApiRequest enforces max in-flight limit

_zig_test_filter:
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@redis_tls_test_url="$$REDIS_TLS_TEST_URL"; \
	 if [ -z "$$redis_tls_test_url" ] && [ -n "$$REDIS_URL" ]; then \
	   case "$$REDIS_URL" in \
	     rediss://*) redis_tls_test_url="$$REDIS_URL" ;; \
	   esac; \
	 fi; \
	 ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 REDIS_TLS_TEST_URL="$$redis_tls_test_url" \
	 zig build test -- --test-filter "$$TEST_FILTER"

_test_e2e_backend:
	@echo "→ [zombied] Running backend/API e2e integration lane..."
	@TEST_FILTER="$(BACKEND_E2E_FILTER_1)" $(MAKE) _zig_test_filter
	@TEST_FILTER="$(BACKEND_E2E_FILTER_2)" $(MAKE) _zig_test_filter
	@TEST_FILTER="$(BACKEND_E2E_FILTER_3)" $(MAKE) _zig_test_filter
	@TEST_FILTER="$(BACKEND_E2E_FILTER_4)" $(MAKE) _zig_test_filter
	@echo "✓ [zombied] Backend/API e2e lane passed"

_test_e2e_backend_smoke:
	@echo "→ [zombied] Running backend/API e2e smoke lane..."
	@TEST_FILTER="$(BACKEND_E2E_SMOKE_FILTER)" $(MAKE) _zig_test_filter
	@echo "✓ [zombied] Backend/API e2e smoke lane passed"

_test_e2e: _test_e2e_backend
	@echo "✓ [zombied] _test_e2e passed"

_test_e2e_smoke: _test_e2e_backend_smoke
	@echo "✓ [zombied] _test_e2e_smoke passed"

test-e2e:  ## Run e2e API flow against a running local service
	@$(MAKE) _test_e2e

_qa_website:  ## Internal: run website Playwright e2e suite
	@echo "→ [website] Running Playwright e2e..."
	@cd ui/packages/website && bun run test:e2e
	@echo "✓ [website] E2E passed"

_qa_website_smoke:  ## Internal: run website Playwright smoke suite
	@echo "→ [website] Running Playwright smoke..."
	@cd ui/packages/website && bun run test:e2e:smoke
	@echo "✓ [website] Smoke passed"

qa_app:  ## Run app QA lane (vitest + Playwright auth/browser checks)
	@echo "→ [app] Running QA lane..."
	@cd ui/packages/app && bun run qa
	@echo "✓ [app] QA lane passed"

qa_app_smoke:  ## Run app smoke lane (fast vitest + Playwright smoke)
	@echo "→ [app] Running smoke lane..."
	@cd ui/packages/app && bun run qa:smoke
	@echo "✓ [app] Smoke lane passed"

qa: _test_e2e _qa_website qa_app  ## Run full QA lanes (backend e2e + website + app)
	@echo "✓ All QA lanes passed"

qa-smoke: _test_e2e_smoke _qa_website_smoke qa_app_smoke  ## Run smoke QA lanes (backend e2e + website + app)
	@echo "✓ All smoke QA lanes passed"
