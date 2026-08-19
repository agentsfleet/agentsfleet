# =============================================================================
# TEST-INTEGRATION — all integration tests (Zig in-process, DB, Redis)
# =============================================================================
# The compose infra these lanes consume — ports, URLs, cert, reset — lives in
# make/test-infra.mk.

.PHONY: test-integration test-integration-db test-integration-redis test-integration-kernel

# The runner's own real-process integration lane (build_runner.zig, no datastore):
# it forks real children and asserts real KERNEL behaviour — the env allowlist +
# kill(-pgid) tree reap + CLOEXEC proofs AND the security-enforcement proofs
# (seccomp trap / Landlock deny / cgroup pids+OOM cage). Linux-only (bodies
# SkipZigTest off-Linux), a distinct execution environment from the Postgres/Redis
# app lane below and the fast unit lane.
#
# Delegation discipline: the cgroup-cage proofs need a delegated cgroup-v2
# controller subtree. That delegation (scripts/cgroup-delegate.sh) is a
# DISPOSABLE-ENVIRONMENT concern — it drains the root cgroup + writes
# subtree_control, which must NEVER touch a developer's host. So it runs ONLY
# inside the macOS throwaway container below (and the privileged CI step). A bare
# `make test-integration-kernel` on a Linux host runs the lane WITHOUT delegating;
# the cgroup proofs then SkipZigTest (requireCgroupDelegation) — no host mutation,
# no false green. In production the runner's cgroup subtree is delegated by the
# init system (systemd Delegate=) / container runtime; this script is never deployed.
RUNNER_CI_IMAGE ?= ghcr.io/agentsfleet/ci-zig-alpine:0.16.0-r4

test-integration-kernel:  ## Run the runner's real-process kernel integration tests (env/kill-tree + seccomp/Landlock/cgroup); native on Linux, auto-containerized on macOS
ifeq ($(shell uname),Darwin)
	@echo "→ [kernel] macOS host has no Linux kernel — running the lane in a disposable privileged Linux container..."
	@# Cache dirs are forced back into the repo here: only $(CURDIR) is mounted, so
	@# the shared $(HOME) global cache does not exist inside the container and
	@# would be rebuilt from scratch and discarded on every run. It must stay
	@# separate from the host's cache regardless — this is a Linux target built
	@# against a different libc.
	@docker run --rm --privileged --cgroupns=private --platform "linux/$(shell uname -m | sed 's/x86_64/amd64/')" \
	  -v "$(CURDIR)":"$(CURDIR)" -w "$(CURDIR)" \
	  -e ZIG_GLOBAL_CACHE_DIR="$(CURDIR)/.tmp/zig-kernel-global-cache" \
	  -e ZIG_LOCAL_CACHE_DIR="$(CURDIR)/.tmp/zig-kernel-local-cache" \
	  "$(RUNNER_CI_IMAGE)" sh -c 'sh scripts/cgroup-delegate.sh make test-integration-kernel'
else
	@echo "→ [kernel] Running runner integration tests via build_runner.zig (env filter + kill-tree + seccomp/Landlock/cgroup)..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig test-integration --summary all
	@echo "✓ [kernel] Runner integration tests passed (Linux real-process proofs)"
endif


test-integration-db: $(TEST_STATE_DEP)  ## Run real DB-backed integration suite only
	@db_url="$$TEST_DATABASE_URL"; \
	if [ -z "$$db_url" ]; then db_url="$(TEST_DATABASE_URL_LOCAL)"; fi; \
	case "$$db_url" in \
	  *localhost*|*127.0.0.1*) \
	    case "$$db_url" in \
	      *sslmode=*) ;; \
	      *\?*) db_url="$$db_url&sslmode=disable" ;; \
	      *) db_url="$$db_url?sslmode=disable" ;; \
	    esac ;; \
	esac; \
	echo "→ [agentsfleetd] Running DB-backed integration tests using $$db_url..."; \
	mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"; \
	echo "→ [agentsfleetd] Auto-migrating test database..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	DATABASE_URL_MIGRATOR="$$db_url" \
	zig build run -- migrate; \
	echo "→ [agentsfleetd] Migration done, running tests..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	LIVE_DB=1 \
	TEST_DATABASE_URL="$$db_url" \
	AGENTSFLEET_QSTASH_LIVE_URL="$(QSTASH_DEV_URL_LOCAL)" \
	AGENTSFLEET_QSTASH_LIVE_TOKEN="$(QSTASH_DEV_TOKEN_LOCAL)" \
	zig build test-integration $(ZIG_TEST_FILTER_ARG)
	@echo "✓ [agentsfleetd] DB-backed integration tests passed"

