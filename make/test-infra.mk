# =============================================================================
# TEST-INFRA — compose datastores, ports, connection URLs, reset discipline
# =============================================================================
# Split from make/test-integration.mk when that file crossed the length cap:
# this is the disposable-environment half — what boots, where it listens, and
# how state is reset — while the lanes that consume it stay with the tests.

.PHONY: _ensure-test-infra _reset-test-db

# Host ports are FIXED. scripts/test-infra-ports.sh decides them: a LINKED
# worktree gets three ports derived from its project name (so worktrees cannot
# collide); a primary checkout — which is what CI always has — keeps the
# conventional 5432/6379/8080.
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
# Word 2 was the Redis service's slot. Redis is retired; the word stays in the
# allocator so every other port keeps the number its running containers have.
AGENTSFLEET_QSTASH_HOST_PORT ?= $(or $(word 3,$(TEST_INFRA_PORTS)),8080)
# The first of the Dragonfly cluster's four published data ports; the last is
# derived so compose can publish them as one range. The four admin ports that
# follow are loopback-only inside the container -- scripts/dragonfly-cluster.sh
# is the only thing that speaks to them, through `docker compose exec`.
AGENTSFLEET_DRAGONFLY_BASE_PORT ?= $(or $(word 4,$(TEST_INFRA_PORTS)),7001)
# Seven published data ports: four cluster nodes, the TLS node, then the two
# nodes of the migration source cluster. Admin ports beyond them stay inside
# the container (scripts/dragonfly-cluster.sh).
AGENTSFLEET_DRAGONFLY_LAST_PORT ?= $(shell echo $$(( $(AGENTSFLEET_DRAGONFLY_BASE_PORT) + 6 )))
# The TLS node is the fifth, named rather than derived from the range end. It
# used to BE the range end, and the source cluster moved that end past it — a
# derived spelling would have silently pointed the trust suite at a source node.
AGENTSFLEET_DRAGONFLY_TLS_HOST_PORT ?= $(shell echo $$(( $(AGENTSFLEET_DRAGONFLY_BASE_PORT) + 4 )))
# The first node of the migration source cluster; a cluster client discovers
# its sibling from this one.
AGENTSFLEET_DRAGONFLY_SOURCE_HOST_PORT ?= $(shell echo $$(( $(AGENTSFLEET_DRAGONFLY_BASE_PORT) + 5 )))
# The daemon's API port rides the dragonfly service (shared namespace), one
# past the datastore range so parallel lanes never meet on 3000. Still inside
# the eight ports this slot reserves for the block (scripts/test-infra-ports.sh).
AGENTSFLEET_API_HOST_PORT ?= $(shell echo $$(( $(AGENTSFLEET_DRAGONFLY_BASE_PORT) + 7 )))
# The plaintext Redis port, derived from the TLS one rather than allocated, so
# a worktree's two Redis ports move together and the allocator keeps owning one
# number per service.
export AGENTSFLEET_PG_HOST_PORT
export AGENTSFLEET_QSTASH_HOST_PORT
export AGENTSFLEET_DRAGONFLY_BASE_PORT AGENTSFLEET_DRAGONFLY_LAST_PORT AGENTSFLEET_API_HOST_PORT
export AGENTSFLEET_DRAGONFLY_TLS_HOST_PORT AGENTSFLEET_DRAGONFLY_SOURCE_HOST_PORT

