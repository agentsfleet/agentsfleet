# =============================================================================
# TEST-INTEGRATION — all integration tests (Zig in-process, DB, Redis)
# =============================================================================

.PHONY: test-integration test-integration-db test-integration-redis _test-integration-zombied _test-integration-db _test-integration-redis _test-integration-full _ensure-test-infra _reset-test-db
TEST_DATABASE_URL_LOCAL ?= postgres://usezombie:usezombie@localhost:5432/usezombiedb
TEST_REDIS_TLS_URL_LOCAL ?= rediss://:usezombie@localhost:6379
# Cert path — populated by _ensure-test-infra after Redis is healthy. Do NOT shell-expand
# at parse time; Redis may not be running yet when the Makefile is first evaluated.
TEST_REDIS_TLS_CA_CERT ?= $(CURDIR)/.tmp/redis-ca.crt

# Bring postgres + redis up via docker compose and wait for healthchecks to pass.
# Idempotent — if already healthy, docker compose up --wait is a no-op. Safe to call
# multiple times. Extracts the Redis TLS CA cert after the container is healthy so
# subsequent targets can rely on $(TEST_REDIS_TLS_CA_CERT) being present.
_ensure-test-infra:
	@if ! docker info >/dev/null 2>&1; then \
	  echo "✗ Docker daemon is not running — start Docker Desktop / dockerd and retry."; \
	  exit 1; \
	fi
	@# container_name in docker-compose.yml is fixed (zombie-postgres / zombie-redis),
	@# so another worktree's compose can leave stale containers blocking ours. Remove
	@# by name if they exist but are NOT owned by this project. Idempotent.
	@this_project=$$(basename "$(CURDIR)"); \
	for c in zombie-postgres zombie-redis; do \
	  owner=$$(docker inspect -f '{{ index .Config.Labels "com.docker.compose.project" }}' $$c 2>/dev/null); \
	  if [ -n "$$owner" ] && [ "$$owner" != "$$this_project" ]; then \
	    echo "→ [infra] Removing stale $$c (owned by project '$$owner')..."; \
	    docker rm -f $$c >/dev/null; \
	  fi; \
	done
	@echo "→ [infra] Starting postgres + redis (waiting for healthchecks)..."
	@docker compose up -d --wait postgres redis
	@mkdir -p "$(CURDIR)/.tmp"
	@echo "→ [infra] Extracting Redis TLS CA cert..."
	@docker compose cp redis:/tls/server.crt "$(TEST_REDIS_TLS_CA_CERT)" >/dev/null
	@test -s "$(TEST_REDIS_TLS_CA_CERT)" || { echo "✗ Failed to extract Redis TLS cert"; exit 1; }
	@echo "✓ [infra] postgres + redis ready; Redis CA cert at $(TEST_REDIS_TLS_CA_CERT)"

# Drop and recreate all app schemas so every test-integration run starts from a clean
# state. Needed because several tests in the suite (rbac, byok, event_loop) leave
# fixture rows behind (paused zombies, lingering secrets) that break subsequent runs.
# Uses the same teardown.sql as the PlanetScale playbook for consistency.
_reset-test-db: _ensure-test-infra
	@echo "→ [infra] Resetting test database schemas to a clean state..."
	@docker compose cp playbooks/011_database_teardown/teardown.sql postgres:/tmp/teardown.sql >/dev/null
	@out=$$(docker compose exec -T postgres psql -U usezombie -d usezombiedb -v ON_ERROR_STOP=1 -q -f /tmp/teardown.sql 2>&1) || { echo "✗ [infra] teardown.sql failed"; echo "$$out"; exit 1; }; echo "$$out" | grep -v "^NOTICE:" | grep -v "^psql:" || true
	@docker compose exec -T postgres rm -f /tmp/teardown.sql >/dev/null
	@echo "✓ [infra] Schemas dropped; migrations will rebuild on next step"

_test-integration-zombied:
	@echo "→ [zombied] Running Zig integration tests..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@env -u TEST_DATABASE_URL -u TEST_REDIS_TLS_URL -u LIVE_DB \
	 ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build test

_test-integration-db: _reset-test-db
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

_test-integration-redis: _reset-test-db
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
	  REDIS_URL_API="$$redis_tls_test_url" \
	  REDIS_TLS_CA_CERT_FILE="$(TEST_REDIS_TLS_CA_CERT)" \
	  zig build test
	@echo "✓ [zombied] Redis integration tests passed"

_test-integration-full: _reset-test-db
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
	REDIS_URL_API="$$redis_tls_test_url" \
	REDIS_TLS_CA_CERT_FILE="$(TEST_REDIS_TLS_CA_CERT)" \
	zig build test && \
	echo "→ [zombied] Running executor-side unit tests (redactor contract, runner_progress lifecycle)..." && \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	zig build test-executor --summary all 2>&1 | tee /dev/stderr | grep -q "passed"
	@echo "✓ [zombied] Full integration suite passed"

test-integration-db: _test-integration-db  ## Run real DB-backed integration suite only

test-integration-redis: _test-integration-redis  ## Run Redis-backed integration suite only

test-integration: _test-integration-full  ## Run all integration tests once against real DB + Redis
	@echo "✓ [zombied] All integration tests passed"
