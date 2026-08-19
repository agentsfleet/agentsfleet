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

# Coverage measures the codebase, not a lane. The unit binaries and the daemon
# integration binary cover largely DISJOINT code — the unit lanes never reach an
# HTTP handler or a store, because reaching one needs a live Postgres and Redis,
# which is exactly what the integration suite provides. Measuring only the unit
# binaries reported handlers at 0% and dragged the whole figure ~28 points below
# the truth, so the union still spans both.
#
# What changed is who runs which half. This lane used to run the daemon
# integration binary too — under kcov, serially, after its unit components — and
# `make test-integration` then ran the same graph again. One full verification
# executed that suite twice; in Continuous Integration (CI) two separate runners
# each booted datastores to do it. The live components moved to the lane that
# already owns live datastores, and this one keeps the unit components only.
#
# It still boots and migrates the datastores. The unit component binaries are
# handed `LIVE_DB=1` and a database URL and have been measured that way since
# these floors were set; taking the datastores away here would change what they
# cover, which is exactly the regression this workstream refuses to trade for
# speed.
#
# It no longer grades. The union it can see is seven components of nine, and a
# floor over seven is a floor over a different codebase. `make
# test-coverage-grade` owns the verdict; this lane records what it measured so
# that grade can refuse evidence that does not fit.
test-coverage-zig:  ## Run the unit Zig coverage components under kcov and record their evidence
	@command -v kcov >/dev/null 2>&1 || { echo "✗ kcov is required for Zig coverage (install: brew install kcov or apt-get install kcov)"; exit 1; }
	@# The datastore bootstrap is invoked here rather than declared as a
	@# prerequisite so the tool check above runs first — booting containers only
	@# to fail on a missing kcov wastes a minute and buries the real message.
	@# Recipe bodies also expand at run time, so this reads TEST_STATE_DEP
	@# correctly regardless of the order the make fragments are included in.
	@$(MAKE) --no-print-directory $(TEST_STATE_DEP)
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)" "$(ZIG_COVERAGE_DIR)" .tmp
	@echo "→ [zig] Building the unit component test binaries for coverage..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-bin
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig test-bin
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-lib-bin
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-s3-bin
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig test-integration-bin
	@# The runner binary, not the daemon integration binary: the runner
	@# integration component forks the real child, and `AGENTSFLEET_RUNNER_BIN`
	@# points at it.
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig
	@echo "→ [zig] Migrating the coverage database..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 DATABASE_URL_MIGRATOR="$${TEST_DATABASE_URL:-$(TEST_DATABASE_URL_LOCAL)}" \
	 zig build run -- migrate
	@# The components are independent kcov runs over already-built binaries
	@# writing to disjoint output directories, so they run concurrently and only
	@# the grade needs them all. Each records its own exit status to a file rather
	@# than being tracked by process id: the status is what decides the lane, and
	@# a file maps it back to the component name without the shell gymnastics of
	@# pairing a pid list against a name list.
	@#
	@# Each component directory is REMOVED, not just `--clean`ed. kcov names its
	@# output subdirectory after a hash of the binary, and `--clean` only resets
	@# the directory for the hash it is writing — a rebuilt binary lands beside
	@# its predecessor rather than replacing it, and a stale sibling would rejoin
	@# the union; a run whose suite never executed kept dragging the figure down
	@# for days after.
	@#
	@# The per-component reports are unioned by the grade target's checker, NOT
	@# by `kcov --merge`. That merge silently returned only the three src/lib
	@# components on Linux — 24 files against macOS's 558 — from identical
	@# arguments and the same kcov 43, so the gate graded 2.8% of the codebase and
	@# called it 93.70%.
	@set -eu; \
	 mkdir -p "$(ZIG_COVERAGE_DIR)"; \
	 db_url="$${TEST_DATABASE_URL:-$(TEST_DATABASE_URL_LOCAL)}"; \
	 redis_url="$${TEST_REDIS_TLS_URL:-$(TEST_REDIS_TLS_URL_LOCAL)}"; \
	 names=""; \
	 for component in $(ZIG_COVERAGE_UNIT_COMPONENTS); do \
	   name=$${component%%:*}; binary=$${component#*:}; output="$(ZIG_COVERAGE_DIR)/$$name"; \
	   echo "→ [zig] kcov component=$$name binary=$$binary"; \
	   rm -rf "$$output"; mkdir -p "$$output"; \
	   rm -f "$(ZIG_COVERAGE_DIR)/kcov-$$name.rc"; \
	   ( set +e; \
	     $(ZIG_COVERAGE_ENV) \
	     $(ZIG_COVERAGE_KCOV) \
	       "$$output" "zig-out/bin/$$binary" \
	       >"$(ZIG_COVERAGE_DIR)/kcov-$$name.log" 2>&1; echo $$? >"$(ZIG_COVERAGE_DIR)/kcov-$$name.rc" ) & \
	   names="$$names $$name"; \
	 done; \
	 wait; \
	 bash scripts/check-kcov-components.sh "$(ZIG_COVERAGE_DIR)" \
	   '$(ZIG_TEST_FAILURE_GREP)' '$(ZIG_TEST_LOG_NOISE)' $$names
	@$(ZIG_EVIDENCE_RECORD) \
	  --producer test-coverage-zig \
	  --manifest "$(ZIG_EVIDENCE_UNIT)" \
	  --coverage-dir "$(ZIG_COVERAGE_DIR)" \
	  $(foreach name,$(ZIG_COVERAGE_UNIT_NAMES),--component $(name))
	@echo "✓ [zig] unit coverage components collected; the merged floor is graded by 'make test-coverage-grade'"
