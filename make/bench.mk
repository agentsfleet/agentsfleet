# =============================================================================
# BENCH — API benchmarks.
#
# `make bench` is the hey HTTP loadgen gate (requires `hey` in PATH — mise
# installs it). It measures a URL, which is the only thing hey can measure.
#
# What the daemon's paths cost is the THROUGHPUT LANES further down —
# bench-lease, bench-steer, bench-outbound, bench-cardinality. Those are Rust,
# they drive the production types directly rather than a URL, and they are the
# agentsfleetd benchmarks.
#
# The Tier-1/Tier-2 pair this header used to describe is gone. Tier-1 was a
# zbench runner over `tests/bench/micro.zig`, which benchmarked daemon
# internals — the router, the error registry, the credential broker — through a
# `bench_app` module the Zig daemon graph provided. That tree went at the
# cutover, so the file could not compile, no build step named it, and `make
# bench` never ran it. It is deleted rather than described.
# =============================================================================

.PHONY: bench _bench-loadgen

bench:  ## Run the Tier-2 hey HTTP loadgen gate.
	@$(MAKE) _bench-loadgen


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

# ── Throughput lanes ─────────────────────────────────────────────────────────
# What the steer, lease, delivery and cardinality paths sustain, and which
# datastore gives out first. Distinct from the loadgen above in the one way
# that matters: `hey` measures a URL, and only the steer path IS one — these
# drive the production types directly, which is the only way to attribute a
# cost between Redis and Postgres.
#
# PROFILE decides scale, target and blast radius (rig | dev | prod), and the
# lane refuses a parameter above its ceiling before it opens a connection.
# The rig IS the compose stack the integration lane already owns, so these
# targets depend on the same bring-up rather than standing up a second one.
#
# The BENCH_* datastore variables are set FROM the lane's TEST_* values here
# rather than read as TEST_* in Rust: the bench code names one spelling, and
# this file is where a deployed target would be substituted for it.
#
# `--manifest-path` rather than `cd $(RUSTD_DIR)`: a lane writes its result to
# `bench/results/` relative to its working directory, and that directory is the
# REPOSITORY root beside `bench/baselines/`, not the Rust workspace inside it.
PROFILE ?= rig

# The one environment every lane binary reads, and the one place a deployed
# target would be substituted for the compose stack.
BENCH_LANE_ENV := BENCH_PROFILE="$(PROFILE)" \
	BENCH_DATABASE_URL="$(TEST_DATABASE_URL)" \
	BENCH_REDIS_URL="$(TEST_REDIS_URL)" \
	BENCH_REDIS_CA_CERT="$(TEST_REDIS_CA_CERT)"

# `--release` is not a detail. A debug build measures rustc's unoptimised
# output, which is the wrong system: the number would be a property of the
# build profile rather than of the path.
BENCH_LANE_RUN := cargo run --release --quiet --manifest-path $(RUSTD_DIR)/Cargo.toml --bin

.PHONY: bench-steer bench-lease bench-outbound bench-cardinality bench-compare

bench-lease: _ensure-test-infra  ## Lease throughput: rate, p95, round trips per lease (PROFILE=rig [BENCH_FLEETS=n] [BENCH_RUNNERS=n])
	@echo "→ [bench-lease] profile=$(PROFILE)"
	@$(BENCH_LANE_ENV) $(BENCH_LANE_RUN) lease

bench-steer: _ensure-test-infra  ## Steer ingress: accepted rate, p95, readiness depth (PROFILE=rig [BENCH_FLEETS=n] [BENCH_CONCURRENCY=n])
	@echo "→ [bench-steer] profile=$(PROFILE)"
	@$(BENCH_LANE_ENV) $(BENCH_LANE_RUN) steer

bench-outbound: _ensure-test-infra  ## Delivery ceiling: rate, p95, head-of-line and retry cost (PROFILE=rig [BENCH_JOBS=n] [BENCH_SLOW_FRACTION=0..1])
	@echo "→ [bench-outbound] profile=$(PROFILE)"
	@$(BENCH_LANE_ENV) $(BENCH_LANE_RUN) outbound

bench-cardinality: _ensure-test-infra  ## Cost per idle fleet up a ladder (PROFILE=rig [BENCH_FLEETS=n], rig cap 1000000)
	@echo "→ [bench-cardinality] profile=$(PROFILE)"
	@$(BENCH_LANE_ENV) $(BENCH_LANE_RUN) cardinality

bench-compare:  ## Delta between a result and its baseline (LANE=lease PROFILE=rig) — always exit 0
	@$(BENCH_LANE_RUN) compare -- "$(LANE)" "$(PROFILE)"