test-integration-redis: $(TEST_STATE_DEP)  ## Run Redis-backed integration suite only
	@redis_tls_test_url="$$TEST_REDIS_TLS_URL"; \
	if [ -z "$$redis_tls_test_url" ] && [ -n "$$REDIS_URL" ]; then \
	  case "$$REDIS_URL" in \
	    rediss://*) redis_tls_test_url="$$REDIS_URL" ;; \
	  esac; \
	fi; \
	if [ -z "$$redis_tls_test_url" ]; then redis_tls_test_url="$(TEST_REDIS_TLS_URL_LOCAL)"; fi; \
	echo "→ [agentsfleetd] Running Redis integration tests using $$redis_tls_test_url..."; \
	mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"; \
	env -u TEST_DATABASE_URL -u LIVE_DB \
	  ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	  ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	  TEST_REDIS_TLS_URL="$$redis_tls_test_url" \
	  REDIS_URL_API="$$redis_tls_test_url" \
	  REDIS_TLS_CA_CERT_FILE="$(TEST_REDIS_TLS_CA_CERT)" \
	  zig build test-integration $(ZIG_TEST_FILTER_ARG)
	@echo "✓ [agentsfleetd] Redis integration tests passed"

# The ONE lane that executes the daemon integration suite against live services,
# and therefore the one that measures it. It used to run the suite bare while
# `test-coverage-zig` ran the same binary again under kcov, so a full
# verification executed ~2000 live-service tests twice and Continuous
# Integration (CI) paid for it on two runners. Instrumenting the run this lane
# was already making is what removes the second one: the same execution yields
# the integration verdict and the integration coverage.
#
# It runs the built binary rather than `zig build test-integration`, because kcov
# needs a binary to drive. That binary exits 0 whether or not its tests ran AND
# whether or not they passed — every one of its tests bails at a guard without a
# live database and Redis — so its exit status proves nothing and the verdict is
# read off the tally below. Zero passes means the suite never ran; any failure
# means the number describes a broken suite. Both fail the lane rather than
# yielding a report that is technically valid and completely wrong.
test-integration: $(TEST_STATE_DEP)  ## Run the daemon integration suite once against real DB + Redis, under coverage
	@command -v kcov >/dev/null 2>&1 || { echo "✗ kcov is required for Zig coverage (install: brew install kcov or apt-get install kcov)"; exit 1; }
	@db_url="$$TEST_DATABASE_URL"; \
	if [ -z "$$db_url" ]; then db_url="$(TEST_DATABASE_URL_LOCAL)"; fi; \
	case "$$db_url" in \
	  *localhost*|*127.0.0.1*) \
	    case "$$db_url" in \
	      *sslmode=*) ;; \
	      *\?*) db_url="$$db_url&sslmode=disable" ;; \
	      *) db_url="$$db_url?sslmode=disable" ;; \
	    esac ;; \
	esac; \
	redis_url="$$TEST_REDIS_TLS_URL"; \
	if [ -z "$$redis_url" ] && [ -n "$$REDIS_URL" ]; then \
	  case "$$REDIS_URL" in \
	    rediss://*) redis_url="$$REDIS_URL" ;; \
	  esac; \
	fi; \
	if [ -z "$$redis_url" ]; then redis_url="$(TEST_REDIS_TLS_URL_LOCAL)"; fi; \
	mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)" "$(ZIG_COVERAGE_DIR)"; \
	echo "→ [agentsfleet-runner] Building the runner binary in the background so it overlaps the migrate compile (separate build graph, no datastore; silent until it links)..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	zig build --build-file build_runner.zig & \
	runner_build_pid=$$!; \
	echo "→ [agentsfleetd] Auto-migrating test database..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	DATABASE_URL_MIGRATOR="$$db_url" \
	zig build run -- migrate; migrate_rc=$$?; \
	echo "→ [agentsfleet-runner] Waiting for the background runner build (usually already linked during migrate)..."; \
	wait "$$runner_build_pid" || { echo "✗ [agentsfleet-runner] Runner binary build failed"; exit 1; }; \
	[ "$$migrate_rc" -eq 0 ] || { echo "✗ [agentsfleetd] Test database migration failed (exit $$migrate_rc) — not running tests against an unmigrated DB"; exit 1; }; \
	echo "✓ [agentsfleet-runner] Runner binary built."; \
	echo "→ [catalogue] model_library seeding is SELF-SERVE: the Zig seed tests apply"; \
	echo "  samples/fixtures/model-library/seed.sql over their own pg connection —"; \
	echo "  the CI zig container has neither node nor psql, so no make step seeds here."; \
	echo "→ [agentsfleetd] Building the integration test binary (silent zig compile first, then the suite)..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	zig build install test-integration-bin $(ZIG_TEST_FILTER_ARG); \
	output="$(ZIG_COVERAGE_DIR)/integration"; \
	echo "→ [zig] kcov component=integration binary=$(ZIG_INTEGRATION_TEST_BIN) (live datastores, serial)"; \
	rm -rf "$$output"; mkdir -p "$$output"; \
	rm -f "$(ZIG_COVERAGE_DIR)/kcov-integration.rc"; \
	( set +e; \
	  $(ZIG_COVERAGE_ENV) \
	  $(ZIG_COVERAGE_KCOV) \
	    "$$output" "zig-out/bin/$(ZIG_INTEGRATION_TEST_BIN)" $(if $(SEED),--seed $(SEED),) \
	    >"$(ZIG_COVERAGE_DIR)/kcov-integration.log" 2>&1; echo $$? >"$(ZIG_COVERAGE_DIR)/kcov-integration.rc" ); \
	tail -n 40 "$(ZIG_COVERAGE_DIR)/kcov-integration.log"
	@# The lifecycle component costs a rebuild. `cmd/serve.zig` is the daemon's
	@# boot sequence and read 0% — 115 reachable lines — because nothing in the
	@# unfiltered run drives it: the one test that boots the real `serve.run`
	@# skips unless it is isolated, since it installs signal handlers, binds a
	@# port and perturbs process-global state the other ~2000 tests share. The
	@# binary takes its filter at BUILD time, so measuring that test means
	@# rebuilding filtered — which is why this runs after the unfiltered
	@# component rather than replacing it. No test runs twice: the one this
	@# executes is the one the unfiltered run skipped.
	@#
	@# Skipped under a narrowing TEST_FILTER, which has already replaced the
	@# graph's own filters; rebuilding a second time from a narrowed tree would
	@# measure whatever that filter happened to select.
	@set -eu; \
	 db_url="$${TEST_DATABASE_URL:-$(TEST_DATABASE_URL_LOCAL)}"; \
	 redis_url="$${TEST_REDIS_TLS_URL:-$(TEST_REDIS_TLS_URL_LOCAL)}"; \
	 names="integration"; \
	 if [ -z "$(strip $(TEST_FILTER))" ]; then \
	   output="$(ZIG_COVERAGE_DIR)/lifecycle"; \
	   echo "→ [zig] rebuilding the integration binary filtered to the lifecycle proof"; \
	   ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	   ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	   zig build test-integration-bin -Dtest-filter="$(LIFECYCLE_TEST_FILTER)"; \
	   echo "→ [zig] kcov component=lifecycle binary=$(ZIG_INTEGRATION_TEST_BIN) (real serve.run, isolated, serial)"; \
	   rm -rf "$$output"; mkdir -p "$$output"; \
	   rm -f "$(ZIG_COVERAGE_DIR)/kcov-lifecycle.rc"; \
	   ( set +e; \
	     $(LIFECYCLE_ISOLATION_ENV)=1 \
	     $(ZIG_COVERAGE_ENV) \
	     $(ZIG_COVERAGE_KCOV) \
	       "$$output" "zig-out/bin/$(ZIG_INTEGRATION_TEST_BIN)" \
	       >"$(ZIG_COVERAGE_DIR)/kcov-lifecycle.log" 2>&1; echo $$? >"$(ZIG_COVERAGE_DIR)/kcov-lifecycle.rc" ); \
	   names="$$names lifecycle"; \
	 fi; \
	 bash scripts/check-kcov-components.sh "$(ZIG_COVERAGE_DIR)" \
	   '$(ZIG_TEST_FAILURE_GREP)' '$(ZIG_TEST_LOG_NOISE)' $$names
	@# The verdict. Read off the tally because the binary's exit status cannot
	@# carry it, and asserted before any report is offered as evidence: a report
	@# over a suite that never ran, or over one that failed, is not a measurement.
	@set -eu; \
	 summary=$$(grep -E '^[0-9]+ passed;' "$(ZIG_COVERAGE_DIR)/kcov-integration.log" | tail -n 1); \
	 passed=$$(printf '%s' "$$summary" | sed -n 's/^\([0-9][0-9]*\) passed;.*/\1/p'); \
	 suite_failed=$$(printf '%s' "$$summary" | sed -n 's/.*; \([0-9][0-9]*\) failed.*/\1/p'); \
	 if [ -z "$$passed" ] || [ "$$passed" -eq 0 ]; then \
	   echo "✗ the integration suite reported no passing tests — coverage would be measured over a suite that never ran"; \
	   tail -n 20 "$(ZIG_COVERAGE_DIR)/kcov-integration.log"; exit 1; \
	 fi; \
	 if [ -n "$$suite_failed" ] && [ "$$suite_failed" -ne 0 ]; then \
	   echo "✗ the integration suite reported $$suite_failed failing test(s) — coverage over a failing suite is not a measurement"; \
	   echo "--- failing tests (component=integration) ---"; \
	   grep -v -E '$(ZIG_TEST_LOG_NOISE)' "$(ZIG_COVERAGE_DIR)/kcov-integration.log" \
	     | grep -B 1 -E '$(ZIG_TEST_FAILURE_GREP)' | head -n 60 || true; \
	   exit 1; \
	 fi; \
	 echo "✓ [agentsfleetd] integration suite executed ($$summary)"; \
	 if [ -z "$(strip $(TEST_FILTER))" ]; then \
	   grep -q "$(LIFECYCLE_RUN_MARKER)" "$(ZIG_COVERAGE_DIR)/kcov-lifecycle.log" || { \
	     echo "✗ the boot→drain lifecycle test did not run (it skips without live datastores); the component would measure a process that started and stopped, and the daemon's boot sequence would read dark"; \
	     tail -n 20 "$(ZIG_COVERAGE_DIR)/kcov-lifecycle.log"; exit 1; \
	   }; \
	   echo "✓ [agentsfleetd] lifecycle boot→drain executed (the real serve.run is measured)"; \
	 fi
	@# A narrowed run records its evidence marked filtered, and the grade refuses
	@# a filtered manifest. Recording nothing would be worse: the previous run's
	@# manifest would survive and read as this run's.
	@$(ZIG_EVIDENCE_RECORD) \
	  --producer test-integration \
	  --manifest "$(ZIG_EVIDENCE_INTEGRATION)" \
	  --coverage-dir "$(ZIG_COVERAGE_DIR)" \
	  $(if $(strip $(TEST_FILTER)),--filtered --component integration,$(foreach name,$(ZIG_COVERAGE_LIVE_COMPONENTS),--component $(name)))
	@# Grading is `test-coverage-grade`'s job, invoked here only when the other
	@# producer has already run — which is what the canonical sequence does. No
	@# unit evidence is not a failure of this lane: producing it was never this
	@# lane's work. Evidence that exists but does not fit IS a failure, and the
	@# grade is what says which field disagreed.
	@if [ -n "$(strip $(TEST_FILTER))" ]; then \
	  echo "→ [zig] merged coverage floor not graded — TEST_FILTER narrowed this run"; \
	elif [ -f "$(ZIG_EVIDENCE_UNIT)" ]; then \
	  $(MAKE) --no-print-directory test-coverage-grade; \
	else \
	  echo "→ [zig] merged coverage floor not graded — no unit evidence at $(ZIG_EVIDENCE_UNIT);"; \
	  echo "  run 'make test-unit-all' first, or 'make test-coverage-grade' once both lanes have run"; \
	fi
	@echo "✓ [agentsfleetd] All integration tests passed"
