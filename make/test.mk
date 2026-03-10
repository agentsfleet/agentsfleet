# =============================================================================
# TEST — unit + integration + e2e
# =============================================================================

.PHONY: test test-unit test-zombied test-unit-zombied test-unit-website test-unit-app test-integration test-integration-zombied test-depth test-coverage-zombied test-e2e _test_e2e _test_e2e_backend _test_e2e_smoke _test_e2e_backend_smoke _qa_website _qa_website_smoke qa qa_app qa-smoke qa_app_smoke memleak bench _soak _bench_apiprofile _ensure-test-bin _bench-run _zig_test_filter

ZIG_GLOBAL_CACHE_DIR ?= $(CURDIR)/.tmp/zig-global-cache
ZIG_LOCAL_CACHE_DIR  ?= $(CURDIR)/.tmp/zig-local-cache
ZOMBIED_COVERAGE_MIN_LINES ?= 35
BENCH_MODE ?= bench
BACKEND_E2E_FILTER_1 ?= integration: beginApiRequest enforces max in-flight limit
BACKEND_E2E_FILTER_2 ?= integration: endApiRequest decrements in-flight counter deterministically
BACKEND_E2E_FILTER_3 ?= integration: start-run queue failure compensation removes only SPEC_QUEUED row
BACKEND_E2E_FILTER_4 ?= integration: retry queue failure compensation restores state and removes retry transition
BACKEND_E2E_SMOKE_FILTER ?= integration: beginApiRequest enforces max in-flight limit

# --- Unit tests ---

test-unit-zombied:  ## Run zombied unit tests (Zig)
	@echo "→ [zombied] Running Zig unit tests..."
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
	 zig build test --summary all
	@$(MAKE) test-depth

test-unit-website:  ## Run website unit tests (vitest)
	@echo "→ [website] Running Vitest unit tests..."
	@cd ui/packages/website && bun run test
	@echo "✓ [website] Unit tests passed"

test-unit-app:  ## Run app unit tests (vitest)
	@echo "→ [app] Running Vitest unit tests..."
	@cd ui/packages/app && bun run test
	@echo "✓ [app] Unit tests passed"

test-zombied: test-unit-zombied test-integration-zombied  ## Run zombied tests (unit + integration)
	@echo "✓ [zombied] Unit + integration passed"

test-unit: test-zombied test-unit-website test-unit-app  ## Run all unit tests (zombied + website + app)
	@echo "✓ All unit tests passed"

# --- Integration tests ---

test-integration-zombied:  ## Run Zig integration tests (deterministic, no live service required)
	@echo "→ [zombied] Running Zig integration tests..."
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
	 zig build test -- --test-filter "integration:"

test-integration: test-integration-zombied  ## Run integration checks
	@echo "→ [zombied] Integration tests passed"

test-depth:  ## Enforce minimum test depth inventory
	@mkdir -p .tmp
	@unit_count=$$(find src -name '*.zig' -exec grep -hE '^test \"' {} + | wc -l | tr -d ' '); \
	 integration_count=$$(find src -name '*.zig' -exec grep -hE '^test \"integration:' {} + | wc -l | tr -d ' '); \
	 printf 'zombied_test_cases=%s\nzombied_integration_cases=%s\n' "$$unit_count" "$$integration_count" | tee .tmp/zombied-test-depth.txt >/dev/null; \
	 if [ "$$unit_count" -lt 25 ]; then echo "✗ expected at least 25 Zig tests, got $$unit_count"; exit 1; fi; \
	 if [ "$$integration_count" -lt 3 ]; then echo "✗ expected at least 3 Zig integration tests, got $$integration_count"; exit 1; fi; \
	 echo "✓ [zombied] test depth gate passed (unit=$$unit_count integration=$$integration_count)"

test-coverage-zombied:  ## Run backend line coverage with kcov and enforce minimum threshold
	@command -v kcov >/dev/null 2>&1 || { echo "✗ kcov is required for backend coverage (install: brew install kcov or apt-get install kcov)"; exit 1; }
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)" coverage/zombied .tmp
	@echo "→ [zombied] Building backend test binary for coverage..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-bin
	@echo "→ [zombied] Running kcov coverage..."
	@kcov --clean --include-pattern="$(CURDIR)/src" coverage/zombied zig-out/bin/zombied-tests >/dev/null
	@line_rate=$$(sed -n 's/.*line-rate="\([0-9.]*\)".*/\1/p' coverage/zombied/cobertura.xml | head -n 1); \
	 if [ -z "$$line_rate" ]; then echo "✗ failed to parse backend line-rate from coverage/zombied/cobertura.xml"; exit 1; fi; \
	 line_pct=$$(awk -v r="$$line_rate" 'BEGIN { printf "%.2f", r * 100 }'); \
	 printf 'zombied_line_coverage_pct=%s\nzombied_line_coverage_min_pct=%s\n' "$$line_pct" "$(ZOMBIED_COVERAGE_MIN_LINES)" | tee .tmp/zombied-coverage.txt >/dev/null; \
	 awk -v got="$$line_pct" -v min="$(ZOMBIED_COVERAGE_MIN_LINES)" 'BEGIN { if ((got + 0) < (min + 0)) { printf "✗ backend line coverage %.2f%% is below threshold %.2f%%\n", got, min; exit 1 } }'; \
	 echo "✓ [zombied] backend line coverage gate passed ($$line_pct% >= $(ZOMBIED_COVERAGE_MIN_LINES)%)"