# The live ports are still discovered from the running container rather than
# assumed from the values above, so these stay the single source of truth about
# what is actually bound.
#
# Resolved lazily with `=` -- NOT `:=` -- because the containers may not be
# running when this Makefile is first parsed; the shell runs at first use, which
# is always after _ensure-test-infra.
#
# The pinning is what makes that safe. While the host side was ephemeral, this
# lookup was correct when it ran and wrong afterwards: a container restart moved
# the port, the URL built from it did not follow, and every Redis test failed at
# TCP connect against a port nothing was listening on.
#
# Each falls back to the declared port when the lookup yields nothing. The case
# that produced this was `docker compose port` returning an empty string where
# no docker CLI was present, so the URL became
# `postgres://…@localhost:/agentsfleetdb`, which the daemon rejects as
# InvalidDatabaseUrl. The declared port is the right answer whenever the lookup
# cannot answer,
# because the caller provisioned the infra itself and told us where it is.
COMPOSE_PG_PORT = $(or $(strip $(shell docker compose port postgres 5432 2>/dev/null | sed 's/.*://')),$(AGENTSFLEET_PG_HOST_PORT))
COMPOSE_QSTASH_PORT = $(or $(strip $(shell docker compose port qstash 8080 2>/dev/null | sed 's/.*://')),$(AGENTSFLEET_QSTASH_HOST_PORT))
# No lookup for the cluster: its published numbers ARE its announced numbers,
# so the declared port is the only correct answer and a discovered one that
# disagreed would be the bug, not the truth.
COMPOSE_DRAGONFLY_PORT = $(AGENTSFLEET_DRAGONFLY_BASE_PORT)
COMPOSE_DRAGONFLY_TLS_PORT = $(AGENTSFLEET_DRAGONFLY_TLS_HOST_PORT)
COMPOSE_DRAGONFLY_SOURCE_PORT = $(AGENTSFLEET_DRAGONFLY_SOURCE_HOST_PORT)

