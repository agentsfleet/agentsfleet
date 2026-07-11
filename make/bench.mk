# =============================================================================
# BENCH — API benchmark and memory leak gates.
#
# `make bench` runs two tiers:
#   Tier-1  zbench micro-benchmarks   (tests/bench/micro.zig — ReleaseFast)
#   Tier-2  hey HTTP loadgen          (requires `hey` in PATH — mise installs it)
# =============================================================================

.PHONY: memleak bench bench-redis _bench-micro _bench-loadgen _ensure-test-bin _memleak-lane _memleak-boot-drain

# `make memleak` leak-gates all THREE test graphs — daemon (agentsfleetd),
# runner (build_runner.zig), lib (src/lib, three artifacts). On Linux the
# blocking gate is valgrind (full leak-check, propagates exit code, subsumes the
# allocator gate); on macOS/other it is the blocking std.testing.allocator run
# per binary + advisory `leaks`. A final focused lane proves the DB-gated
# boot→SIGTERM→drain lifecycle test actually RUNS under the gate (see below) —
# without it the full-suite valgrind run silently skips every DB test and the
# leak claim would be vacuous.
memleak:  ## Run Zig memory-leak gates across the daemon, runner, and lib test graphs
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@$(MAKE) _memleak-lane LANE=agentsfleetd MEMLEAK_BUILD="test-bin" MEMLEAK_OPENSSL_OFF=1 MEMLEAK_BINS="agentsfleetd-tests"
	@$(MAKE) _memleak-lane LANE=runner MEMLEAK_BUILD="--build-file build_runner.zig test-bin" MEMLEAK_OPENSSL_OFF=0 MEMLEAK_BINS="agentsfleet-runner-tests"
	@$(MAKE) _memleak-lane LANE=lib MEMLEAK_BUILD="test-lib-bin" MEMLEAK_OPENSSL_OFF=1 MEMLEAK_BINS="agentsfleet-lib-tests agentsfleet-logging-tests agentsfleet-call-deadline-tests"
	@$(MAKE) _memleak-boot-drain
	@echo "✓ memleak gate passed (agentsfleetd + runner + lib lanes + boot→drain lifecycle)"

# One parametrized lane. Build MEMLEAK_BUILD, then leak-gate every binary in
# MEMLEAK_BINS. The `|| exit 1` lives INSIDE the shell `for` because a bare `for`
# does not propagate a mid-list failure. `-Dopenssl=false` is a ROOT-graph option
# (daemon + lib build.zig) — valgrind chokes on OpenSSL's own pool allocations;
# the runner graph (build_runner.zig) links no OpenSSL and REJECTS the flag, so
# its lane passes MEMLEAK_OPENSSL_OFF=0. Linux pins ReleaseSafe (valgrind needs
# an optimized-but-safe binary); macOS uses the default build.
_memleak-lane:
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)"; \
	case "$$(uname -s)" in \
	  Linux) \
	    command -v valgrind >/dev/null 2>&1 || { echo "✗ valgrind is required on Linux for make memleak"; exit 1; }; \
	    echo "→ [$(LANE)] Building (ReleaseSafe$(if $(filter 1,$(MEMLEAK_OPENSSL_OFF)), openssl off)) for the valgrind gate..."; \
	    zig build $(MEMLEAK_BUILD) -Doptimize=ReleaseSafe $(if $(filter 1,$(MEMLEAK_OPENSSL_OFF)),-Dopenssl=false,) $(if $(MEMLEAK_CPU),-Dcpu=$(MEMLEAK_CPU),) || exit 1; \
	    for b in $(MEMLEAK_BINS); do \
	      echo "→ [$(LANE)] valgrind leak gate: $$b..."; \
	      valgrind --quiet --leak-check=full --show-leak-kinds=all --errors-for-leak-kinds=definite,possible --undef-value-errors=no --error-exitcode=1 zig-out/bin/$$b || exit 1; \
	    done;; \
	  Darwin) \
	    echo "→ [$(LANE)] Building for the allocator gate..."; \
	    zig build $(MEMLEAK_BUILD) $(if $(filter 1,$(MEMLEAK_OPENSSL_OFF)),-Dopenssl=false,) || exit 1; \
	    for b in $(MEMLEAK_BINS); do \
	      echo "→ [$(LANE)] allocator leak gate: $$b..."; \
	      zig-out/bin/$$b || exit 1; \
	      if command -v leaks >/dev/null 2>&1; then \
	        MallocStackLogging=1 leaks -atExit -- zig-out/bin/$$b >/dev/null || echo "→ [$(LANE)] leaks advisory unavailable in current runtime"; \
	      fi; \
	    done;; \
	  *) \
	    echo "→ [$(LANE)] platform=$$(uname -s): allocator gate only"; \
	    zig build $(MEMLEAK_BUILD) $(if $(filter 1,$(MEMLEAK_OPENSSL_OFF)),-Dopenssl=false,) || exit 1; \
	    for b in $(MEMLEAK_BINS); do zig-out/bin/$$b || exit 1; done;; \
	esac
	@echo "✓ [$(LANE)] memleak lane passed"

