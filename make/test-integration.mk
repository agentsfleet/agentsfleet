# =============================================================================
# TEST-INTEGRATION — all integration tests (Zig in-process, DB, Redis)
# =============================================================================

.PHONY: test-integration test-integration-db test-integration-redis test-integration-kernel _test-integration-agentsfleetd _test-integration-db _test-integration-redis _test-integration-full _ensure-test-infra _reset-test-db

# The runner's own real-process integration lane (build_runner.zig, no datastore):
# it forks real children and asserts real KERNEL behaviour — the env allowlist +
# kill(-pgid) tree reap + CLOEXEC proofs AND the security-enforcement proofs
# (seccomp trap / Landlock deny / cgroup pids+OOM cage). Linux-only (bodies
# SkipZigTest off-Linux), a distinct execution environment from the Postgres/Redis
# app lane below and the fast unit lane.
#
# Delegation discipline: the cgroup-cage proofs need a delegated cgroup-v2
# controller subtree. That delegation (scripts/cgroup-delegate.sh) is a
# DISPOSABLE-ENVIRONMENT concern — it drains the root cgroup + writes
# subtree_control, which must NEVER touch a developer's host. So it runs ONLY
# inside the macOS throwaway container below (and the privileged CI step). A bare
# `make test-integration-kernel` on a Linux host runs the lane WITHOUT delegating;
# the cgroup proofs then SkipZigTest (requireCgroupDelegation) — no host mutation,
# no false green. In production the runner's cgroup subtree is delegated by the
# init system (systemd Delegate=) / container runtime; this script is never deployed.
RUNNER_CI_IMAGE ?= ghcr.io/agentsfleet/ci-zig-alpine:0.16.0

test-integration-kernel:  ## Run the runner's real-process kernel integration tests (env/kill-tree + seccomp/Landlock/cgroup); native on Linux, auto-containerized on macOS
ifeq ($(shell uname),Darwin)
	@echo "→ [kernel] macOS host has no Linux kernel — running the lane in a disposable privileged Linux container..."
	@docker run --rm --privileged --cgroupns=private --platform "linux/$(shell uname -m | sed 's/x86_64/amd64/')" \
	  -v "$(CURDIR)":"$(CURDIR)" -w "$(CURDIR)" \
	  "$(RUNNER_CI_IMAGE)" sh -c 'sh scripts/cgroup-delegate.sh && make test-integration-kernel'
else
	@echo "→ [kernel] Running runner integration tests via build_runner.zig (env filter + kill-tree + seccomp/Landlock/cgroup)..."
	@mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"
	@ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	 ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	 zig build --build-file build_runner.zig test-integration --summary all
	@echo "✓ [kernel] Runner integration tests passed (Linux real-process proofs)"
endif

# sslmode=disable: the local docker Postgres has no TLS and parseUrl defaults to
# `.require` (hosted providers mandate it) — without it every local DB-lane test
# fails at connect with SSLNotSupportedByServer before it can run.
TEST_DATABASE_URL_LOCAL ?= postgres://agentsfleet:agentsfleet@localhost:5432/agentsfleetdb?sslmode=disable
TEST_REDIS_TLS_URL_LOCAL ?= rediss://:agentsfleet@localhost:6379
# Cert path — populated by _ensure-test-infra after Redis is healthy. Do NOT shell-expand
# at parse time; Redis may not be running yet when the Makefile is first evaluated.
TEST_REDIS_TLS_CA_CERT ?= $(CURDIR)/.tmp/redis-ca.crt
# QStash local dev server (docker-compose `qstash` service). The emulator ships a
# hardcoded local identity and rejects anything else (a different user 404s, a
# different password 401s), so this is a fixture we reproduce, not a credential we
# choose — and nothing it authenticates to holds real data. Derived here from its
# two plain parts so no credential-shaped blob is stored in the repo.
# The opt-in live QStash tests read these vars; unset (or server down) → self-skip.
QSTASH_DEV_URL_LOCAL ?= http://localhost:8080
QSTASH_DEV_IDENTITY ?= defaultUser
QSTASH_DEV_SECRET ?= defaultPassword
QSTASH_DEV_TOKEN_LOCAL ?= $(shell printf '{"UserID":"%s","Password":"%s"}' '$(QSTASH_DEV_IDENTITY)' '$(QSTASH_DEV_SECRET)' | base64 | tr -d '\n')

