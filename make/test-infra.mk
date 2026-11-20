.PHONY: _ensure-test-infra _reset-test-db

# Fixed host ports from scripts/test-infra-ports.sh: a linked worktree derives
# its own, a primary checkout (CI) keeps 5432/8080. Exported, never written to
# `.env`: CI's container wrote it as root and moved ports under the workflow.
# `?=` so an override wins.
TEST_INFRA_PORTS := $(shell bash scripts/test-infra-ports.sh 2>/dev/null)
AGENTSFLEET_PG_HOST_PORT     ?= $(or $(word 1,$(TEST_INFRA_PORTS)),5432)
AGENTSFLEET_QSTASH_HOST_PORT ?= $(or $(word 2,$(TEST_INFRA_PORTS)),8080)
# The daemon container's API port, in the mailpit service's namespace.
AGENTSFLEET_API_HOST_PORT ?= $(or $(word 3,$(TEST_INFRA_PORTS)),7006)
export AGENTSFLEET_PG_HOST_PORT
export AGENTSFLEET_QSTASH_HOST_PORT
export AGENTSFLEET_API_HOST_PORT
# Mailpit SMTP and web, the next two ports in this worktree's slot.
AGENTSFLEET_MAILPIT_SMTP_PORT ?= $(shell echo $$(( $(AGENTSFLEET_API_HOST_PORT) + 1 )))
AGENTSFLEET_MAILPIT_UI_PORT ?= $(shell echo $$(( $(AGENTSFLEET_API_HOST_PORT) + 2 )))
export AGENTSFLEET_MAILPIT_SMTP_PORT AGENTSFLEET_MAILPIT_UI_PORT

# The services the suites read, by these exact names: `?=` so the environment
# wins, `export` so a test reads what this file defines.
# sslmode=disable is in the default because local Postgres has no TLS; a hosted
# URL from the environment keeps its own. 127.0.0.1, not `localhost`: on macOS
# the name cost up to 6 s per connect against a 5 s budget.
TEST_DATABASE_URL ?= postgres://agentsfleet:agentsfleet@127.0.0.1:$(AGENTSFLEET_PG_HOST_PORT)/agentsfleetdb?sslmode=disable
# The API pool's login, as production's: a member of `api_runtime` only, so a
# missing grant fails here as it would deployed. `_migrate-test-db` creates it.
TEST_DATABASE_API_LOGIN := agentsfleet_api
TEST_DATABASE_URL_API ?= postgres://$(TEST_DATABASE_API_LOGIN):$(TEST_DATABASE_API_LOGIN)@127.0.0.1:$(AGENTSFLEET_PG_HOST_PORT)/agentsfleetdb?sslmode=disable
export TEST_DATABASE_URL TEST_DATABASE_URL_API

# Three core NATS nodes advertise nothing, so clients take every node as a seed
# and a test that stops one reconnects to a peer.
AGENTSFLEET_NATS_BASE_PORT ?= $(or $(word 4,$(TEST_INFRA_PORTS)),4222)
AGENTSFLEET_NATS_PORT_2 ?= $(shell echo $$(( $(AGENTSFLEET_NATS_BASE_PORT) + 1 )))
AGENTSFLEET_NATS_PORT_3 ?= $(shell echo $$(( $(AGENTSFLEET_NATS_BASE_PORT) + 2 )))
export AGENTSFLEET_NATS_BASE_PORT AGENTSFLEET_NATS_PORT_2 AGENTSFLEET_NATS_PORT_3
TEST_NATS_URL ?= nats://agentsfleet:agentsfleet@127.0.0.1:$(AGENTSFLEET_NATS_BASE_PORT),nats://agentsfleet:agentsfleet@127.0.0.1:$(AGENTSFLEET_NATS_PORT_2),nats://agentsfleet:agentsfleet@127.0.0.1:$(AGENTSFLEET_NATS_PORT_3)
# How the exclusive lane stops and restarts one node: compose plus the verb.
TEST_NATS_CONTROL ?= docker compose --project-directory $(CURDIR)
export TEST_NATS_URL TEST_NATS_CONTROL
# The model catalogue seeder's Bun suite, which afd_db's catalogue test runs
# against a freshly migrated database.
TEST_CATALOGUE_SUITE ?= $(CURDIR)/scripts/model-catalogue
export TEST_CATALOGUE_SUITE

