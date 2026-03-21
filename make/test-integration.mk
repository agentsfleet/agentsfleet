# =============================================================================
# TEST-INTEGRATION — all integration tests (Zig in-process, DB, Redis)
# =============================================================================

.PHONY: test-integration _test-integration-zombied _test-integration-db _test-integration-redis

_test-integration-zombied:
	@echo "→ [zombied] Running Zig integration tests..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test -- --test-filter "integration:"

_test-integration-db:
	@if [ -z "$$HANDLER_DB_TEST_URL" ]; then \
	  echo "✗ HANDLER_DB_TEST_URL is not set — DB integration tests cannot run"; \
	  echo "  Set it to a live Postgres URL, e.g.:"; \
	  echo "    export HANDLER_DB_TEST_URL='postgres://usezombie:usezombie@localhost:5432/usezombiedb?sslmode=disable'"; \
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

_test-integration-redis:
	@if [ -z "$$REDIS_TLS_TEST_URL" ]; then \
	  echo "✗ REDIS_TLS_TEST_URL is not set — Redis integration tests cannot run"; \
	  echo "  Set it to a live Redis TLS URL, e.g.:"; \
	  echo "    export REDIS_TLS_TEST_URL=rediss://:usezombie@localhost:6379"; \
	  exit 1; \
	fi
	@echo "→ [zombied] Running Redis integration tests (REDIS_TLS_TEST_URL is set)..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 REDIS_TLS_TEST_URL="$$REDIS_TLS_TEST_URL" \
	 zig build test -- --test-filter "integration:"
	@echo "✓ [zombied] Redis integration tests passed"

test-integration: _test-integration-zombied _test-integration-db _test-integration-redis  ## Run all integration tests (Zig + DB + Redis)
	@echo "✓ [zombied] All integration tests passed"
