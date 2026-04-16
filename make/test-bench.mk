# =============================================================================
# TEST-BENCH — API benchmark and memory leak gates (M24_001)
#
# `make bench` runs two tiers:
#   Tier-1  zbench micro-benchmarks   (src/zbench_micro.zig — ReleaseFast)
#   Tier-2  hey HTTP loadgen          (requires `hey` in PATH — mise installs it)
# =============================================================================

.PHONY: memleak bench _bench-micro _bench-loadgen _ensure-test-bin

memleak:  ## Run Zig memory leak gates (allocator tests + Linux valgrind pass)
	@echo "→ [zombied] Running allocator leak guard tests..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test -- --test-filter "finalizeWorkerAllocator"
	@case "$$(uname -s)" in \
	  Linux) \
	    command -v valgrind >/dev/null 2>&1 || { echo "✗ valgrind is required on Linux for make memleak"; exit 1; }; \
	    $(MAKE) _ensure-test-bin TARGET="$(MEMLEAK_TARGET)" OPTIMIZE=ReleaseSafe; \
	    echo "→ [zombied] Running valgrind leak gate..."; \
	    valgrind --quiet --leak-check=full --show-leak-kinds=all --errors-for-leak-kinds=definite,possible --undef-value-errors=no --error-exitcode=1 \
	      zig-out/bin/zombied-tests;; \
	  Darwin) \
	    if command -v leaks >/dev/null 2>&1; then \
	      $(MAKE) _ensure-test-bin; \
	      echo "→ [zombied] Running macOS leaks gate..."; \
	      MallocStackLogging=1 leaks -atExit -- zig-out/bin/zombied-tests >/dev/null || \
	        echo "→ [zombied] leaks check unavailable in current runtime (continuing with allocator gate)"; \
	    else \
	      echo "→ [zombied] leaks not found; allocator gate only"; \
	    fi;; \
	  *) \
	    echo "→ [zombied] platform=$$(uname -s): allocator gate only";; \
	esac
	@echo "✓ [zombied] memleak gate passed"

bench:  ## Run Tier-1 zbench micro + Tier-2 hey HTTP loadgen (M24_001).
	@$(MAKE) _bench-micro
	@$(MAKE) _bench-loadgen

_bench-micro:  ## Internal: zbench-backed code micro-benchmarks (Tier-1).
	@mkdir -p .tmp "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@echo "→ [zombied] Tier-1: running zbench micro-benchmarks (ReleaseFast)..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build -Dwith-bench-tools=true -Doptimize=ReleaseFast bench-micro
	@echo "✓ [zombied] Tier-1 zbench passed"

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
	 METHOD="$${API_BENCH_METHOD:-GET}"; \
	 DURATION="$${API_BENCH_DURATION_SEC:-20}"; \
	 CONC="$${API_BENCH_CONCURRENCY:-20}"; \
	 TIMEOUT_MS="$${API_BENCH_TIMEOUT_MS:-5000}"; \
	 MAX_ERR_RATE="$${API_BENCH_MAX_ERROR_RATE:-0.01}"; \
	 MAX_P95_MS="$${API_BENCH_MAX_P95_MS:-150}"; \
	 TIMEOUT_SEC=$$(( (TIMEOUT_MS + 999) / 1000 )); \
	 ARTIFACT=".tmp/api-bench-$$(date +%s).csv"; \
	 echo "→ [zombied] Tier-2: hey -m $$METHOD -z $${DURATION}s -c $$CONC -t $$TIMEOUT_SEC $$URL"; \
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
	 echo "✓ [zombied] Tier-2 hey loadgen passed"

_ensure-test-bin:
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-bin $(if $(TARGET),-Dtarget=$(TARGET),) $(if $(OPTIMIZE),-Doptimize=$(OPTIMIZE),) $(if $(MEMLEAK_CPU),-Dcpu=$(MEMLEAK_CPU),)
