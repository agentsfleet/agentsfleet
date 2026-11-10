# =============================================================================
# TEST-INFRA — compose datastores, ports, connection URLs, reset discipline
# =============================================================================
# Split from make/test-integration.mk when that file crossed the length cap:
# this is the disposable-environment half — what boots, where it listens, and
# how state is reset — while the lanes that consume it stay with the tests.

.PHONY: _ensure-test-infra _reset-test-db

# Host ports are FIXED. scripts/test-infra-ports.sh decides them: a LINKED
# worktree gets its ports derived from its project name (so worktrees cannot
# collide); a primary checkout — which is what CI always has — keeps the
# conventional 5432/8080.
#
# Exported into the environment rather than written to `.env`. Compose reads
# `.env` automatically, which is why the first attempt used it, but in CI the
# make target runs inside a container as root: the `.env` it wrote was unreadable
# by the host runner and moved the published ports out from under the connection
# strings the workflow had already pinned. An exported variable crosses no
# ownership boundary.
#
# `?=` so an explicit override still wins. Computed once per make invocation.
TEST_INFRA_PORTS := $(shell bash scripts/test-infra-ports.sh 2>/dev/null)
AGENTSFLEET_PG_HOST_PORT     ?= $(or $(word 1,$(TEST_INFRA_PORTS)),5432)
AGENTSFLEET_QSTASH_HOST_PORT ?= $(or $(word 2,$(TEST_INFRA_PORTS)),8080)
# The daemon container's API port. It rides the mailpit service (shared
# namespace), apart from 3000 so parallel lanes never meet there.
AGENTSFLEET_API_HOST_PORT ?= $(or $(word 3,$(TEST_INFRA_PORTS)),7006)
export AGENTSFLEET_PG_HOST_PORT
export AGENTSFLEET_QSTASH_HOST_PORT
export AGENTSFLEET_API_HOST_PORT
# Mailpit's SMTP port and web page, the next two past the API port: still
# inside the slot scripts/test-infra-ports.sh allocates per worktree.
AGENTSFLEET_MAILPIT_SMTP_PORT ?= $(shell echo $$(( $(AGENTSFLEET_API_HOST_PORT) + 1 )))
AGENTSFLEET_MAILPIT_UI_PORT ?= $(shell echo $$(( $(AGENTSFLEET_API_HOST_PORT) + 2 )))
export AGENTSFLEET_MAILPIT_SMTP_PORT AGENTSFLEET_MAILPIT_UI_PORT

# The URLs below use the declared ports directly. They are fixed (above) and
# compose publishes exactly these numbers, so the declared port IS the bound
# one; a lookup could only disagree by being wrong.

# WHERE THE TEST SUITES GET THEIR SERVICES — three names, and only three.
#
# Each is `?=` and EXPORTED, which is the whole mechanism: `?=` means the
# environment or a CI job wins, `export` means a test process reads the same
# name this file defines. No recipe re-derives them, and no suite reads a
# differently-prefixed alias of them.
#
# They were four names and a shell macro until M176. `TEST_DATABASE_URL_LOCAL`
# was a default, bare `TEST_DATABASE_URL` was an override hook nothing ever set,
# `RUSTD_RESOLVE_DB_URL` picked between them and re-appended a `sslmode` the
# default already carried, and the recipe passed the result on as
# `AFD_TEST_DATABASE_URL` — one value, three names, and a resolver whose only
# live branch was "use the default". `?=` is the make idiom that already means
# what the resolver was hand-rolling.
#
# sslmode=disable: the local docker Postgres has no TLS and parseUrl defaults to
# `.require` (hosted providers mandate it) — without it every local DB-lane test
# fails at connect with SSLNotSupportedByServer before it can run. It is IN the
# default rather than appended by a recipe, so a hosted URL supplied from the
# environment keeps its TLS instead of having it stripped by a lane.
#
# 127.0.0.1, not `localhost`. The name costs a resolver round trip on every
# connection, and on macOS it occasionally costs a five-second one: measured
# over 20 sequential datastore connects, `localhost` ran a 566 ms median with a
# 6343 ms worst case, and the literal address ran 246 ms with a 2194 ms worst.
# Every live test opens its own connection, so the median is multiplied across
# the lane and the tail is what made `ConnectTimeout` a coin flip against the
# 5 s connect budget.
TEST_DATABASE_URL ?= postgres://agentsfleet:agentsfleet@127.0.0.1:$(AGENTSFLEET_PG_HOST_PORT)/agentsfleetdb?sslmode=disable
# The API pool's login, as production's: a member of `api_runtime` and nothing
# else (`pscale role create … --inherited-roles api_runtime`,
# playbooks/operations/database_region_move/001_playbook.md). The owner above
# holds every grant, so a suite connecting as it never meets a missing one; the
# daemon does. `_migrate-test-db` creates this login once the schema's roles exist.
TEST_DATABASE_API_LOGIN := agentsfleet_api
TEST_DATABASE_URL_API ?= postgres://$(TEST_DATABASE_API_LOGIN):$(TEST_DATABASE_API_LOGIN)@127.0.0.1:$(AGENTSFLEET_PG_HOST_PORT)/agentsfleetdb?sslmode=disable
export TEST_DATABASE_URL TEST_DATABASE_URL_API

