# =============================================================================
# TEST-UNIT — agentsfleetd, agentsfleet, website, app + multi-package coverage gate
# =============================================================================

.PHONY: test-unit-agentsfleetd test-unit-agentsfleet-runner test-unit-agentsfleet-lib test-unit-cli test-unit-website test-unit-app test-unit-design-system test-coverage-zig test-coverage-all

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
	@echo "→ [lib] Running incident-response harness unit tests (M157 §6)..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build bench-incident-test --summary all
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

test-coverage-all: test-coverage-zig  ## Run coverage gates across Zig, app, website, agentsfleet, and design-system
	@echo "→ [app] Running Vitest with --coverage..."
	@cd ui/packages/app && bun run test:coverage
	@echo "→ [website] Running Vitest with --coverage..."
	@cd ui/packages/website && bun run test:coverage
	@echo "→ [agentsfleet] Enforcing the 100% coverage floor (scripts/enforce-coverage.mjs)..."
	@cd cli && bun run test
	@echo "→ [design-system] Running Vitest with --coverage..."
	@cd ui/packages/design-system && bun run test:coverage
	@echo "✓ All package coverage gates passed"

test-coverage-zig:  ## Run and gate merged Zig line coverage for daemon, runner, and shared libraries
	@command -v kcov >/dev/null 2>&1 || { echo "✗ kcov is required for Zig coverage (install: brew install kcov or apt-get install kcov)"; exit 1; }
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)" "$(ZIG_COVERAGE_DIR)" .tmp
	@echo "→ [zig] Building component test binaries for coverage..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-bin
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig test-bin
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-lib-bin
	@# The five components are independent kcov runs over already-built binaries
	@# writing to disjoint output directories, so they run concurrently and only
	@# the merge below needs them all. Each records its own exit status to a file
	@# rather than being tracked by process id: the status is what decides the
	@# gate, and a file maps it back to the component name without the shell
	@# gymnastics of pairing a pid list against a name list.
	@set -eu; \
	 components="agentsfleetd:agentsfleetd-tests runner:agentsfleet-runner-tests lib:agentsfleet-lib-tests logging:agentsfleet-logging-tests deadline:agentsfleet-call-deadline-tests"; \
	 inputs=""; names=""; \
	 for component in $$components; do \
	   name=$${component%%:*}; binary=$${component#*:}; output="$(ZIG_COVERAGE_DIR)/$$name"; \
	   echo "→ [zig] kcov component=$$name binary=$$binary"; \
	   mkdir -p "$$output"; \
	   rm -f ".tmp/kcov-$$name.rc"; \
	   ( set +e; kcov --clean --include-pattern="$(CURDIR)/src" "$$output" "zig-out/bin/$$binary" \
	       >".tmp/kcov-$$name.log" 2>&1; echo $$? >".tmp/kcov-$$name.rc" ) & \
	   names="$$names $$name"; \
	   inputs="$$inputs $$output"; \
	 done; \
	 wait; \
	 failed=0; \
	 for name in $$names; do \
	   rc=$$(cat ".tmp/kcov-$$name.rc" 2>/dev/null || echo 1); \
	   case "$$rc" in ''|*[!0-9]*) rc=1;; esac; \
	   if [ "$$rc" -ne 0 ]; then \
	     echo "✗ Zig coverage component $$name exited $$rc"; tail -n 40 ".tmp/kcov-$$name.log"; failed=1; continue; \
	   fi; \
	   report=$$(find "$(ZIG_COVERAGE_DIR)/$$name" -name cobertura.xml -type f -size +0c -print -quit); \
	   test -n "$$report" || { echo "✗ Zig coverage component $$name produced no Cobertura report"; failed=1; }; \
	 done; \
	 [ "$$failed" -eq 0 ] || exit 1; \
	 merged="$(ZIG_COVERAGE_DIR)/merged"; \
	 kcov --merge "$$merged" $$inputs >/dev/null; \
	 merged_report=$$(find "$$merged" -name cobertura.xml -type f -size +0c -print -quit); \
	 test -n "$$merged_report" || { echo "✗ merged Zig coverage produced no Cobertura report"; exit 1; }; \
	 line_rate=$$(sed -n 's/.*line-rate="\([0-9.]*\)".*/\1/p' "$$merged_report" | head -n 1); \
	 if [ -z "$$line_rate" ]; then echo "✗ failed to parse Zig line-rate from $$merged_report"; exit 1; fi; \
	 line_pct=$$(awk -v r="$$line_rate" 'BEGIN { printf "%.2f", r * 100 }'); \
	 printf 'zig_line_coverage_pct=%s\nzig_line_coverage_min_pct=%s\n' "$$line_pct" "$(ZIG_COVERAGE_MIN_LINES)" | tee .tmp/zig-coverage.txt >/dev/null; \
	 awk -v got="$$line_pct" -v min="$(ZIG_COVERAGE_MIN_LINES)" 'BEGIN { if ((got + 0) < (min + 0)) { printf "✗ Zig line coverage %.2f%% is below threshold %.2f%%\n", got, min; exit 1 } }'; \
	 echo "✓ [zig] merged line coverage passed ($$line_pct% >= $(ZIG_COVERAGE_MIN_LINES)%; report=$$merged/index.html)"
