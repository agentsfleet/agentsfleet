# =============================================================================
# TEST-UNIT — zombied, zombiectl, website, app
# =============================================================================

.PHONY: test-unit test-zombied test-unit-zombied test-unit-zombiectl test-unit-website test-unit-app test-depth test-coverage-zombied

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

test-zombied: test-unit-zombied _test-integration-zombied  ## Run zombied tests (unit + integration)
	@echo "✓ [zombied] Unit + integration passed"

test-unit: test-zombied test-unit-zombiectl test-unit-website test-unit-app  ## Run all unit tests (zombied + zombiectl + website + app)
	@echo "✓ All unit tests passed"

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