# Three core NATS nodes, one client port each. Clients take every node as a
# seed, because the nodes advertise nothing, so a test that stops one node
# reconnects to a peer. Nothing persists, so the reset has nothing to flush.
AGENTSFLEET_NATS_BASE_PORT ?= $(or $(word 4,$(TEST_INFRA_PORTS)),4222)
AGENTSFLEET_NATS_PORT_2 ?= $(shell echo $$(( $(AGENTSFLEET_NATS_BASE_PORT) + 1 )))
AGENTSFLEET_NATS_PORT_3 ?= $(shell echo $$(( $(AGENTSFLEET_NATS_BASE_PORT) + 2 )))
export AGENTSFLEET_NATS_BASE_PORT AGENTSFLEET_NATS_PORT_2 AGENTSFLEET_NATS_PORT_3
TEST_NATS_URL ?= nats://agentsfleet:agentsfleet@127.0.0.1:$(AGENTSFLEET_NATS_BASE_PORT),nats://agentsfleet:agentsfleet@127.0.0.1:$(AGENTSFLEET_NATS_PORT_2),nats://agentsfleet:agentsfleet@127.0.0.1:$(AGENTSFLEET_NATS_PORT_3)
# How a test stops and restarts one node: compose, with the verb appended
# (`stop nats-2`, `up -d --wait nats-2`). Only the exclusive lane uses it.
TEST_NATS_CONTROL ?= docker compose --project-directory $(CURDIR)
export TEST_NATS_URL TEST_NATS_CONTROL

# The invite email's sink: the port a suite's `smtp-relay` bag names, and the
# API it reads the delivered message back from.
TEST_MAILPIT_SMTP_PORT ?= $(AGENTSFLEET_MAILPIT_SMTP_PORT)
TEST_MAILPIT_URL ?= http://127.0.0.1:$(AGENTSFLEET_MAILPIT_UI_PORT)
export TEST_MAILPIT_SMTP_PORT TEST_MAILPIT_URL
# QStash (Upstash's message queue) local dev server, the docker-compose `qstash`
# service. The opt-in live-scheduler tests read these two names directly, so they
# are exported here beside the datastore URLs; unset, or the server down, and
# those tests skip themselves.
#
# The emulator ships a hardcoded local identity and rejects anything else (a
# different user 404s, a different password 401s), so the token is a fixture we
# reproduce, not a credential we choose, and nothing it authenticates to holds
# real data. It is derived from its two plain parts so no credential-shaped blob
# is stored in the repo.
#
# The URL is the API base, `/v2` included. `QStash::upsert` composes
# `{api_base}/schedules/{destination}`, matching the vendor's own
# `https://qstash.upstash.io/v2`, so a base without the version segment 404s
# every push, and a 404 is a refusal: the row lands `Failed` with "not yet
# registered" and reads exactly like a scheduler outage. `127.0.0.1` rather than
# `localhost` for the reason the datastore URLs above give: measured here at
# 13 ms against 140 ms for the name.
AGENTSFLEET_QSTASH_LIVE_URL ?= http://127.0.0.1:$(AGENTSFLEET_QSTASH_HOST_PORT)/v2
AGENTSFLEET_QSTASH_LIVE_TOKEN ?= $(shell printf '{"UserID":"%s","Password":"%s"}' defaultUser defaultPassword | base64 | tr -d '\n')
export AGENTSFLEET_QSTASH_LIVE_URL AGENTSFLEET_QSTASH_LIVE_TOKEN

