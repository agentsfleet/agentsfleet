# =============================================================================
# TEST-UNIT — zombied, zombiectl, website, app + multi-package coverage gate
# =============================================================================

.PHONY: test-unit-zombied test-unit-zombiectl test-unit-website test-unit-app test-unit-design-system test-unit-skills test-coverage-all _test-coverage-zombied

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
	@$(MAKE) test-unit-executor
	@$(MAKE) _lint_zig_test_depth

test-unit-zombiectl:  ## Run zombiectl CLI unit tests (bun)
	@echo "→ [zombiectl] Building dist/ (tests spawn dist/bin/zombiectl.js)..."
	@cd zombiectl && bun run build >/dev/null
	@echo "→ [zombiectl] Running Bun unit tests..."
	@cd zombiectl && bun test
	@echo "✓ [zombiectl] Unit tests passed"

test-unit-website:  ## Run website unit tests (vitest)
	@echo "→ [website] Running Vitest unit tests..."
	@cd ui/packages/website && bun run test
	@echo "✓ [website] Unit tests passed"

test-unit-app:  ## Run app unit tests (vitest, no coverage)
	@echo "→ [app] Running Vitest unit tests..."
	@cd ui/packages/app && bun run test
	@echo "✓ [app] Unit tests passed"

test-unit-design-system:  ## Run design-system unit tests (vitest, no coverage)
	@echo "→ [design-system] Running Vitest unit tests..."
	@cd ui/packages/design-system && bun run test
	@echo "✓ [design-system] Unit tests passed"

test-unit-skills:  ## Run agent-skill substitution + invariant unit tests (node --test, deterministic subset)
	@echo "→ [skills] Running agent-skill substitution + invariant suite..."
	@node --test --test-reporter=spec $$(find tests/skill-evals -name '*.test.js' | sort)
	@echo "✓ [skills] All agent-skill checks passed"

test-coverage-all:  ## Run coverage gates across app + website + zombiectl + design-system
	@echo "→ [app] Running Vitest with --coverage..."
	@cd ui/packages/app && bun run test:coverage
	@echo "→ [website] Running Vitest with --coverage..."
	@cd ui/packages/website && bun run test:coverage
	@echo "→ [zombiectl] Running Bun test --coverage..."
	@cd zombiectl && bun run test:coverage
	@echo "→ [design-system] Running Vitest with --coverage..."
	@cd ui/packages/design-system && bun run test:coverage
	@echo "✓ All package coverage gates passed"

_test-coverage-zombied:
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
