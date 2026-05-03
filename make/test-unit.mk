# =============================================================================
# TEST-UNIT — zombied, zombiectl, website, app
# =============================================================================

.PHONY: test-zombied test-unit-zombied _test-unit-zombied-executor test-unit-zombiectl test-unit-website test-unit-app test-coverage-app test-depth test-coverage-zombied test-auth

test-unit-zombied:  ## Run zombied unit tests (Zig)
	@echo "→ [zombied] Running Zig unit tests..."
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
	 zig build test --summary all
	@$(MAKE) _test-unit-zombied-executor
	@$(MAKE) test-depth

test-auth:  ## Portability gate — compile + run src/auth/** in isolation (M18_002 §1.3)
	@echo "→ [zombied] Running src/auth/ portability gate..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-auth --summary all
	@echo "✓ [zombied] src/auth/ compiles + tests pass in isolation (portability contract holds)"

_test-unit-zombied-executor:  ## Run zombied-executor sidecar unit tests (Zig)
	@echo "→ [zombied-executor] Running executor sidecar tests..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-executor --summary all 2>&1 | tee /dev/stderr | grep -q "passed" \
	   && echo "✓ [zombied-executor] Executor tests passed" \
	   || { echo "✗ [zombied-executor] Executor tests failed"; exit 1; }

test-unit-zombiectl:  ## Run zombiectl CLI unit tests (bun)
	@echo "→ [zombiectl] Running Bun unit tests..."
	@cd zombiectl && bun test
	@echo "✓ [zombiectl] Unit tests passed"

test-unit-website:  ## Run website unit tests (vitest)
	@echo "→ [website] Running Vitest unit tests..."
	@cd ui/packages/website && bun run test
	@echo "✓ [website] Unit tests passed"

test-unit-app:  ## Run app unit tests (vitest)
	@echo "→ [app] Running Vitest unit tests..."
	@cd ui/packages/app && bun run test
	@echo "✓ [app] Unit tests passed"

test-skill-evals:  ## Run agent-skill evals (node --test, deterministic subset)
	@echo "→ [skill-evals] Running install-skill substitution + invariant suite..."
	@node --test --test-reporter=spec $$(find tests/skill-evals -name '*.test.js' | sort)
	@echo "✓ [skill-evals] All skill evals passed"

test-coverage-app:  ## Run app unit tests with v8 coverage and enforce thresholds (vitest.config.ts)
	@echo "→ [app] Running Vitest with --coverage..."
	@cd ui/packages/app && bun run test:coverage
	@echo "✓ [app] Coverage gate passed (statements ≥95, branches ≥90, functions ≥95, lines ≥95)"

test-zombied: test-unit-zombied _test-integration-zombied  ## Run zombied tests (unit + integration)
	@echo "✓ [zombied] Unit + integration passed"


test-depth:  ## Enforce minimum test depth inventory
	@mkdir -p .tmp
	@unit_count=$$(find src -name '*.zig' -exec grep -hE '^test "' {} + | wc -l | tr -d ' '); \
	 integration_count=$$(find src -name '*.zig' -exec grep -hE '^test "integration:' {} + | wc -l | tr -d ' '); \
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