# Bring postgres + qstash + mailpit + nats up via docker compose and wait for healthchecks
# to pass. Idempotent — if already healthy, docker compose up --wait is a no-op. Safe to
# call multiple times.
#
_ensure-test-infra:
	@if ! docker info >/dev/null 2>&1; then \
	  echo "✗ Docker daemon is not running — start Docker Desktop / dockerd and retry."; \
	  exit 1; \
	fi
	@# No stale-container sweep: compose namespaces these services per project, so a
	@# sibling worktree's containers are simply different containers. The sweep that
	@# used to live here force-removed them by fixed name, which is what let one
	@# worktree's test run destroy another's mid-flight.
	@echo "→ [infra] Host ports: postgres=$(AGENTSFLEET_PG_HOST_PORT) qstash=$(AGENTSFLEET_QSTASH_HOST_PORT) api=$(AGENTSFLEET_API_HOST_PORT) mailpit=$(AGENTSFLEET_MAILPIT_SMTP_PORT)/$(AGENTSFLEET_MAILPIT_UI_PORT) nats=$(AGENTSFLEET_NATS_BASE_PORT)-$(AGENTSFLEET_NATS_PORT_3)"
	@echo "→ [infra] Starting postgres + qstash + mailpit + nats (waiting for healthchecks)..."
	@docker compose up -d --wait --remove-orphans postgres qstash mailpit nats-1 nats-2 nats-3
	@echo "✓ [infra] postgres + qstash + mailpit + nats ready"

# Drop and recreate all app schemas so every test-integration run starts from a clean
# state. Needed because several tests in the suite (rbac, tenant_provider, event_loop) leave
# fixture rows behind (paused agents, lingering secrets) that break subsequent runs.
# Uses the same teardown.sql as the PlanetScale playbook for consistency. The bus
# keeps nothing (core NATS, no JetStream), so there is nothing else to reset.
_reset-test-db: _ensure-test-infra
	@echo "→ [infra] Resetting test database schemas to a clean state..."
	@docker compose cp playbooks/operations/teardown/database/teardown.sql postgres:/tmp/teardown.sql >/dev/null
	@out=$$(docker compose exec -T postgres psql -U agentsfleet -d agentsfleetdb -v ON_ERROR_STOP=1 -q -f /tmp/teardown.sql 2>&1) || { echo "✗ [infra] teardown.sql failed"; echo "$$out"; exit 1; }; echo "$$out" | grep -v "^NOTICE:" | grep -v "^psql:" || true
	@docker compose exec -T postgres rm -f /tmp/teardown.sql >/dev/null
	@echo "✓ [infra] Schemas dropped; migrations will rebuild on next step"

# Every integration target starts by dropping schemas,
# because several suites leave fixture rows behind that break the next run. That
# is the right default for a gate and the wrong one for an edit-run-edit loop,
# where the reset plus re-migrate dominates a run filtered to one test.
#
# KEEP_TEST_STATE=1 swaps the reset for a plain infra check. It is deliberately
# opt-in and never set by CI: a green run under it proves nothing about a clean
# checkout, which is exactly what the third verification tier exists to check.
TEST_STATE_DEP := $(if $(KEEP_TEST_STATE),_ensure-test-infra,_reset-test-db)
