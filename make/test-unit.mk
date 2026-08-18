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
	@echo "→ [lib] Running the R2/z3 wrapper test (own compilation — imports the named z3 module)..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-s3 --summary all
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

# What a failing component's log is grepped for when the gate reports it. The
# Zig test runner puts its verdict on its OWN line — `FAIL (TestExpectedEqual)`
# — while the test's name and the assertion message sit on the line ABOVE, so
# the match is taken with `grep -B 1` or the report names nothing. Every other
# alternative is anchored: an unanchored `panic` matched the *passing* test
# "…instead of @intCast-panicking" and printed it as the failure for a whole
# round of red Continuous Integration (CI).
ZIG_TEST_FAILURE_GREP = (^|\.\.\.)FAIL\b|^error: .* failed:|error return trace|^thread [0-9]+ panic|^panic:
# Dropped before the `-B 1` window is taken: valgrind writes its own commentary
# (`--PID-- …`, `==PID== …`) into the same stream, and a single interleaved
# warning is enough to push the failing test's name out of the window.
ZIG_TEST_LOG_NOISE = ^--[0-9]+--|^==[0-9]+==

ZIG_UNIT_COVERAGE_COMPONENTS = agentsfleetd:agentsfleetd-tests runner:agentsfleet-runner-tests lib:agentsfleet-lib-tests logging:agentsfleet-logging-tests deadline:agentsfleet-call-deadline-tests s3:agentsfleet-s3-tests
ZIG_UNIT_COVERAGE_NAMES = agentsfleetd runner lib logging deadline s3

# Unit ownership ends here. The live daemon and runner-kernel roots belong to
# test-integration, which consumes this provenance-matched component set and
# grades the final union after every integration shard succeeds.
test-coverage-zig:  ## Run every Zig unit owner once and publish reusable coverage components
	@command -v kcov >/dev/null 2>&1 || { echo "✗ kcov is required for Zig coverage (install: brew install kcov or apt-get install kcov)"; exit 1; }
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)" "$(ZIG_COVERAGE_DIR)" "$(VERIFICATION_RESULTS_DIR)/unit"
	@python3 scripts/check_verification_graph.py validate --output "$(VERIFICATION_GRAPH_FILE)"
	@echo "→ [zig] Building component test binaries for coverage..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-bin test-lib-bin test-s3-bin
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig test-bin
	@set -eu; \
	 started=$$(date +%s); names=""; \
	 for component in $(ZIG_UNIT_COVERAGE_COMPONENTS); do \
	   name=$${component%%:*}; binary=$${component#*:}; output="$(ZIG_COVERAGE_DIR)/$$name"; \
	   echo "→ [zig] kcov component=$$name binary=$$binary"; \
	   rm -rf "$$output"; mkdir -p "$$output"; \
	   rm -f "$(ZIG_COVERAGE_DIR)/kcov-$$name.rc"; \
	   ( set +e; \
	     kcov --clean --include-pattern="$(CURDIR)/src" --exclude-pattern=_test.zig \
	       "$$output" "zig-out/bin/$$binary" \
	       >"$(ZIG_COVERAGE_DIR)/kcov-$$name.log" 2>&1; echo $$? >"$(ZIG_COVERAGE_DIR)/kcov-$$name.rc" ) & \
	   names="$$names $$name"; \
	 done; \
	 wait; \
	 failed=0; \
	 for name in $$names; do \
	   rc=$$(cat "$(ZIG_COVERAGE_DIR)/kcov-$$name.rc" 2>/dev/null || echo 1); \
	   case "$$rc" in ''|*[!0-9]*) rc=1;; esac; \
	   if [ "$$rc" -ne 0 ]; then \
	     echo "✗ Zig coverage component $$name exited $$rc"; \
	     tail -n 40 "$(ZIG_COVERAGE_DIR)/kcov-$$name.log"; failed=1; continue; \
	   fi; \
	   report=$$(find "$(ZIG_COVERAGE_DIR)/$$name" -name cobertura.xml -type f -size +0c -print -quit); \
	   test -n "$$report" || { echo "✗ Zig coverage component $$name produced no Cobertura report"; failed=1; }; \
	 done; \
	 [ "$$failed" -eq 0 ] || exit 1; \
	 echo "→ [zig] Running non-coverage unit owners..."; \
	 ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" zig build test-auth --summary all; \
	 ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" zig build -Dwith-bench-tools=true bench-incident-test --summary all; \
	 report_flags=""; \
	 for name in $$names; do \
	   report=$$(find "$(ZIG_COVERAGE_DIR)/$$name" -name cobertura.xml -type f -size +0c -print -quit); \
	   report_flags="$$report_flags --report $$report"; \
	 done; \
	 duration_ms=$$(( ($$(date +%s) - started) * 1000 )); \
	 python3 scripts/check_verification_graph.py write-result \
	   --graph "$(VERIFICATION_GRAPH_FILE)" \
	   --output "$(VERIFICATION_RESULTS_DIR)/unit/manifest.json" \
	   --execution unit --outcome success --duration-ms "$$duration_ms" \
	   --cache-state "$(VERIFICATION_CACHE_STATE)" --environment-label unit $$report_flags; \
	 echo "✓ [zig] unit coverage components are complete and reusable"
