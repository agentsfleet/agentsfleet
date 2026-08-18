# =============================================================================
# VERIFICATION — coverage-producing integration owners and final union
# =============================================================================

.PHONY: _test-integration-build _test-integration-daemon-build _test-integration-runner-build _test-integration-isolation _test-integration-runner-coverage _test-integration-runner-coverage-native _test-integration-shard _test-integration-grade _test-integration-timing

_test-integration-build: _test-integration-daemon-build _test-integration-runner-build

_test-integration-daemon-build:
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)" "$(ZIG_COVERAGE_DIR)" "$(VERIFICATION_RESULTS_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build install test-integration-bin

_test-integration-runner-build:
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)" "$(ZIG_COVERAGE_DIR)" "$(VERIFICATION_RESULTS_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig test-integration-bin
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig

_test-integration-isolation:
	@set -eu; \
	 [ "$${TEST_INFRA:-}" != provided ] \
	 || { echo "✗ aggregate sharding cannot share TEST_INFRA=provided; invoke one shard per isolated worker"; exit 1; }; \
	 values=""; index=0; \
	 while [ "$$index" -lt "$(INTEGRATION_SHARD_COUNT)" ]; do \
	   values="$$values\n$$(basename "$(CURDIR)")-shard-$$index|$$(( $(AGENTSFLEET_PG_HOST_PORT) + index * 3 ))|$$(( $(AGENTSFLEET_REDIS_HOST_PORT) + index * 3 ))|$$(( $(AGENTSFLEET_QSTASH_HOST_PORT) + index * 3 ))"; \
	   index=$$((index + 1)); \
	 done; \
	 total=$$(printf '%b\n' "$$values" | sed '/^$$/d' | wc -l | tr -d ' '); \
	 unique=$$(printf '%b\n' "$$values" | sed '/^$$/d' | sort -u | wc -l | tr -d ' '); \
	 [ "$$total" -eq "$(INTEGRATION_SHARD_COUNT)" ] && [ "$$unique" -eq "$$total" ] \
	 || { echo "✗ integration shard runtime state collides"; exit 1; }; \
	 echo "✓ [integration] $$total disjoint shard runtime allocations"

_test-integration-runner-coverage:
ifeq ($(shell uname),Darwin)
	@echo "→ [kernel] Running coverage in a disposable privileged Linux container..."
	@docker build --platform "linux/$(RUNNER_COVERAGE_PLATFORM)" \
	  -t "$(RUNNER_COVERAGE_IMAGE)" -f scripts/Dockerfile.runner-coverage .
	@mkdir -p "$(CURDIR)/.tmp/runner-kernel" "$(CURDIR)/.tmp/zig-kernel-cross-cache"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(CURDIR)/.tmp/zig-kernel-cross-cache" \
	 zig build --build-file build_runner.zig test-integration-bin \
	   --prefix "$(CURDIR)/.tmp/runner-kernel" -Dtarget="$(RUNNER_COVERAGE_ARCH)-linux-gnu"
	@docker run --rm --privileged --cgroupns=private \
	  --platform "linux/$(RUNNER_COVERAGE_PLATFORM)" \
	  -v "$(CURDIR)":"$(CURDIR)" -w "$(CURDIR)" \
	  -v "$$(git rev-parse --path-format=absolute --git-common-dir)":"$$(git rev-parse --path-format=absolute --git-common-dir)" \
	  -e HOST_UID="$$(id -u)" -e HOST_GID="$$(id -g)" \
	  "$(RUNNER_COVERAGE_IMAGE)" sh -c \
	  'git config --global --add safe.directory "$(CURDIR)"; set +e; \
	   sh scripts/cgroup-delegate.sh make _test-integration-runner-coverage-native \
	     RUNNER_INTEGRATION_TEST_BIN="$(CURDIR)/.tmp/runner-kernel/bin/agentsfleet-runner-integration-tests"; rc=$$?; \
	   chown -R "$$HOST_UID:$$HOST_GID" coverage/zig/runner_integration .tmp/verification-results/runner-kernel 2>/dev/null || true; exit $$rc'
else
	@$(MAKE) --no-print-directory _test-integration-runner-coverage-native
endif

_test-integration-runner-coverage-native:
	@set -eu; \
	 started=$$(date +%s); name=runner_integration; \
	 output="$(ZIG_COVERAGE_DIR)/$$name"; log="$(ZIG_COVERAGE_DIR)/kcov-$$name.log"; \
	 rm -rf "$$output" "$(VERIFICATION_RESULTS_DIR)/runner-kernel"; \
	 mkdir -p "$$output" "$(VERIFICATION_RESULTS_DIR)/runner-kernel"; \
	 echo "→ [zig] kcov component=$$name binary=agentsfleet-runner-integration-tests"; \
	 kcov --clean --include-pattern="$(CURDIR)/src" --exclude-pattern=_test.zig \
	   "$$output" "$(RUNNER_INTEGRATION_TEST_BIN)" >"$$log" 2>&1 \
	 || { echo "✗ runner-kernel coverage failed"; tail -n 40 "$$log"; exit 1; }; \
	 grep -Eq '[0-9]+ passed; [0-9]+ skipped; 0 failed\.$$' "$$log" \
	 || { echo "✗ runner-kernel test process reported a failure"; tail -n 40 "$$log"; exit 1; }; \
	 report=$$(find "$$output" -name cobertura.xml -type f -size +0c -print -quit); \
	 test -n "$$report" || { echo "✗ runner-kernel produced no coverage report"; exit 1; }; \
	 duration_ms=$$(( ($$(date +%s) - started) * 1000 )); \
	 python3 scripts/check_verification_graph.py write-result \
	   --graph "$(VERIFICATION_GRAPH_FILE)" \
	   --output "$(VERIFICATION_RESULTS_DIR)/runner-kernel/manifest.json" \
	   --execution runner-kernel --outcome success --duration-ms "$$duration_ms" \
	   --cache-state "$(VERIFICATION_CACHE_STATE)" --environment-label runner-kernel \
	   --report "$$report"

_test-integration-shard:
	@set -eu; \
	 index="$(SHARD_INDEX)"; count="$(INTEGRATION_SHARD_COUNT)"; \
	 case "$$index" in ''|*[!0-9]*) echo "✗ SHARD_INDEX must be numeric"; exit 1;; esac; \
	 [ "$$index" -lt "$$count" ] || { echo "✗ SHARD_INDEX=$$index is outside count=$$count"; exit 1; }; \
	 result_dir="$(VERIFICATION_RESULTS_DIR)/integration-shard-$$index"; \
	 rm -rf "$$result_dir"; mkdir -p "$$result_dir"; \
	 cert="$$result_dir/redis-ca.crt"; \
	 cleanup=0; \
	 if [ "$${TEST_INFRA:-}" = provided ]; then \
	   isolation_key="$${INTEGRATION_SHARD_ISOLATION_KEY:?INTEGRATION_SHARD_ISOLATION_KEY is required with TEST_INFRA=provided}"; \
	   db_url="$${TEST_DATABASE_URL:?TEST_DATABASE_URL is required with TEST_INFRA=provided}"; \
	   redis_url="$${TEST_REDIS_TLS_URL:?TEST_REDIS_TLS_URL is required with TEST_INFRA=provided}"; \
	   cert="$${TEST_REDIS_TLS_CA_CERT:-$(TEST_REDIS_TLS_CA_CERT)}"; \
	   qstash_url="$${AGENTSFLEET_QSTASH_LIVE_URL:-$(QSTASH_DEV_URL_LOCAL)}"; \
	 else \
	   pg_port=$$(( $(AGENTSFLEET_PG_HOST_PORT) + index * 3 )); \
	   redis_port=$$(( $(AGENTSFLEET_REDIS_HOST_PORT) + index * 3 )); \
	   qstash_port=$$(( $(AGENTSFLEET_QSTASH_HOST_PORT) + index * 3 )); \
	   project="$$(basename "$(CURDIR)")-shard-$$index"; cleanup=1; \
	   isolation_key="$$project"; \
	   export COMPOSE_PROJECT_NAME="$$project" AGENTSFLEET_PG_HOST_PORT="$$pg_port"; \
	   export AGENTSFLEET_REDIS_HOST_PORT="$$redis_port" AGENTSFLEET_QSTASH_HOST_PORT="$$qstash_port"; \
	   $(MAKE) --no-print-directory _reset-test-db TEST_REDIS_TLS_CA_CERT="$$cert"; \
	   db_url="postgres://agentsfleet:agentsfleet@localhost:$$pg_port/agentsfleetdb?sslmode=disable"; \
	   redis_url="rediss://:agentsfleet@localhost:$$redis_port"; \
	   qstash_url="http://localhost:$$qstash_port"; \
	 fi; \
	 finish() { if [ "$$cleanup" -eq 1 ]; then docker compose down >/dev/null 2>&1 || true; fi; }; \
	 trap finish EXIT INT TERM; \
	 DATABASE_URL_MIGRATOR="$$db_url" zig-out/bin/agentsfleetd migrate; \
	 if [ "$$index" -eq $$(( count - 1 )) ]; then \
	   name=lifecycle; isolated_env="$(LIFECYCLE_ISOLATION_ENV)=1"; \
	 else \
	   name="integration-shard-$$index"; isolated_env=""; \
	 fi; \
	 output="$(ZIG_COVERAGE_DIR)/$$name"; log="$(ZIG_COVERAGE_DIR)/kcov-$$name.log"; \
	 rm -rf "$$output"; mkdir -p "$$output"; started=$$(date +%s); \
	 echo "→ [zig] kcov component=$$name shard=$$index/$$count"; \
	 env $$isolated_env LIVE_DB=1 TEST_DATABASE_URL="$$db_url" \
	   AGENTSFLEET_TEST_INSTRUMENTED=1 \
	   TEST_REDIS_TLS_URL="$$redis_url" REDIS_URL_API="$$redis_url" \
	   REDIS_TLS_CA_CERT_FILE="$$cert" AGENTSFLEET_RUNNER_BIN="$(CURDIR)/zig-out/bin/agentsfleet-runner" \
	   AGENTSFLEET_QSTASH_LIVE_URL="$$qstash_url" AGENTSFLEET_QSTASH_LIVE_TOKEN="$(QSTASH_DEV_TOKEN_LOCAL)" \
	   kcov --clean --include-pattern="$(CURDIR)/src" --exclude-pattern=_test.zig \
	   "$$output" "zig-out/bin/agentsfleetd-integration-tests" \
	   --shard-index="$$index" --shard-count="$$count" >"$$log" 2>&1 \
	 || { echo "✗ integration shard $$index failed"; tail -n 60 "$$log"; exit 1; }; \
	 grep -Eq '[0-9]+ passed; [0-9]+ skipped; 0 failed\.$$' "$$log" \
	 || { echo "✗ integration shard $$index test process reported a failure"; tail -n 60 "$$log"; exit 1; }; \
	 report=$$(find "$$output" -name cobertura.xml -type f -size +0c -print -quit); \
	 test -n "$$report" || { echo "✗ integration shard $$index produced no coverage report"; exit 1; }; \
	 if [ "$$name" = lifecycle ]; then \
	   grep -q "$(LIFECYCLE_RUN_MARKER)" "$$log" \
	   || { echo "✗ lifecycle shard did not execute the boot→drain proof"; exit 1; }; \
	 fi; \
	 duration_ms=$$(( ($$(date +%s) - started) * 1000 )); \
	 python3 scripts/check_verification_graph.py write-result \
	   --graph "$(VERIFICATION_GRAPH_FILE)" --output "$$result_dir/manifest.json" \
	   --execution integration-shard --outcome success --duration-ms "$$duration_ms" \
	   --cache-state "$(VERIFICATION_CACHE_STATE)" --shard-index "$$index" \
	   --shard-count "$$count" --isolation-key "$$isolation_key" \
	   --environment-label integration --report "$$report"