# Bring postgres + redis up via docker compose and wait for healthchecks to pass.
# Idempotent — if already healthy, docker compose up --wait is a no-op. Safe to call
# multiple times. Extracts the Redis TLS CA cert after the container is healthy so
# subsequent targets can rely on $(TEST_REDIS_TLS_CA_CERT) being present.
#
# TEST_INFRA=provided — the caller already booted postgres/redis and extracted the
# CA cert by running THIS recipe in an environment that has docker (CI: the memleak
# workflow runs it on the host, then the valgrind container — which carries no
# docker CLI — runs the gate with the flag). Fail-closed: the flag never bypasses
# the cert check, so a caller that claims infra without providing it dies loudly.
_ensure-test-infra:
ifeq ($(TEST_INFRA),provided)
	@test -s "$(TEST_REDIS_TLS_CA_CERT)" || { echo "✗ TEST_INFRA=provided but $(TEST_REDIS_TLS_CA_CERT) is missing — the caller did not actually provision infra"; exit 1; }
	@echo "✓ [infra] postgres + redis provided by caller (TEST_INFRA=provided); compose skipped"
else
	@if ! docker info >/dev/null 2>&1; then \
	  echo "✗ Docker daemon is not running — start Docker Desktop / dockerd and retry."; \
	  exit 1; \
	fi
	@# container_name in docker-compose.yml is fixed (agentsfleet-postgres / agentsfleet-redis),
	@# so another worktree's compose can leave stale containers blocking ours. Remove
	@# by name if they exist but are NOT owned by this project. Idempotent.
	@this_project=$$(basename "$(CURDIR)"); \
	for c in agentsfleet-postgres agentsfleet-redis agentsfleet-qstash; do \
	  owner=$$(docker inspect -f '{{ index .Config.Labels "com.docker.compose.project" }}' $$c 2>/dev/null); \
	  if [ -n "$$owner" ] && [ "$$owner" != "$$this_project" ]; then \
	    echo "→ [infra] Removing stale $$c (owned by project '$$owner')..."; \
	    docker rm -f $$c >/dev/null; \
	  fi; \
	done
	@echo "→ [infra] Starting postgres + redis + qstash (waiting for healthchecks)..."
	@docker compose up -d --wait postgres redis qstash
	@mkdir -p "$(CURDIR)/.tmp"
	@echo "→ [infra] Extracting Redis TLS CA cert..."
	@docker compose cp redis:/tls/server.crt "$(TEST_REDIS_TLS_CA_CERT)" >/dev/null
	@test -s "$(TEST_REDIS_TLS_CA_CERT)" || { echo "✗ Failed to extract Redis TLS cert"; exit 1; }
	@echo "✓ [infra] postgres + redis ready; Redis CA cert at $(TEST_REDIS_TLS_CA_CERT)"
endif

# Drop and recreate all app schemas so every test-integration run starts from a clean
# state. Needed because several tests in the suite (rbac, tenant_provider, event_loop) leave
# fixture rows behind (paused agents, lingering secrets) that break subsequent runs.
# Uses the same teardown.sql as the PlanetScale playbook for consistency.
# Redis is flushed in the same reset: fixture agent ids are fixed, so streams,
# consumer groups, and unacked PEL entries persist across runs — and the strand
# recovery path (own-PEL read + reclaim sweep) makes that stale state reachable,
# replaying prior-run events into a freshly reset DB (shared-tenant balance drift).
_reset-test-db: _ensure-test-infra
	@echo "→ [infra] Resetting test database schemas to a clean state..."
	@docker compose cp playbooks/operations/teardown/database/teardown.sql postgres:/tmp/teardown.sql >/dev/null
	@out=$$(docker compose exec -T postgres psql -U agentsfleet -d agentsfleetdb -v ON_ERROR_STOP=1 -q -f /tmp/teardown.sql 2>&1) || { echo "✗ [infra] teardown.sql failed"; echo "$$out"; exit 1; }; echo "$$out" | grep -v "^NOTICE:" | grep -v "^psql:" || true
	@docker compose exec -T postgres rm -f /tmp/teardown.sql >/dev/null
	@echo "✓ [infra] Schemas dropped; migrations will rebuild on next step"
	@echo "→ [infra] Flushing test Redis (prior-run streams/groups/PELs)..."
	@docker compose exec -T redis redis-cli --tls --cacert /tls/server.crt -a agentsfleet --no-auth-warning FLUSHALL >/dev/null
	@echo "✓ [infra] Redis flushed"