# The daemon lane above runs with NO DB/Redis env, so every DB-gated test — the
# boot→SIGTERM→drain lifecycle proof included — SKIPS under valgrind. This lane
# exports the integration env + migrates, then runs the daemon test binary
# FILTERED to the lifecycle test under the gate and greps its run-proof marker,
# so the "boot→drain is leak-clean" claim can never be vacuous. Exporting the DB
# env to the FULL suite instead would un-skip every DB/Redis integration test
# under a 10–30× valgrind slowdown, so the -Dtest-filter narrowing is deliberate.
_memleak-boot-drain: _ensure-test-infra
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@db_url="$(TEST_DATABASE_URL_LOCAL)"; redis_url="$(TEST_REDIS_TLS_URL_LOCAL)"; ca="$(TEST_REDIS_TLS_CA_CERT)"; \
	filter="daemon boot -> SIGTERM -> drain"; marker="SERVE_LIFECYCLE_BOOT_DRAIN_RAN"; \
	echo "→ [boot-drain] Migrating the test database..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" DATABASE_URL_MIGRATOR="$$db_url" zig build run -- migrate || exit 1; \
	case "$$(uname -s)" in \
	  Linux) \
	    command -v valgrind >/dev/null 2>&1 || { echo "✗ valgrind is required on Linux for make memleak"; exit 1; }; \
	    opt="-Doptimize=ReleaseSafe -Dopenssl=false"; \
	    runner="valgrind --quiet --leak-check=full --show-leak-kinds=all --errors-for-leak-kinds=definite,possible --undef-value-errors=no --error-exitcode=1";; \
	  *) opt="-Dopenssl=false"; runner="";; \
	esac; \
	echo "→ [boot-drain] Building the lifecycle test binary (filtered)..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" zig build test-bin $$opt -Dtest-filter="$$filter" $(if $(MEMLEAK_CPU),-Dcpu=$(MEMLEAK_CPU),) || exit 1; \
	echo "→ [boot-drain] Running the lifecycle test under the leak gate (live pg + TLS redis)..."; \
	out=$$(TEST_DATABASE_URL="$$db_url" TEST_REDIS_TLS_URL="$$redis_url" REDIS_TLS_CA_CERT_FILE="$$ca" $$runner zig-out/bin/agentsfleetd-tests 2>&1) || { echo "$$out"; echo "✗ [boot-drain] leak gate failed"; exit 1; }; \
	echo "$$out" | grep -q "$$marker" || { echo "$$out"; echo "✗ [boot-drain] lifecycle test did NOT run (skipped — infra env misconfigured); the leak claim would be vacuous"; exit 1; }; \
	echo "✓ [boot-drain] boot→SIGTERM→drain ran leak-clean under the gate"

bench:  ## Run Tier-1 zbench micro + Tier-2 hey HTTP loadgen.
	@$(MAKE) _bench-micro
	@$(MAKE) _bench-loadgen

bench-redis:  ## Redis XADD concurrency bench (skip-by-default unless BENCH_REDIS=1; needs local Redis).
	@mkdir -p .tmp "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@if [ -z "$$BENCH_REDIS" ]; then \
	  echo "→ [agentsfleetd] bench-redis skipped — set BENCH_REDIS=1 against a live Redis (override REDIS_URL to point elsewhere)."; \
	  exit 0; \
	fi
	@echo "→ [agentsfleetd] bench-redis: 8 producer threads against $${REDIS_URL:-redis://localhost:6379} (ReleaseFast)..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 BENCH_REDIS="$$BENCH_REDIS" REDIS_URL="$$REDIS_URL" \
	 zig build -Dwith-bench-tools=true -Doptimize=ReleaseFast bench-redis
	@echo "✓ [agentsfleetd] bench-redis done"

_bench-micro:  ## Internal: zbench-backed code micro-benchmarks (Tier-1).
	@mkdir -p .tmp "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@echo "→ [agentsfleetd] Tier-1: running zbench micro-benchmarks (ReleaseFast)..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build -Dwith-bench-tools=true -Doptimize=ReleaseFast bench-micro
	@echo "✓ [agentsfleetd] Tier-1 zbench passed"