_test-integration-grade:
	@set -eu; \
	 manifest_flags="--manifest $(VERIFICATION_RESULTS_DIR)/unit/manifest.json --manifest $(VERIFICATION_RESULTS_DIR)/runner-kernel/manifest.json"; \
	 shard_flags=""; index=0; \
	 while [ "$$index" -lt "$(INTEGRATION_SHARD_COUNT)" ]; do \
	   manifest_flags="$$manifest_flags --manifest $(VERIFICATION_RESULTS_DIR)/integration-shard-$$index/manifest.json"; \
	   if [ "$$index" -lt $$(( $(INTEGRATION_SHARD_COUNT) - 1 )) ]; then \
	     shard_flags="$$shard_flags --integration-shard integration-shard-$$index"; \
	   fi; \
	   index=$$((index + 1)); \
	 done; \
	 python3 scripts/check_verification_graph.py validate-results \
	   --graph "$(VERIFICATION_GRAPH_FILE)" --expected-shard-count "$(INTEGRATION_SHARD_COUNT)" $$manifest_flags; \
	 component_flags=""; \
	 for name in $(ZIG_UNIT_COVERAGE_NAMES) $(filter-out integration,$(ZIG_INTEGRATION_COVERAGE_COMPONENTS)); do \
	   component_flags="$$component_flags --component $$name"; \
	 done; \
	 for name in $(ZIG_COVERAGE_REQUIRED_COMPONENTS); do \
	   component_flags="$$component_flags --require-component $$name"; \
	 done; \
	 for name in $(ZIG_COVERAGE_REQUIRED_ROOTS); do component_flags="$$component_flags --require-root $$name"; done; \
	 for pair in $(ZIG_COVERAGE_FOLDER_FLOORS); do component_flags="$$component_flags --folder-floor $$pair"; done; \
	 for pair in $(ZIG_COVERAGE_FOLDER_TARGETS); do component_flags="$$component_flags --folder-target $$pair"; done; \
	 python3 scripts/check_zig_coverage.py --coverage-dir "$(ZIG_COVERAGE_DIR)" \
	   $$component_flags $$shard_flags --lifecycle-shard lifecycle \
	   --expected-integration-shards "$(INTEGRATION_SHARD_COUNT)" \
	   --min-pct "$(ZIG_COVERAGE_MIN_PCT)" --target-pct "$(ZIG_COVERAGE_TARGET_PCT)" \
	   --min-files "$(ZIG_COVERAGE_MIN_FILES)" --min-lines "$(ZIG_COVERAGE_MIN_MEASURED_LINES)" \
	   --merged-report "$(ZIG_COVERAGE_DIR)/merged" --repo-root "$(CURDIR)" \
	   --summary-file "$(ZIG_COVERAGE_SUMMARY_FILE)"

_test-integration-timing:
	@set -eu; \
	 timing_flags="--timing $(VERIFICATION_TIMING_DIR)/runner-kernel.json"; index=0; \
	 while [ "$$index" -lt "$(INTEGRATION_SHARD_COUNT)" ]; do \
	   timing_flags="$$timing_flags --timing $(VERIFICATION_TIMING_DIR)/integration-shard-$$index.json"; \
	   index=$$((index + 1)); \
	 done; \
	 python3 scripts/verification_evidence.py summarize $$timing_flags \
	   --expected-shard-count "$(INTEGRATION_SHARD_COUNT)" \
	   --output "$(VERIFICATION_RESULTS_DIR)/timing-summary.json"
