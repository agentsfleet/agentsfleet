# `bench-http` loads a URL with `hey`. Every other lane is Rust that drives the
# production types directly, the only way to split a cost between the bus and
# Postgres.

.PHONY: bench-http

bench-http:  ## HTTP p95 and error rate under hey load against API_BENCH_URL (default: this worktree's /healthz). Run when you change the HTTP stack
	@mkdir -p .tmp
	@command -v hey >/dev/null 2>&1 || { \
	  echo "✗ hey is required for make bench-http. Install via:"; \
	  echo "    mise use -g 'ubi:rakyll/hey@latest'"; \
	  echo "  or:"; \
	  echo "    go install github.com/rakyll/hey@latest"; \
	  exit 1; \
	}
	@set -e; \
	 URL="$${API_BENCH_URL:-$(LOCAL_DAEMON_URL)/healthz}"; \
	 curl -fsS --max-time 3 "$$URL" >/dev/null 2>&1 || { \
	   echo "✗ No live server at $$URL — Tier-2 bench needs a running API."; \
	   echo "  Start it first:  FOLLOW_LOGS=0 make up"; \
	   echo "  Or point bench at dev: API_BENCH_URL=https://api-dev.agentsfleet.net/healthz make bench-http"; \
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

# PROFILE (rig | dev | prod) sets scale, target and blast radius; a lane refuses
# a parameter above its profile's cap before it opens a connection.
PROFILE ?= rig

# Provenance names the bus build from the one file that pins it.
DATASTORE_IMAGE := $(shell awk '/image: nats:/ { print $$2; exit }' docker-compose.yml)

# Only the exact word `owned` entitles a run to reset the rig and inject faults.
BENCH_TARGET_OWNED ?= owned

# A lane refuses to run without its provenance. Deferred (`=`): make/build.mk
# defines GIT_COMMIT after this file is included, and an empty revision would
# refuse every run.
BENCH_LANE_ENV = BENCH_PROFILE="$(PROFILE)" \
	BENCH_DATABASE_URL="$(TEST_DATABASE_URL)" \
	BENCH_NATS_URL="$(TEST_NATS_URL)" \
	BENCH_REVISION="$(GIT_COMMIT)" \
	BENCH_DATASTORE_IMAGE="$(DATASTORE_IMAGE)" \
	BENCH_TARGET_OWNED="$(BENCH_TARGET_OWNED)"

# `--release`: a debug build measures rustc's unoptimised output, not the path.
# `--manifest-path`, not `cd`: a lane writes bench/results/ relative to the
# repository root, beside bench/baselines/.
BENCH_LANE_RUN := cargo run --release --locked --quiet --manifest-path $(RUSTD_DIR)/Cargo.toml --bin

BENCH_LANES := bench-runs bench-recovery bench-intake bench-delivery bench-idle bench-stream

.PHONY: $(BENCH_LANES) bench-all bench-compare

# An owned rig resets and migrates before each lane: the integration suite leaves
# unsettled turns a run would meet as candidates. A dev or prod target is never
# reset. `filter` alone would accept "owned no", hence the word count.
BENCH_RIG_OWNED := $(and $(filter owned,$(BENCH_TARGET_OWNED)),$(filter 1,$(words $(BENCH_TARGET_OWNED))))
BENCH_MIGRATE := $(if $(and $(filter rig,$(PROFILE)),$(BENCH_RIG_OWNED)),$(TEST_STATE_DEP) _migrate-test-db)

# saturation exhausts the pool on purpose, so it refuses any profile but the rig.
BENCH_SHAPE ?= default

bench-runs:  ## Runs per second, p95, statements and commits per run (BENCH_SHAPE=default|saturation|burst). Run when you change assign, renew or report SQL; thread, turn or run tables; the gateway
bench-recovery:  ## Recovery burst through the full daemon and runner processes (BENCH_FLEETS=1|10|100|500). Run when you change checkpoints, respawn, run expiry
bench-intake:  ## Accepted rate, p95 and readiness depth of intake. Run when you change message and webhook intake, the turn insert, wake publish
bench-delivery:  ## Delivery rate, p95, head-of-line and retry cost. Run when you change deliveries to Slack or webhooks
bench-idle:  ## Cost per idle fleet up a ladder. Run when you change anything that runs per fleet on a timer: sweeps, heartbeats, indexes
bench-stream:  ## Allocations and p95 per frame, heap per stream; refuses an unbacked SSE_MAX_STREAMS_DEFAULT. Run when you change the live stream (SSE) and the NATS items path

bench-runs: BENCH_LANE_SHAPE = BENCH_SHAPE="$(BENCH_SHAPE)"
# Boots the full daemon and runner processes, so it is af_e2e's bench, not an
# afd_bench binary.
bench-recovery: BENCH_LANE_RUN = cargo bench --profile dist --locked --manifest-path $(RUSTD_DIR)/Cargo.toml -p af_e2e --bench

# The target names the binary after `bench-`.
$(BENCH_LANES): bench-%: _ensure-test-infra $(BENCH_MIGRATE)
	@echo "→ [$@] profile=$(PROFILE)$(if $(BENCH_LANE_SHAPE), shape=$(BENCH_SHAPE))"
	@$(BENCH_LANE_SHAPE) $(BENCH_LANE_ENV) $(BENCH_LANE_RUN) $*

# One lane at a time, each on a fresh rig; `make -j` cannot overlap them.
bench-all:  ## Every lane above except bench-http, in turn, stopping at the first failure. Run when you make a large refactor
	@for lane in $(BENCH_LANES); do $(MAKE) --no-print-directory $$lane || exit 1; done

bench-compare:  ## Delta between a result and its baseline (LANE=runs PROFILE=rig) — always exit 0
	@$(BENCH_LANE_RUN) compare -- "$(LANE)" "$(PROFILE)"