_bench-loadgen:  ## Internal: hey-backed HTTP loadgen gate (Tier-2).
	@mkdir -p .tmp
	@command -v hey >/dev/null 2>&1 || { \
	  echo "✗ hey is required for make bench. Install via:"; \
	  echo "    mise use -g 'ubi:rakyll/hey@latest'"; \
	  echo "  or:"; \
	  echo "    go install github.com/rakyll/hey@latest"; \
	  exit 1; \
	}
	@set -e; \
	 URL="$${API_BENCH_URL:-http://127.0.0.1:3000/healthz}"; \
	 curl -fsS --max-time 3 "$$URL" >/dev/null 2>&1 || { \
	   echo "✗ No live server at $$URL — Tier-2 bench needs a running API."; \
	   echo "  Start it first:  FOLLOW_LOGS=0 make up"; \
	   echo "  Or point bench at dev: API_BENCH_URL=https://api-dev.agentsfleet.net/healthz make bench"; \
	   exit 1; \
	 }; \
	 METHOD="$${API_BENCH_METHOD:-GET}"; \
	 DURATION="$${API_BENCH_DURATION_SEC:-20}"; \
	 CONC="$${API_BENCH_CONCURRENCY:-20}"; \
	 TIMEOUT_MS="$${API_BENCH_TIMEOUT_MS:-5000}"; \
	 MAX_ERR_RATE="$${API_BENCH_MAX_ERROR_RATE:-0.01}"; \
	 MAX_P95_MS="$${API_BENCH_MAX_P95_MS:-150}"; \
	 TIMEOUT_SEC=$$(( (TIMEOUT_MS + 999) / 1000 )); \
	 ARTIFACT=".tmp/api-bench-$$(date +%s).csv"; \
	 echo "→ [agentsfleetd] Tier-2: hey -m $$METHOD -z $${DURATION}s -c $$CONC -t $$TIMEOUT_SEC $$URL"; \
	 hey -m "$$METHOD" -z "$${DURATION}s" -c "$$CONC" -t "$$TIMEOUT_SEC" -o csv "$$URL" > "$$ARTIFACT" || { echo "✗ hey exited non-zero"; exit 1; }; \
	 TOTAL=$$(tail -n +2 "$$ARTIFACT" | wc -l | awk '{print $$1}'); \
	 [ "$$TOTAL" -gt 0 ] || { echo "✗ hey produced zero samples"; exit 1; }; \
	 ERR=$$(tail -n +2 "$$ARTIFACT" | awk -F, '{s=$$7+0; if (s<200||s>=300) c++} END{print c+0}'); \
	 ERR_RATE=$$(awk -v e=$$ERR -v t=$$TOTAL 'BEGIN{printf "%.6f", e/t}'); \
	 SORTED=".tmp/api-bench-sorted-$$$$.txt"; \
	 trap 'rm -f "$$SORTED"' EXIT; \
	 tail -n +2 "$$ARTIFACT" | awk -F, '{print $$1}' | sort -n > "$$SORTED"; \
	 P50_S=$$(awk -v t=$$TOTAL 'NR==int(t*0.50){print; exit}' "$$SORTED"); \
	 P95_S=$$(awk -v t=$$TOTAL 'NR==int(t*0.95){print; exit}' "$$SORTED"); \
	 P99_S=$$(awk -v t=$$TOTAL 'NR==int(t*0.99){print; exit}' "$$SORTED"); \
	 P50_MS=$$(awk -v v=$$P50_S 'BEGIN{printf "%.2f", v*1000}'); \
	 P95_MS=$$(awk -v v=$$P95_S 'BEGIN{printf "%.2f", v*1000}'); \
	 P99_MS=$$(awk -v v=$$P99_S 'BEGIN{printf "%.2f", v*1000}'); \
	 RPS=$$(awk -v t=$$TOTAL -v d=$$DURATION 'BEGIN{printf "%.2f", t/d}'); \
	 echo "total=$$TOTAL ok=$$((TOTAL-ERR)) fail=$$ERR error_rate=$$ERR_RATE req_per_sec=$$RPS"; \
	 echo "latency_ms p50=$$P50_MS p95=$$P95_MS p99=$$P99_MS"; \
	 echo "artifact=$$ARTIFACT"; \
	 awk -v er=$$ERR_RATE -v max=$$MAX_ERR_RATE 'BEGIN{if (er+0 > max+0) {print "✗ error rate " er " exceeds gate " max; exit 1}}'; \
	 awk -v p=$$P95_MS -v max=$$MAX_P95_MS 'BEGIN{if (p+0 > max+0) {print "✗ p95 " p "ms exceeds gate " max "ms"; exit 1}}'; \
	 echo "✓ [agentsfleetd] Tier-2 hey loadgen passed"

_ensure-test-bin:
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-bin $(if $(TARGET),-Dtarget=$(TARGET),) $(if $(OPTIMIZE),-Doptimize=$(OPTIMIZE),) $(if $(MEMLEAK_CPU),-Dcpu=$(MEMLEAK_CPU),) $(EXTRA_BUILD_FLAGS)
