# =============================================================================
# TEST-INTEGRATION — all integration tests (Zig in-process, DB, Redis)
# =============================================================================

.PHONY: test-integration test-integration-db test-integration-redis _test-integration-zombied _test-integration-db _test-integration-redis _test-integration-full
TEST_DATABASE_URL_LOCAL ?= postgres://usezombie:usezombie@localhost:5432/usezombiedb
TEST_REDIS_TLS_URL_LOCAL ?= rediss://:usezombie@localhost:6379
# Self-signed cert from docker compose Redis — extracted once before tests.
TEST_REDIS_TLS_CA_CERT  ?= $(shell mkdir -p .tmp && docker compose cp redis:/tls/server.crt .tmp/redis-ca.crt 2>/dev/null && echo "$$(pwd)/.tmp/redis-ca.crt")

_test-integration-zombied:
	@echo "→ [zombied] Running Zig integration tests..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@env -u TEST_DATABASE_URL -u TEST_REDIS_TLS_URL -u LIVE_DB \
	 ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test

_test-integration-db:
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
	echo "→ [zombied] Running DB-backed integration tests using $$db_url..."; \
	mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"; \
	echo "→ [zombied] Auto-migrating test database..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	DATABASE_URL_MIGRATOR="$$db_url" \
	zig build run -- migrate; \
	echo "→ [zombied] Migration done, running tests..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	LIVE_DB=1 \
	TEST_DATABASE_URL="$$db_url" \
	zig build test
	@echo "✓ [zombied] DB-backed integration tests passed"

_test-integration-redis:
	@redis_tls_test_url="$$TEST_REDIS_TLS_URL"; \
	if [ -z "$$redis_tls_test_url" ] && [ -n "$$REDIS_URL" ]; then \
	  case "$$REDIS_URL" in \
	    rediss://*) redis_tls_test_url="$$REDIS_URL" ;; \
	  esac; \
	fi; \
	if [ -z "$$redis_tls_test_url" ]; then redis_tls_test_url="$(TEST_REDIS_TLS_URL_LOCAL)"; fi; \
	echo "→ [zombied] Running Redis integration tests using $$redis_tls_test_url..."; \
	mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"; \
	env -u TEST_DATABASE_URL -u LIVE_DB \
	  ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	  ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	  TEST_REDIS_TLS_URL="$$redis_tls_test_url" \
	  REDIS_TLS_CA_CERT_FILE="$(TEST_REDIS_TLS_CA_CERT)" \
	  zig build test
	@echo "✓ [zombied] Redis integration tests passed"

_test-integration-full:
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
	redis_tls_test_url="$$TEST_REDIS_TLS_URL"; \
	if [ -z "$$redis_tls_test_url" ] && [ -n "$$REDIS_URL" ]; then \
	  case "$$REDIS_URL" in \
	    rediss://*) redis_tls_test_url="$$REDIS_URL" ;; \
	  esac; \
	fi; \
	if [ -z "$$redis_tls_test_url" ]; then redis_tls_test_url="$(TEST_REDIS_TLS_URL_LOCAL)"; fi; \
	echo "→ [zombied] Auto-migrating test database..."; \
	mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	DATABASE_URL_MIGRATOR="$$db_url" \
	zig build run -- migrate; \
	echo "→ [zombied] Running full integration suite against real DB + Redis..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	LIVE_DB=1 \
	TEST_DATABASE_URL="$$db_url" \
	TEST_REDIS_TLS_URL="$$redis_tls_test_url" \
	REDIS_TLS_CA_CERT_FILE="$(TEST_REDIS_TLS_CA_CERT)" \
	zig build test
	@echo "✓ [zombied] Full integration suite passed"

test-integration-db: _test-integration-db  ## Run real DB-backed integration suite only

test-integration-redis: _test-integration-redis  ## Run Redis-backed integration suite only

test-integration: _test-integration-full  ## Run all integration tests once against real DB + Redis
	@echo "✓ [zombied] All integration tests passed"
