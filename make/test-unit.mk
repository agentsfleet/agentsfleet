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

# Coverage measures the codebase, not a lane. The unit binaries and the
# integration binary cover largely DISJOINT code — the unit lanes never reach an
# HTTP handler or a store, because reaching one needs a live Postgres and Redis,
# which is exactly what the integration suite provides. Measuring only the unit
# binaries reported handlers at 0% and dragged the whole figure ~28 points below
# the truth, so this target runs both and merges them.
#
# It depends on the same datastore bootstrap `test-integration` uses, so the
# target boots what it needs instead of failing on a missing database.
test-coverage-zig:  ## Run and gate merged Zig line coverage across the unit lanes and the live-service integration suite
	@command -v kcov >/dev/null 2>&1 || { echo "✗ kcov is required for Zig coverage (install: brew install kcov or apt-get install kcov)"; exit 1; }
	@# The datastore bootstrap is invoked here rather than declared as a
	@# prerequisite so the tool check above runs first — booting containers only
	@# to fail on a missing kcov wastes a minute and buries the real message.
	@# Recipe bodies also expand at run time, so this reads TEST_STATE_DEP
	@# correctly regardless of the order the make fragments are included in.
	@$(MAKE) --no-print-directory $(TEST_STATE_DEP)
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
	@# The integration suite execs the runner binary and reads a migrated
	@# database; `install` builds the daemon exe that performs the migration.
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build install test-integration-bin
	@echo "→ [zig] Migrating the coverage database..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 DATABASE_URL_MIGRATOR="$${TEST_DATABASE_URL:-$(TEST_DATABASE_URL_LOCAL)}" \
	 zig build run -- migrate
	@# The components are independent kcov runs over already-built binaries
	@# writing to disjoint output directories, so they run concurrently and only
	@# the merge below needs them all. Each records its own exit status to a file
	@# rather than being tracked by process id: the status is what decides the
	@# gate, and a file maps it back to the component name without the shell
	@# gymnastics of pairing a pid list against a name list.
	@#
	@# Each component directory is REMOVED, not just `--clean`ed. kcov names its
	@# output subdirectory after a hash of the binary, and `--clean` only resets
	@# the directory for the hash it is writing — a rebuilt binary lands beside
	@# its predecessor rather than replacing it, and a stale sibling would rejoin
	@# the union below; a run whose suite never executed kept dragging the figure
	@# down for days after.
	@#
	@# The per-component reports are unioned by `scripts/check_zig_coverage.py`,
	@# NOT by `kcov --merge`. That merge silently returned only the three src/lib
	@# components on Linux — 24 files against macOS's 558 — from identical
	@# arguments and the same kcov 43, so the gate graded 2.8% of the codebase and
	@# called it 93.70%. The script fails when a component contributes nothing.
	@#
	@# `--exclude-pattern` keeps the test bodies OUT of the denominator. They are
	@# ~23k of the measured lines and are themselves ~90% covered — counting them
	@# inflated the figure by roughly seven points and, worse, made the gate
	@# satisfiable by writing more test files rather than covering more product.
	@#
	@# The integration component runs AFTER the unit group, not alongside it. Both
	@# binaries stand up test HTTP servers on ephemeral ports, and running them
	@# together lost a test to AddressInUse even though the harness retried on a
	@# fresh port. It is the long pole either way; a gate that flakes is worth
	@# less than the few minutes serialising it costs.
	@#
	@# It carries its own environment: every one of its tests bails at a guard
	@# without a live database and Redis, which is how a previous run produced a
	@# report over a suite that never executed. That binary also exits 0 whether
	@# or not its tests ran AND whether
	@# or not they passed, so its exit status proves nothing — the counts are read
	@# off the run below. Zero passes means the suite never ran; any failure means
	@# the number describes a broken suite. Both fail the target rather than
	@# yielding a report that is technically valid and completely wrong.
	@set -eu; \
	 db_url="$${TEST_DATABASE_URL:-$(TEST_DATABASE_URL_LOCAL)}"; \
	 redis_url="$${TEST_REDIS_TLS_URL:-$(TEST_REDIS_TLS_URL_LOCAL)}"; \
	 components="agentsfleetd:agentsfleetd-tests runner:agentsfleet-runner-tests lib:agentsfleet-lib-tests logging:agentsfleet-logging-tests deadline:agentsfleet-call-deadline-tests"; \
	 names=""; \
	 for component in $$components; do \
	   name=$${component%%:*}; binary=$${component#*:}; output="$(ZIG_COVERAGE_DIR)/$$name"; \
	   echo "→ [zig] kcov component=$$name binary=$$binary"; \
	   rm -rf "$$output"; mkdir -p "$$output"; \
	   rm -f ".tmp/kcov-$$name.rc"; \
	   ( set +e; \
	     LIVE_DB=1 \
	     TEST_DATABASE_URL="$$db_url" \
	     TEST_REDIS_TLS_URL="$$redis_url" \
	     REDIS_URL_API="$$redis_url" \
	     REDIS_TLS_CA_CERT_FILE="$(TEST_REDIS_TLS_CA_CERT)" \
	     AGENTSFLEET_RUNNER_BIN="$(CURDIR)/zig-out/bin/agentsfleet-runner" \
	     AGENTSFLEET_QSTASH_LIVE_URL="$(QSTASH_DEV_URL_LOCAL)" \
	     AGENTSFLEET_QSTASH_LIVE_TOKEN="$(QSTASH_DEV_TOKEN_LOCAL)" \
	     kcov --clean --include-pattern="$(CURDIR)/src" --exclude-pattern=_test.zig \
	       "$$output" "zig-out/bin/$$binary" \
	       >".tmp/kcov-$$name.log" 2>&1; echo $$? >".tmp/kcov-$$name.rc" ) & \
	   names="$$names $$name"; \
	 done; \
	 wait; \
	 integration_output="$(ZIG_COVERAGE_DIR)/integration"; \
	 echo "→ [zig] kcov component=integration binary=agentsfleetd-integration-tests (live datastores, serial)"; \
	 rm -rf "$$integration_output"; mkdir -p "$$integration_output"; \
	 rm -f ".tmp/kcov-integration.rc"; \
	 ( set +e; \
	   LIVE_DB=1 \
	   TEST_DATABASE_URL="$$db_url" \
	   TEST_REDIS_TLS_URL="$$redis_url" \
	   REDIS_URL_API="$$redis_url" \
	   REDIS_TLS_CA_CERT_FILE="$(TEST_REDIS_TLS_CA_CERT)" \
	   AGENTSFLEET_RUNNER_BIN="$(CURDIR)/zig-out/bin/agentsfleet-runner" \
	   AGENTSFLEET_QSTASH_LIVE_URL="$(QSTASH_DEV_URL_LOCAL)" \
	   AGENTSFLEET_QSTASH_LIVE_TOKEN="$(QSTASH_DEV_TOKEN_LOCAL)" \
	   kcov --clean --include-pattern="$(CURDIR)/src" --exclude-pattern=_test.zig \
	     "$$integration_output" "zig-out/bin/agentsfleetd-integration-tests" \
	     >".tmp/kcov-integration.log" 2>&1; echo $$? >".tmp/kcov-integration.rc" ); \
	 names="$$names integration"; \
	 failed=0; \
	 for name in $$names; do \
	   rc=$$(cat ".tmp/kcov-$$name.rc" 2>/dev/null || echo 1); \
	   case "$$rc" in ''|*[!0-9]*) rc=1;; esac; \
	   if [ "$$rc" -ne 0 ]; then \
	     echo "✗ Zig coverage component $$name exited $$rc"; \
	     echo "--- failing tests (component=$$name) ---"; \
	     grep -E '\.\.\.FAIL|^error: .*failed:' ".tmp/kcov-$$name.log" | head -n 60 || true; \
	     echo "--- tally (component=$$name) ---"; \
	     grep -E '^[0-9]+ passed;' ".tmp/kcov-$$name.log" | tail -n 1 || true; \
	     echo "--- last 40 lines (component=$$name) ---"; \
	     tail -n 40 ".tmp/kcov-$$name.log"; failed=1; continue; \
	   fi; \
	   report=$$(find "$(ZIG_COVERAGE_DIR)/$$name" -name cobertura.xml -type f -size +0c -print -quit); \
	   test -n "$$report" || { echo "✗ Zig coverage component $$name produced no Cobertura report"; failed=1; }; \
	 done; \
	 [ "$$failed" -eq 0 ] || exit 1; \
	 summary=$$(grep -E '^[0-9]+ passed;' ".tmp/kcov-integration.log" | tail -n 1); \
	 passed=$$(printf '%s' "$$summary" | sed -n 's/^\([0-9][0-9]*\) passed;.*/\1/p'); \
	 suite_failed=$$(printf '%s' "$$summary" | sed -n 's/.*; \([0-9][0-9]*\) failed.*/\1/p'); \
	 if [ -z "$$passed" ] || [ "$$passed" -eq 0 ]; then \
	   echo "✗ the integration suite reported no passing tests — coverage would be measured over a suite that never ran"; \
	   tail -n 20 ".tmp/kcov-integration.log"; exit 1; \
	 fi; \
	 if [ -n "$$suite_failed" ] && [ "$$suite_failed" -ne 0 ]; then \
	   echo "✗ the integration suite reported $$suite_failed failing test(s) — coverage over a failing suite is not a measurement"; \
	   echo "--- failing tests (component=integration) ---"; \
	   grep -E '\.\.\.FAIL|^error: .*failed:' ".tmp/kcov-integration.log" | head -n 60 || true; \
	   exit 1; \
	 fi; \
	 echo "✓ [zig] integration suite executed ($$summary)"; \
	 component_flags=""; \
	 for name in $$names; do component_flags="$$component_flags --component $$name"; done; \
	 python3 scripts/check_zig_coverage.py \
	   --coverage-dir "$(ZIG_COVERAGE_DIR)" \
	   $$component_flags \
	   --min-pct "$(ZIG_COVERAGE_MIN_LINES)" \
	   --merged-report "$(ZIG_COVERAGE_DIR)/merged" \
	   --summary-file .tmp/zig-coverage.txt