# Optional narrowing, for studying ONE failure without the rest of the lane's
# cascade noise:  make test-integration TEST_FILTER='integration(model_library)'
#
# Exposes build.zig's existing `-Dtest-filter` on the existing targets rather
# than adding a parallel one, because everything BUT the test selection has to
# stay identical: the schema reset, the migrate, the `docker compose port`
# discovery, and the CA-freshness check are the parts a hand-rolled `zig build
# test-integration` gets wrong. §Discovery's "why the lane was lying" is what
# that costs — a suite dialling a dead port, read as behaviour. Skipping the
# reset is the other half: a second run against un-reset state goes 457/0 →
# 447/10.
#
# NOTE: a filter REPLACES the integration graph's own default filters (the
# `_integration_test` file filter and the `integration:` name filter), so a
# narrowed run selects across the whole integration root rather than within
# those. Check your filter actually matches something — a filter that matches
# nothing exits 0 and reads as a pass.
#
# Empty by default, so the R1-graded invocation is always the full suite.
TEST_FILTER ?=
ZIG_TEST_FILTER_ARG = $(if $(strip $(TEST_FILTER)),-Dtest-filter="$(TEST_FILTER)",)

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
# over 20 sequential Redis connects, `localhost` ran a 566 ms median with a
# 6343 ms worst case, and the literal address ran 246 ms with a 2194 ms worst.
# Every live test opens its own connection, so the median is multiplied across
# the lane and the tail is what made `ConnectTimeout` a coin flip against the
# 5 s connect budget. The Redis certificate carries `IP:127.0.0.1` in its SAN
# beside `DNS:localhost`, so TLS verification is unaffected.
TEST_DATABASE_URL ?= postgres://agentsfleet:agentsfleet@127.0.0.1:$(COMPOSE_PG_PORT)/agentsfleetdb?sslmode=disable
# `redis://`, not `rediss://`, and the reason is measured rather than assumed.
#
# A TLS connect to the lane's Redis runs a 232 ms median against 0.1 ms for
# plain TCP to the same server, and that difference is a handshake re-proving a
# certificate authority which is identical on every one of the two hundred
# connects a lane opens. It is not merely slow: a connect's budget starts
# BEFORE the connection is admitted, so the queue those handshakes build is
# what spends the budget, and a perfectly healthy Redis answers `ConnectTimeout`
# to whichever test sat deepest in it.
#
# Nothing about TLS goes unproven. It moves to `TEST_REDIS_TLS_URL` below and is
# proven where proving it means something -- and proven harder, because that
# suite asserts a foreign authority is REFUSED, which an all-TLS lane never did:
# every connect there used the right certificate, so a lane that had silently
# stopped verifying would have passed exactly the same.
# The cluster, as one seed. A cluster client discovers the other nodes from
# this one, and a suite that needs every address asks the topology for it
# rather than a second variable. Plaintext: the TLS handshake is paid where
# it proves something, below. The name is the environment contract the daemon
# reads (REDIS_URL); the datastore behind it is Dragonfly.
TEST_REDIS_URL ?= redis://:agentsfleet@127.0.0.1:$(COMPOSE_DRAGONFLY_PORT)
# The TLS node, for the suite whose subject IS the trust decision.
TEST_REDIS_TLS_URL ?= rediss://:agentsfleet@127.0.0.1:$(COMPOSE_DRAGONFLY_TLS_PORT)
# Cert path — populated by _ensure-test-infra after the datastore is healthy.
# Do NOT shell-expand at parse time; the container may not be running yet when
# the Makefile is first evaluated.
TEST_REDIS_CA_CERT ?= $(CURDIR)/.tmp/redis-ca.crt
# The authority that signed nothing here, for the refusal half of the trust
# dimension. Extracted beside the real one; see `integration_tls_trust.rs`.
TEST_REDIS_FOREIGN_CA ?= $(CURDIR)/.tmp/redis-foreign-ca.crt
# The datastore an operator is switching AWAY from: its own two-primary
# cluster, given as one seed like the target. The migration rehearsal imports
# from here and must leave it intact; nothing else in the lane touches it.
TEST_REDIS_SOURCE_URL ?= redis://:agentsfleet@127.0.0.1:$(COMPOSE_DRAGONFLY_SOURCE_PORT)
# How a test moves slots or resets the cluster: the script, inside its own
# container, with the caller's arguments appended. The suites know one command
# line and nothing about docker; the admin ports it reaches never leave the
# container (see scripts/dragonfly-cluster.sh).
TEST_DRAGONFLY_CONTROL ?= docker compose --project-directory $(CURDIR) exec -T dragonfly bash /scripts/dragonfly-cluster.sh
export TEST_DATABASE_URL TEST_REDIS_URL TEST_REDIS_TLS_URL TEST_REDIS_CA_CERT TEST_REDIS_FOREIGN_CA TEST_REDIS_SOURCE_URL TEST_DRAGONFLY_CONTROL
# QStash local dev server (docker-compose `qstash` service). The emulator ships a
# hardcoded local identity and rejects anything else (a different user 404s, a
# different password 401s), so this is a fixture we reproduce, not a credential we
# choose — and nothing it authenticates to holds real data. Derived here from its
# two plain parts so no credential-shaped blob is stored in the repo.
# The opt-in live QStash tests read these vars; unset (or server down) → self-skip.
# The API BASE, `/v2` included. `QStash::upsert` composes
# `{api_base}/schedules/{destination}`, matching the vendor's own
# `https://qstash.upstash.io/v2`, so a base without the version segment 404s
# every push — and a 404 is a refusal, so the row lands `Failed` with "not yet
# registered" and reads exactly like a scheduler outage.
#
# `127.0.0.1` rather than `localhost` for the reason the datastore URLs above
# give: measured here at 13 ms against 140 ms for the name.
QSTASH_DEV_URL_LOCAL ?= http://127.0.0.1:$(COMPOSE_QSTASH_PORT)/v2
QSTASH_DEV_IDENTITY ?= defaultUser
QSTASH_DEV_SECRET ?= defaultPassword
QSTASH_DEV_TOKEN_LOCAL ?= $(shell printf '{"UserID":"%s","Password":"%s"}' '$(QSTASH_DEV_IDENTITY)' '$(QSTASH_DEV_SECRET)' | base64 | tr -d '\n')

