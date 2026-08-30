# =============================================================================
# BENCH — API benchmarks.
#
# `make bench` runs two tiers:
#   Tier-1  zbench micro-benchmarks   (tests/bench/micro.zig — ReleaseFast)
#   Tier-2  hey HTTP loadgen          (requires `hey` in PATH — mise installs it)
# =============================================================================

.PHONY: bench bench-redis bench-incident _bench-micro _bench-loadgen

bench:  ## Run Tier-1 zbench micro + Tier-2 hey HTTP loadgen.
	@$(MAKE) _bench-micro
	@$(MAKE) _bench-loadgen

# ── Incident-response benchmark ──────────────────────────────────────────────
# SEED_MANIFEST selects the manifest half (eval is the scored set; the spelled
# out file name also works). BENCH_RUNS optionally points at a findings file —
# without it the target proves harness health + prints the corpus hash line
# that the reproducibility rubric compares across runs.
BENCH_INCIDENT_DIR := bench/incident-response
SEED_MANIFEST ?= eval
ifeq ($(SEED_MANIFEST),eval)
BENCH_INCIDENT_MANIFEST := $(BENCH_INCIDENT_DIR)/seeds/evaluation.json
else
BENCH_INCIDENT_MANIFEST := $(BENCH_INCIDENT_DIR)/seeds/$(SEED_MANIFEST).json
endif

bench-incident:  ## Incident-response benchmark: harness tests, corpus hash, scoring (SEED_MANIFEST=eval, BENCH_RUNS=<findings.json>)
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@echo "→ [bench-incident] harness unit tests..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build -Dwith-bench-tools=true bench-incident-test
	@echo "→ [bench-incident] corpus + score ($(SEED_MANIFEST))..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build -Dwith-bench-tools=true bench-incident -- \
	   --evaluation $(BENCH_INCIDENT_MANIFEST) \
	   --calibration $(BENCH_INCIDENT_DIR)/seeds/calibration.json \
	   --baseline $(BENCH_INCIDENT_DIR)/baseline.json \
	   --freeze $(BENCH_INCIDENT_DIR)/freeze.json \
	   $(if $(BENCH_RUNS),--runs $(BENCH_RUNS),)
	@echo "✓ [bench-incident] passed"

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

# ── Cutover benchmark ────────────────────────────────────────────────────────
# The lane the swap decision reads: is the candidate daemon fast enough, and
# small enough, to replace the one serving now.
#
# THE BUDGETS ARE DECLARED HERE AND DEFAULT TO NOTHING, deliberately.
#
# `BENCH_P95_TOLERANCE_PCT` is how much slower the candidate may be at the 95th
# percentile; `BENCH_RSS_CEILING_MB` is its resident-set ceiling. Neither has a
# value yet because neither has been measured yet — the Rust daemon has not run
# under load beside the Zig one, and a number written before the measurement is
# the judgment this row exists to replace (RULE TIM). `scripts/bench_cutover.sh`
# refuses to run with either empty and names the one it is missing, so the lane
# fails loudly rather than measuring, printing, and returning success.
#
# The milestone that performs the swap sets them from a recorded baseline.
BENCH_P95_TOLERANCE_PCT ?=
BENCH_RSS_CEILING_MB ?=

.PHONY: bench-cutover bench-cutover-self-test

# LOCAL=1 stands the stack up and points the lane at it, so the whole thing is
# one command. Without it the lane measures whatever BASE_URL names, which is
# how it runs against a deployment.
bench-cutover: $(if $(LOCAL),_ensure-local-daemon,)  ## Cutover benchmark (BASE_URL=<url> [COMPARE_URL=<url>] | LOCAL=1)
	@BASE_URL="$(or $(BASE_URL),$(if $(LOCAL),$(LOCAL_DAEMON_URL),))" \
	 COMPARE_URL="$(COMPARE_URL)" \
	 BENCH_RSS_CONTAINER="$(or $(BENCH_RSS_CONTAINER),$(if $(LOCAL),$(LOCAL_DAEMON_CONTAINER),))" \
	 BENCH_P95_TOLERANCE_PCT="$(BENCH_P95_TOLERANCE_PCT)" \
	 BENCH_RSS_CEILING_MB="$(BENCH_RSS_CEILING_MB)" \
	 bash scripts/bench_cutover.sh

# The lane's own tests. Fixture load generator, fixture resident set, no daemon
# — so it rides `lint-all` and proves the half that decides: that a missing
# budget is refused by name, and that a measurement past one fails.
bench-cutover-self-test:  ## Run scripts/bench_cutover_test.sh — the cutover benchmark's own tests
	@echo "→ [bench] Running cutover benchmark self-tests..."
	@bash scripts/bench_cutover_test.sh
	@echo "✓ [bench] Cutover benchmark self-tests passed"
