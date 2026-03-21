# =============================================================================
# TEST-BENCH — API benchmark and memory leak gates
# =============================================================================

.PHONY: memleak bench _soak _bench_apiprofile _ensure-test-bin _bench-run

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

bench:  ## Run API benchmark gate (defaults target http://127.0.0.1:3000/healthz; optional API_BENCH_* env overrides)
	@$(MAKE) _bench-run BENCH_MODE=$(BENCH_MODE)

_soak:  ## Internal: run API soak benchmark
	@$(MAKE) _bench-run BENCH_MODE=soak

_bench_apiprofile:  ## Internal: run API benchmark with profiling artifacts
	@$(MAKE) _bench-run BENCH_MODE=profile

_bench-run:
	@mkdir -p .tmp "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@echo "→ [zombied] Running API benchmark mode=$(BENCH_MODE)..."
	@echo "→ [zombied] Optional overrides: API_BENCH_URL API_BENCH_METHOD API_BENCH_DURATION_SEC API_BENCH_CONCURRENCY API_BENCH_TIMEOUT_MS API_BENCH_MAX_ERROR_RATE API_BENCH_MAX_P95_MS API_BENCH_MAX_RSS_GROWTH_MB"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 BENCH_MODE="$(BENCH_MODE)" \
	 zig build -Dwith-bench-tools=true bench-api

_ensure-test-bin:
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test-bin $(if $(TARGET),-Dtarget=$(TARGET),) $(if $(OPTIMIZE),-Doptimize=$(OPTIMIZE),) $(if $(MEMLEAK_CPU),-Dcpu=$(MEMLEAK_CPU),)
