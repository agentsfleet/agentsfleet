# =============================================================================
# TEST-UNIT — agentsfleetd, agentsfleet, website, app + multi-package coverage gate
# =============================================================================

.PHONY: test-unit-agentsfleetd test-unit-agentsfleet-runner test-unit-agentsfleet-lib test-unit-cli test-unit-website test-unit-app test-unit-design-system test-coverage-all _test-coverage-agentsfleetd

test-unit-agentsfleetd:  ## Run agentsfleetd unit tests (Zig)
	@echo "→ [agentsfleetd] Running Zig unit tests..."
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
	@$(MAKE) _lint_zig_test_depth

test-unit-agentsfleet-runner:  ## Run agentsfleet-runner unit tests (Zig; own build graph, no datastore)
	@echo "→ [agentsfleet-runner] Running Zig unit tests via build_runner.zig (contract + daemon + common)..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig test --summary all
	@echo "✓ [agentsfleet-runner] Unit tests passed (independent of agentsfleetd/src)"

test-unit-agentsfleet-lib:  ## Run shared src/lib module unit tests (Zig; named modules, no datastore)
	@echo "→ [lib] Running shared src/lib module unit tests..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-lib --summary all
	@echo "✓ [lib] Shared src/lib unit tests passed (consumed by agentsfleetd + agentsfleet-runner)"

test-unit-cli:  ## Run agentsfleet CLI unit tests (bun)
	@echo "→ [agentsfleet] Building dist/ (tests spawn dist/bin/agentsfleet.js)..."
	@cd cli && bun run build >/dev/null
	@echo "→ [agentsfleet] Running Bun unit tests..."
	@# --timeout 30000: the help-e2e / PTY tests spawn the built binary and wait
	@# for output; bun's 5s default flakes under the parallel pre-push lane load.
	@cd cli && bun test --timeout 30000
	@echo "✓ [agentsfleet] Unit tests passed"

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

test-coverage-all:  ## Run coverage gates across app + website + agentsfleet + design-system
	@echo "→ [app] Running Vitest with --coverage..."
	@cd ui/packages/app && bun run test:coverage
	@echo "→ [website] Running Vitest with --coverage..."
	@cd ui/packages/website && bun run test:coverage
	@echo "→ [agentsfleet] Running Bun test --coverage..."
	@cd cli && bun run test:coverage
	@echo "→ [design-system] Running Vitest with --coverage..."
	@cd ui/packages/design-system && bun run test:coverage
	@echo "✓ All package coverage gates passed"

_test-coverage-agentsfleetd:
	@command -v kcov >/dev/null 2>&1 || { echo "✗ kcov is required for backend coverage (install: brew install kcov or apt-get install kcov)"; exit 1; }
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)" coverage/agentsfleetd .tmp
	@echo "→ [agentsfleetd] Building backend test binary for coverage..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-bin
	@echo "→ [agentsfleetd] Running kcov coverage..."
	@kcov --clean --include-pattern="$(CURDIR)/src" coverage/agentsfleetd zig-out/bin/agentsfleetd-tests >/dev/null
	@line_rate=$$(sed -n 's/.*line-rate="\([0-9.]*\)".*/\1/p' coverage/agentsfleetd/cobertura.xml | head -n 1); \
	 if [ -z "$$line_rate" ]; then echo "✗ failed to parse backend line-rate from coverage/agentsfleetd/cobertura.xml"; exit 1; fi; \
	 line_pct=$$(awk -v r="$$line_rate" 'BEGIN { printf "%.2f", r * 100 }'); \
	 printf 'agentsfleetd_line_coverage_pct=%s\nagentsfleetd_line_coverage_min_pct=%s\n' "$$line_pct" "$(AGENTD_COVERAGE_MIN_LINES)" | tee .tmp/agentsfleetd-coverage.txt >/dev/null; \
	 awk -v got="$$line_pct" -v min="$(AGENTD_COVERAGE_MIN_LINES)" 'BEGIN { if ((got + 0) < (min + 0)) { printf "✗ backend line coverage %.2f%% is below threshold %.2f%%\n", got, min; exit 1 } }'; \
	 echo "✓ [agentsfleetd] backend line coverage gate passed ($$line_pct% >= $(AGENTD_COVERAGE_MIN_LINES)%)"