# The names the live-scheduler tests read. Exported here beside the datastore
# URLs for their reason: the suites read these names directly rather than a lane
# resolving them into a fourth spelling. `make/test-integration.mk` exported the
# same two before M175 §6 deleted it with the Zig gating, and nothing re-exported
# them afterwards -- which is why the Rust port's schedule sync had "no QStash
# fake" recorded against it while the compose service was up the whole time.
AGENTSFLEET_QSTASH_LIVE_URL ?= $(QSTASH_DEV_URL_LOCAL)
AGENTSFLEET_QSTASH_LIVE_TOKEN ?= $(QSTASH_DEV_TOKEN_LOCAL)
export AGENTSFLEET_QSTASH_LIVE_URL AGENTSFLEET_QSTASH_LIVE_TOKEN

# Bring postgres + dragonfly + qstash up via docker compose and wait for healthchecks to pass.
# Idempotent — if already healthy, docker compose up --wait is a no-op. Safe to call
# multiple times. Extracts the datastore TLS CA cert after the container is healthy so
# subsequent targets can rely on $(TEST_REDIS_CA_CERT) being present.
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
	@echo "→ [infra] Host ports: postgres=$(AGENTSFLEET_PG_HOST_PORT) qstash=$(AGENTSFLEET_QSTASH_HOST_PORT) dragonfly=$(AGENTSFLEET_DRAGONFLY_BASE_PORT)-$(AGENTSFLEET_DRAGONFLY_LAST_PORT) api=$(AGENTSFLEET_API_HOST_PORT)"
	@echo "→ [infra] Starting postgres + dragonfly + qstash (waiting for healthchecks)..."
	@docker compose up -d --wait --remove-orphans postgres dragonfly qstash
	@mkdir -p "$(CURDIR)/.tmp"
	@echo "→ [infra] Extracting the datastore TLS CA cert..."
	@# No `>/dev/null`: a failed copy used to be silent, and the `test -s` below
	@# only proved the file was non-empty — which a STALE cert from a destroyed
	@# container satisfies. Every TLS connection then failed signature
	@# verification, which reads as dozens of unrelated datastore test failures.
	@docker compose cp dragonfly:/data/tls/ca.crt "$(TEST_REDIS_CA_CERT)"
	@docker compose cp dragonfly:/data/tls/foreign-ca.crt "$(TEST_REDIS_FOREIGN_CA)"
	@test -s "$(TEST_REDIS_CA_CERT)" || { echo "✗ Failed to extract the datastore TLS cert"; exit 1; }
	@# Freshness, not size: the copied cert must be byte-identical to the one the
	@# server is actually presenting.
	@container_sha=$$(docker compose exec -T dragonfly sha256sum /data/tls/ca.crt | awk '{print $$1}'); \
	local_sha=$$(shasum -a 256 "$(TEST_REDIS_CA_CERT)" | awk '{print $$1}'); \
	if [ "$$container_sha" != "$$local_sha" ]; then \
	  echo "✗ [infra] datastore CA cert is stale (container $$container_sha != local $$local_sha)"; \
	  exit 1; \
	fi
	@echo "✓ [infra] postgres + dragonfly + qstash ready; datastore CA cert at $(TEST_REDIS_CA_CERT)"

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
	@# The cluster too, and its slot layout: a migration test that moved slots
	@# and failed before moving them back would otherwise hand the next run a
	@# topology it did not ask for. Owned by construction -- the script only
	@# ever addresses loopback inside its own container.
	@echo "→ [infra] Flushing test Dragonfly cluster and restoring its slot layout..."
	@$(TEST_DRAGONFLY_CONTROL) reset >/dev/null
	@echo "✓ [infra] Dragonfly flushed"

# Every integration target starts by dropping schemas and flushing Redis,
# because several suites leave fixture rows behind that break the next run. That
# is the right default for a gate and the wrong one for an edit-run-edit loop,
# where the reset plus re-migrate dominates a narrow `-Dtest-filter` run.
#
# KEEP_TEST_STATE=1 swaps the reset for a plain infra check. It is deliberately
# opt-in and never set by CI: a green run under it proves nothing about a clean
# checkout, which is exactly what the third verification tier exists to check.
TEST_STATE_DEP := $(if $(KEEP_TEST_STATE),_ensure-test-infra,_reset-test-db)
