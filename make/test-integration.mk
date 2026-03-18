# =============================================================================
# TEST-INTEGRATION — zombied in-process + DB-backed handler tests
# =============================================================================

.PHONY: test-integration test-integration-zombied test-integration-db

test-integration-zombied:  ## Run Zig integration tests (deterministic, no live service required)
	@echo "→ [zombied] Running Zig integration tests..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@redis_tls_test_url="$$REDIS_TLS_TEST_URL"; \
	 if [ -z "$$redis_tls_test_url" ] && [ -n "$$REDIS_URL" ]; then \
	   case "$$REDIS_URL" in \
	     rediss://*) redis_tls_test_url="$$REDIS_URL" ;; \
	   esac; \
	 fi; \
	 ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 REDIS_TLS_TEST_URL="$$redis_tls_test_url" \
	 zig build test -- --test-filter "integration:"

test-integration-db:  ## Run DB-backed handler integration tests (requires HANDLER_DB_TEST_URL)
	@if [ -z "$$HANDLER_DB_TEST_URL" ]; then \
	  echo "✗ HANDLER_DB_TEST_URL is not set — DB integration tests cannot run"; \
	  echo "  Set it to a live Postgres URL, e.g.:"; \
	  echo "    export HANDLER_DB_TEST_URL=postgres://usezombie:usezombie@localhost:5432/usezombiedb"; \
	  exit 1; \
	fi
	@echo "→ [zombied] Running DB-backed integration tests (HANDLER_DB_TEST_URL is set)..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@echo "→ [zombied] Auto-migrating test database..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 DATABASE_URL_API="$$HANDLER_DB_TEST_URL" \
	 zig build run -- migrate
	@echo "→ [zombied] Migration done, running tests..."
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 HANDLER_DB_TEST_URL="$$HANDLER_DB_TEST_URL" \
	 zig build test -- --test-filter "integration:"
	@echo "✓ [zombied] DB-backed integration tests passed"

test-integration: test-integration-zombied  ## Run integration checks (no DB required)
	@echo "✓ [zombied] Integration tests passed"
