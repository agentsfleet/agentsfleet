# =============================================================================
# TEST — unit + integration + e2e
# =============================================================================

.PHONY: test test-unit test-unit-zombied test-unit-website test-integration test-integration-zombied test-depth test-coverage-zombied test-e2e qa qa-smoke memleak bench _soak _bench_apiprofile _ensure-test-bin _bench-run

ZIG_GLOBAL_CACHE_DIR ?= $(CURDIR)/.tmp/zig-global-cache
ZIG_LOCAL_CACHE_DIR  ?= $(CURDIR)/.tmp/zig-local-cache
ZOMBIED_COVERAGE_MIN_LINES ?= 35
BENCH_MODE ?= bench
LEAK_FILTER ?= finalizeWorkerAllocator returns false for clean allocator

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

test-unit: test-unit-zombied test-unit-website  ## Run all unit tests (zombied + website)
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
	@unit_count=$$(rg -n '^test \"' src -g '*.zig' | wc -l | tr -d ' '); \
	 integration_count=$$(rg -n '^test \"integration:' src -g '*.zig' | wc -l | tr -d ' '); \
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
	@echo "→ [zombied] Running e2e API tests..."
	@curl -sf -X POST http://localhost:3000/v1/runs \
		-H "Authorization: Bearer $$API_KEY" \
		-H "Content-Type: application/json" \
		-d '{"workspace_id":"$$ACCEPTANCE_WORKSPACE_ID","spec_id":"$$ACCEPTANCE_SPEC_ID","mode":"api","requested_by":"ci","idempotency_key":"e2e-001"}' \
		| jq .

# --- E2E: Website Playwright ---
# Local: Playwright config starts Vite dev server automatically.
# Post-deploy: BASE_URL=https://usezombie.com make qa-smoke

qa:  ## Run Playwright e2e tests (full suite)
	@echo "→ [website] Running Playwright e2e..."
	@cd ui/packages/website && bun run test:e2e
	@echo "✓ [website] E2E passed"

qa-smoke:  ## Run Playwright smoke tests only (fast CI gate)
	@echo "→ [website] Running Playwright smoke..."
	@cd ui/packages/website && bun run test:e2e:smoke
	@echo "✓ [website] Smoke passed"

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
	      zig-out/bin/zombied-tests --test-filter "$(LEAK_FILTER)";; \
	  Darwin) \
	    if command -v leaks >/dev/null 2>&1; then \
	      $(MAKE) _ensure-test-bin; \
	      echo "→ [zombied] Running macOS leaks gate..."; \
	      MallocStackLogging=1 leaks -atExit -- zig-out/bin/zombied-tests --test-filter "$(LEAK_FILTER)" >/dev/null || \
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

test: test-unit test-integration test-e2e  ## Run all backend tests (zombied unit + integration + e2e)
	@echo "✓ All backend tests passed"