# The invite email's sink: its SMTP port and the API it is read back from.
TEST_MAILPIT_SMTP_PORT ?= $(AGENTSFLEET_MAILPIT_SMTP_PORT)
TEST_MAILPIT_URL ?= http://127.0.0.1:$(AGENTSFLEET_MAILPIT_UI_PORT)
export TEST_MAILPIT_SMTP_PORT TEST_MAILPIT_URL
# QStash's local emulator. The token is the emulator's hardcoded identity,
# derived from its plain parts so no credential-shaped blob sits in the repo.
# The base must carry `/v2`: without it every push 404s and reads as an outage.
AGENTSFLEET_QSTASH_LIVE_URL ?= http://127.0.0.1:$(AGENTSFLEET_QSTASH_HOST_PORT)/v2
AGENTSFLEET_QSTASH_LIVE_TOKEN ?= $(shell printf '{"UserID":"%s","Password":"%s"}' defaultUser defaultPassword | base64 | tr -d '\n')
export AGENTSFLEET_QSTASH_LIVE_URL AGENTSFLEET_QSTASH_LIVE_TOKEN

# Idempotent: `up --wait` is a no-op when everything is healthy.
_ensure-test-infra:
	@if ! docker info >/dev/null 2>&1; then \
	  echo "✗ Docker daemon is not running — start Docker Desktop / dockerd and retry."; \
	  exit 1; \
	fi
	@# No stale-container sweep: compose namespaces per project, and a sweep by
	@# fixed name once destroyed a sibling worktree's run mid-flight.
	@echo "→ [infra] Host ports: postgres=$(AGENTSFLEET_PG_HOST_PORT) qstash=$(AGENTSFLEET_QSTASH_HOST_PORT) api=$(AGENTSFLEET_API_HOST_PORT) mailpit=$(AGENTSFLEET_MAILPIT_SMTP_PORT)/$(AGENTSFLEET_MAILPIT_UI_PORT) nats=$(AGENTSFLEET_NATS_BASE_PORT)-$(AGENTSFLEET_NATS_PORT_3)"
	@echo "→ [infra] Starting postgres + qstash + mailpit + nats (waiting for healthchecks)..."
	@docker compose up -d --wait --remove-orphans postgres qstash mailpit ant bee elk
	@echo "✓ [infra] postgres + qstash + mailpit + nats ready"

# Drops every app schema, since suites leave fixture rows behind. The bus keeps
# nothing (core NATS), so there is nothing else to reset.
_reset-test-db: _ensure-test-infra
	@echo "→ [infra] Resetting test database schemas to a clean state..."
	@docker compose cp playbooks/operations/teardown/database/teardown.sql postgres:/tmp/teardown.sql >/dev/null
	@out=$$(docker compose exec -T postgres psql -U agentsfleet -d agentsfleetdb -v ON_ERROR_STOP=1 -q -f /tmp/teardown.sql 2>&1) || { echo "✗ [infra] teardown.sql failed"; echo "$$out"; exit 1; }; echo "$$out" | grep -v "^NOTICE:" | grep -v "^psql:" || true
	@docker compose exec -T postgres rm -f /tmp/teardown.sql >/dev/null
	@echo "✓ [infra] Schemas dropped; migrations will rebuild on next step"

# KEEP_TEST_STATE=1 skips the reset for the inner loop. CI never sets it: a
# green run under it proves nothing about a clean checkout.
TEST_STATE_DEP := $(if $(KEEP_TEST_STATE),_ensure-test-infra,_reset-test-db)