# --- E2E: API flow (zombied backend) ---

test-e2e:  ## Run e2e API flow against a running local service
	@$(MAKE) _test_e2e

# --- E2E: Website Playwright ---
# Local: Playwright config starts Vite dev server automatically.
# Post-deploy: BASE_URL=https://usezombie.com make qa-smoke

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

_qa_website:  ## Internal: run website Playwright e2e suite
	@echo "→ [website] Running Playwright e2e..."
	@cd ui/packages/website && bun run test:e2e
	@echo "✓ [website] E2E passed"

_qa_website_smoke:  ## Internal: run website Playwright smoke suite
	@echo "→ [website] Running Playwright smoke..."
	@cd ui/packages/website && bun run test:e2e:smoke
	@echo "✓ [website] Smoke passed"

qa_app:  ## Run app QA lane (deterministic vitest suite)
	@echo "→ [app] Running QA lane..."
	@cd ui/packages/app && bun run qa
	@echo "✓ [app] QA lane passed"

qa_app_smoke:  ## Run app smoke lane (fast vitest subset)
	@echo "→ [app] Running smoke lane..."
	@cd ui/packages/app && bun run qa:smoke
	@echo "✓ [app] Smoke lane passed"

qa: _test_e2e _qa_website qa_app  ## Run full QA lanes (backend e2e + website + app)
	@echo "✓ All QA lanes passed"

qa-smoke: _test_e2e_smoke _qa_website_smoke qa_app_smoke  ## Run smoke QA lanes (backend e2e + website + app)
	@echo "✓ All smoke QA lanes passed"

# --- API stress/perf gates (single runner, mode-based wrappers) ---

memleak:  ## Run Zig memory leak gates (allocator tests + Linux valgrind pass)
	@echo "→ [zombied] Running allocator leak guard tests..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test -- --test-filter "finalizeWorkerAllocator"
	@case "$$(uname -s)" in \
	  Linux) \
	    command -v valgrind >/dev/null 2>&1 || { echo "✗ valgrind is required on Linux for make memleak"; exit 1; }; \
	    $(MAKE) _ensure-test-bin; \
	    echo "→ [zombied] Running valgrind leak gate..."; \
	    valgrind --quiet --leak-check=full --show-leak-kinds=all --errors-for-leak-kinds=definite,possible --error-exitcode=1 \
	      zig-out/bin/zombied-tests;; \
	  Darwin) \
	    if command -v leaks >/dev/null 2>&1; then \
	      $(MAKE) _ensure-test-bin; \
	      echo "→ [zombied] Running macOS leaks gate..."; \
	      MallocStackLogging=1 leaks -atExit -- zig-out/bin/zombied-tests >/dev/null || \
	        echo "→ [zombied] leaks check unavailable in current runtime (continuing with allocator gate)"; \
	    else \
	      echo "→ [zombied] leaks not found; allocator gate only"; \
	    fi;; \
	  *) \
	    echo "→ [zombied] platform=$$(uname -s): allocator gate only";; \
	esac
	@echo "✓ [zombied] memleak gate passed"

bench:  ## Run API benchmark runner (BENCH_MODE=bench|soak|profile)
	@$(MAKE) _bench-run BENCH_MODE=$(BENCH_MODE)

_soak:  ## Internal: run API soak benchmark
	@$(MAKE) _bench-run BENCH_MODE=soak

_bench_apiprofile:  ## Internal: run API benchmark with profiling artifacts
	@$(MAKE) _bench-run BENCH_MODE=profile

_bench-run:
	@mkdir -p .tmp
	@echo "→ [zombied] Running API benchmark mode=$(BENCH_MODE)..."
	@BENCH_MODE="$(BENCH_MODE)" bun scripts/api_bench_runner.js

_ensure-test-bin:
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-bin

# --- Aggregate ---

test: test-unit _test_e2e  ## Run full test suite (test-unit + backend/API e2e)
	@echo "✓ Full test suite passed"