_test-integration-agentsfleetd:
	@echo "→ [agentsfleetd] Running Zig integration tests..."
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
	echo "→ [agentsfleetd] Running DB-backed integration tests using $$db_url..."; \
	mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"; \
	echo "→ [agentsfleetd] Auto-migrating test database..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	DATABASE_URL_MIGRATOR="$$db_url" \
	zig build run -- migrate; \
	echo "→ [agentsfleetd] Migration done, running tests..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	LIVE_DB=1 \
	TEST_DATABASE_URL="$$db_url" \
	AGENTSFLEET_QSTASH_LIVE_URL="$(QSTASH_DEV_URL_LOCAL)" \
	AGENTSFLEET_QSTASH_LIVE_TOKEN="$(QSTASH_DEV_TOKEN_LOCAL)" \
	zig build test
	@echo "✓ [agentsfleetd] DB-backed integration tests passed"

_test-integration-redis: _reset-test-db
	@redis_tls_test_url="$$TEST_REDIS_TLS_URL"; \
	if [ -z "$$redis_tls_test_url" ] && [ -n "$$REDIS_URL" ]; then \
	  case "$$REDIS_URL" in \
	    rediss://*) redis_tls_test_url="$$REDIS_URL" ;; \
	  esac; \
	fi; \
	if [ -z "$$redis_tls_test_url" ]; then redis_tls_test_url="$(TEST_REDIS_TLS_URL_LOCAL)"; fi; \
	echo "→ [agentsfleetd] Running Redis integration tests using $$redis_tls_test_url..."; \
	mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"; \
	env -u TEST_DATABASE_URL -u LIVE_DB \
	  ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	  ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	  TEST_REDIS_TLS_URL="$$redis_tls_test_url" \
	  REDIS_URL_API="$$redis_tls_test_url" \
	  REDIS_TLS_CA_CERT_FILE="$(TEST_REDIS_TLS_CA_CERT)" \
	  zig build test
	@echo "✓ [agentsfleetd] Redis integration tests passed"

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
	mkdir -p "$(ZIG_GLOBAL_CACHE_DIR)" "$(ZIG_LOCAL_CACHE_DIR)"; \
	echo "→ [agentsfleet-runner] Building the runner binary in the background so it overlaps the migrate compile (separate build graph, no datastore; silent until it links)..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	zig build --build-file build_runner.zig & \
	runner_build_pid=$$!; \
	echo "→ [agentsfleetd] Auto-migrating test database..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	DATABASE_URL_MIGRATOR="$$db_url" \
	zig build run -- migrate; migrate_rc=$$?; \
	echo "→ [agentsfleet-runner] Waiting for the background runner build (usually already linked during migrate)..."; \
	wait "$$runner_build_pid" || { echo "✗ [agentsfleet-runner] Runner binary build failed"; exit 1; }; \
	[ "$$migrate_rc" -eq 0 ] || { echo "✗ [agentsfleetd] Test database migration failed (exit $$migrate_rc) — not running tests against an unmigrated DB"; exit 1; }; \
	echo "✓ [agentsfleet-runner] Runner binary built."; \
	echo "→ [agentsfleetd] Building the integration test binary, then running the suite against real DB + Redis (silent zig compile first, then tests)..."; \
	ZIG_GLOBAL_CACHE_DIR="$(ZIG_GLOBAL_CACHE_DIR)" \
	ZIG_LOCAL_CACHE_DIR="$(ZIG_LOCAL_CACHE_DIR)" \
	AGENTSFLEET_RUNNER_BIN="$$(pwd)/zig-out/bin/agentsfleet-runner" \
	LIVE_DB=1 \
	TEST_DATABASE_URL="$$db_url" \
	TEST_REDIS_TLS_URL="$$redis_tls_test_url" \
	REDIS_URL_API="$$redis_tls_test_url" \
	REDIS_TLS_CA_CERT_FILE="$(TEST_REDIS_TLS_CA_CERT)" \
	AGENTSFLEET_QSTASH_LIVE_URL="$(QSTASH_DEV_URL_LOCAL)" \
	AGENTSFLEET_QSTASH_LIVE_TOKEN="$(QSTASH_DEV_TOKEN_LOCAL)" \
	zig build test
	@echo "✓ [agentsfleetd] Full integration suite passed"

test-integration-db: _test-integration-db  ## Run real DB-backed integration suite only

test-integration-redis: _test-integration-redis  ## Run Redis-backed integration suite only

test-integration: _test-integration-full  ## Run worker integration tests against real DB + Redis
	@echo "✓ [agentsfleetd] All integration tests passed"
