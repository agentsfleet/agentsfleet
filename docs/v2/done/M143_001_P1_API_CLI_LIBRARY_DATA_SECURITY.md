<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->
# M143_001: Library data reads are bounded and secret-safe

**Prototype:** v2.0.0
**Milestone:** M143
**Workstream:** 001
**Date:** Jul 24, 2026
**Status:** DONE
**Priority:** P1 — library reads repeat database and decrypt work and expose unbounded behavior
**Categories:** API, CLI
**Batch:** B1 — establishes interfaces consumed by later workstreams
**Branch:** `feat/m143-library-read-surfaces`
**Test Baseline:** unit=3051 integration=407
**Depends on:** none
**Provenance:** LLM-drafted (Codex, Jul 24, 2026) from Oracle second-pass review
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §§8.2–10 and `docs/architecture/fleet_bundles.md` §Library tiers

---

## Overview

**Goal (testable):** Authenticated model and Fleet APIs return bounded deterministic pages with set-oriented projection, race-safe secret references, and no secret-value egress.
**Problem:** Tenant models and Fleet reads are unbounded, repeat work, and lack exact cursor, cache, identity, and mutation-race rules.
**Solution summary:** Add tenant and global model keysets, tier-qualified Fleet reads, transactionally generated catalogue revisions shared by response and billing caches, and one lock protocol for every secret-reference producer and delete path.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api): bound and secure library data reads
- **Intent (one sentence):** Users and automation receive stable library pages without secret leakage, identity ambiguity, stale billing rates, or race windows.
- **Handshake** — at PLAN, restate the Intent and assumptions; mismatch means STOP before edits.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/tenant_model_entries_view.zig` and `state/vault.zig` — projection and presence patterns.
2. `src/agentsfleetd/state/model_library/sql.zig` and `fleet_library/sql.zig` — catalogue and ordering ownership.
3. `docs/REST_API_DESIGN_GUIDELINES.md` and `docs/AUTH.md` — API, authorization, and secret rules.
4. `docs/architecture/billing_and_provider_keys.md` and `fleet_bundles.md` — billing and tier semantics.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/tenant_model_entries_view.zig`; `handlers/tenant_model_entries.zig`; `handlers/fleets/secrets.zig` | EDIT | Paged projection and reference-producing paths. |
| `src/agentsfleetd/secrets/crypto_store.zig`; `secrets/sql.zig`; `state/vault.zig`; `state/secret_probe.zig`; `state/tenant_provider.zig` | EDIT | Bulk decrypt/presence and shared lock order. |
| `src/agentsfleetd/state/tenant_model_entries.zig`; `state/tenant_model_entries/sql.zig` | EDIT | Tenant registry keyset and reference transaction. |
| `src/agentsfleetd/http/handlers/model_library.zig`; `handlers/admin/model_library_admin.zig`; `state/model_library_store.zig`; `state/model_library/sql.zig`; `state/model_rate_cache.zig` | EDIT/CREATE | Global page, mutations, revision, response and billing caches. |
| `src/agentsfleetd/http/handlers/library/gallery.zig`; `handlers/library/catalog.zig`; `handlers/library/entry_view.zig`; `handlers/library/gallery_detail.zig`; `fleet_library/sql.zig`; `state/model_library_cache.zig` | EDIT/CREATE | Fleet page/detail and response LRU. |
| `src/agentsfleetd/http/route_table.zig`; `route_table_invoke_library.zig`; `routes.zig`; `route_scopes.zig`; `route_matchers.zig`; `route_matchers_library.zig` | EDIT/CREATE | Export exact collection/detail matchers without growing `router.zig`. |
| `schema/0NN_model_library_revision.sql`; `schema/embed.zig` | CREATE/EDIT | Database-owned catalogue generation. **`NN` is the next free slot as it stands at CHORE(open), not a number fixed here.** Slot 35 was free when this spec was drafted and is now taken by `035_workspace_create_idempotency.sql` (merged in #556); M144_001 may or may not free it again. Read `schema/` and `schema/embed.zig`, take the next unused version, and record the chosen number in Discovery. Two specs naming the same slot is how a merge ships two migrations under one version. |
| `public/openapi/root.yaml`; `paths/models.yaml`; `paths/fleet-library.yaml`; `components/schemas.yaml`; `public/openapi.json` | EDIT | Exact paths, pages, headers, errors, and generated API. |
| `public/llms.txt`; `public/skill.md`; `public/agentsfleet-manifest.json` | EDIT | Exact public operation inventories. |
| `cli/src/lib/api-paths.ts`; `cli/src/commands/fleet_library.ts`; `cli/src/commands/fleet_install_source.ts`; `cli/src/commands/fleet_install.ts`; `cli/test/fleet_library.test.ts`; `cli/test/fleet_install.test.ts` | EDIT | Tier-qualified paged API use. |
| `ui/packages/app/lib/api/model_library.ts`; `ui/packages/app/lib/api/fleet-library.ts`; `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/actions.ts`; `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/lib/reads.ts`; `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/actions.ts` | EDIT | Typed consumers and server-brokered detail; no broad types growth. |
| `docs/REST_API_DESIGN_GUIDELINES.md`; `docs/architecture/billing_and_provider_keys.md`; `fleet_bundles.md`; `data_flow.md`; `user_flow.md`; `docs/AUTH.md` | EDIT | Compound-keyset exception, billing, lock, and metadata rules. |
| `src/agentsfleetd/http/handlers/model_library_integration_test.zig`; `handlers/admin/model_library_admin_integration_test.zig`; `handlers/library/catalog_integration_test.zig`; `src/agentsfleetd/state/tenant_model_entries_integration_test.zig`; `state/model_rate_cache_integration_test.zig`; `src/agentsfleetd/http/route_scopes_test.zig`; `route_matchers_test.zig` | EDIT/CREATE | Cursor, cache, race, routing, bounds, and sink proof. |
| `src/agentsfleetd/http/handlers/library/library_cursor_test.zig`; `library_query_normalization_test.zig`; `library_keyset_test.zig`; `src/agentsfleetd/state/model_library_cache_test.zig`; `state/model_rate_cache_key_test.zig` | CREATE | Unit tier for the cursor codec, normalization, seek predicate, cache accounting, and rate-cache key. |

**Scope grading.** Rubric R3 compares `git diff --name-only origin/main` against this table. Every cell is an exact path, so the comparison is mechanical. A path that turns out to be genuinely required and is missing here is a spec amendment, recorded in Discovery, not a silent addition.

### Files Changed — amendments (§1 implementation)

Each row below is a path the diff touches that the original table did not name, with the reason it was required. Recorded per the rule above.

| File | Action | Why it was required |
|------|--------|---------------------|
| `src/agentsfleetd/secrets/metadata.zig` | CREATE (moved) | Was `http/handlers/fleets/secret_metadata.zig`. The projection became a **write-time** function, so `state/vault.zig` must call it; leaving it under `http/handlers/` would make `state/` import from the HTTP layer. |
| `src/agentsfleetd/secrets/metadata_backfill.zig`; `cmd/backfill.zig`; `cli/commands.zig`; `main.zig` | CREATE/EDIT | `agentsfleetd backfill`. §1 promotes columns a SQL migration cannot populate (the Key Encryption Key lives in the application), so the sweep needs a command. |
| `src/agentsfleetd/http/pagination.zig` | CREATE | The shared compound-cursor codec and limit parser. §§1–3 all need one; three bespoke copies is how their canonical forms drift. |
| `src/agentsfleetd/http/handlers/tenant_model_entries_list.zig`; `http/route_table_invoke.zig` | CREATE/EDIT | Pagination pushed the 4-endpoint handler past the 350-line cap (RULE FLL). Split by question: this file owns asking for entries, the other owns changing them. |
| `src/agentsfleetd/state/secret_reference_txn.zig` | CREATE | The one lock protocol Dimension 1.2 requires. Spelling it at each of five call sites is how one call site eventually spells it backwards, and reversed lock order is a deadlock rather than a visible bug. |
| `src/agentsfleetd/errors/error_entries.zig`; `errors/gen_error_codes.zig` | EDIT | Registering `UZ-LIBRARY-001..008` and its public-docs category. The registry rejects an unregistered code at compile time. |
| `src/agentsfleetd/fleet_runtime/approval_gate_constants.zig` + 11 call sites (`fleet_runtime/approval_gate*.zig`, `fleet_runtime/config_gates.zig`, `fleet/approval_gate.zig`, `http/handlers/webhooks/approval.zig`, and their tests) | CREATE/EDIT | `error_registry.zig` sat exactly on the 350-line cap, and the ERROR REGISTRY harness requires every code to be declared in that file. Its last 19 lines were Redis prefixes, gate timeouts, and event names — no error codes. Moving them was the only way to register the namespace; five of these files imported the registry ONLY for those constants. |
| `src/agentsfleetd/secrets/crypto_store_test.zig`; `http/webhook_test_fixtures.zig`; `http/handlers/fleets/secret_list.zig`; `tests.zig` | EDIT | Call sites of the changed `crypto_store.store` signature and the moved projection module. Fixtures route through `vault.storeJsonPlaintext` so seeded rows carry the same projection production writes (RULE ITF). |
| `public/openapi/paths/tenant-models.yaml` | EDIT | The listed `paths/models.yaml` is the **global** catalogue; the tenant registry §1 actually pages lives in this file. Documents `limit`, `starting_after`, `total`, `next_cursor`, and the 400 codes. |

### Files Changed — amendments (test-infra isolation)

Outside §1's blast radius by construction: none of it is product code. It is recorded here because the integration lane is how every Dimension in this spec is graded, and the lane was returning verdicts that were not about the code under test.

| File | Action | Why it was required |
|------|--------|---------------------|
| `docker-compose.yml` | EDIT | Dropped `container_name` from the three test-infra services so Compose namespaces them per worktree. Three worktrees shared one Postgres/Redis, and the lane begins by dropping every schema, so concurrent agents destroyed each other's runs. Host ports are now per-worktree but **fixed** — see the next row for why the first attempt (Docker-assigned ephemeral ports) was wrong. |
| `scripts/test-infra-ports.sh` | CREATE | Derives three stable host ports from a hash of the Compose project name and **prints** them; `make/test-integration.mk` exports them into the environment. It deliberately writes no `.env` — an earlier version did, and in Continuous Integration (CI) the make target runs as root inside a container, so the `.env` it produced was unreadable by the host runner and moved the published ports out from under connection strings the workflow had already pinned. **Consequence to know:** the export exists only inside `make`, and `docker-compose.yml` defaults to the conventional `5432/6379/8080`. A bare `docker compose up` in a worktree therefore binds the SHARED ports — and run after `make`, it recreates the containers onto them, because the config differs. Bring test infra up through `make` only. |
| `make/test-integration.mk` | EDIT | Deleted the stale-container sweep (it force-removed containers by fixed name — the mechanism that destroyed sibling worktrees' runs); discovers the live port via `docker compose port`; un-silenced the Redis CA copy and compares container-vs-local sha256 so a stale cert fails loudly instead of as dozens of unrelated TLS failures; runs the port script before `docker compose up`. |
| `make/test.mk` | EDIT | Zig's **global** cache default moved off `$(CURDIR)` to a machine-shared path. Per-worktree global caches meant four checkouts each recompiling the same dependency graph; the local cache stays per-worktree. |
| `make/quality.mk` | EDIT | `_fmt_check` was `find … -exec zig fmt --check {} \;` — one compiler process per file (800+), and `find` exits 0 whatever `-exec` returns, so the gate could not fail. `zig fmt --check src` is one process with a real exit code (72s → <1s; `lint-zig` 87s → 41s). Also removed a 14-entry allowlist that could no longer match any path, and collapsed five 1:1 target forwards. |
| `src/agentsfleetd/observability/otel_traces.zig`; `observability/semconv.zig` | EDIT | Genuinely unformatted on `main`. Only reachable once the formatting gate above could fail at all. |
| `scripts/test-infra-ports.sh`; `make/test.mk` | CREATE/EDIT | Per-worktree host ports must be FIXED, not Docker-assigned: an ephemeral published port moves on every container restart while the Makefile resolves it into a URL once, so a restart mid-run pointed the suite at a dead port (see Discovery). Only a LINKED worktree is pinned — a primary checkout, which is what Continuous Integration (CI) always has, keeps 5432/6379/8080, because the workflow hardcodes those into the connection strings it hands the test container. `test.mk` shares Zig's global cache across worktrees. |

### Files Changed — amendments (§1 test tier)

| File | Action | Why it was required |
|------|--------|---------------------|
| `src/agentsfleetd/state/tenant_model_entries_integration_test.zig` | CREATE | Dimension 1.1's named test. The spec's Files Changed listed only `state/tenant_model_entries_integration_test.zig` under a combined row; it did not exist. Order, cursor-walk and zero-decrypt are database-observable claims, so they cannot live in the unit tier. |
| `src/agentsfleetd/state/secret_reference_txn_integration_test.zig` | CREATE | Dimension 1.2's named test. Needs two real connections — the protocol's claim is entirely about what a second session observes while the first holds a row lock, which one connection cannot express. |
| `src/agentsfleetd/http/handlers/library/library_cursor_test.zig` | CREATE | Named unit test `test_library_cursor_codec_roundtrip`, already listed in the original table. |
| `src/agentsfleetd/http/pagination.zig`; `handlers/tenant_model_entries_list.zig` | EDIT | Extracted `identityMatches` so the `UZ-LIBRARY-002` rule is unit-testable without an HTTP context; the handler now calls it instead of spelling the comparison inline, so the test covers the real rule rather than a copy of it. |

### Files Changed — amendments (§3 Dimension 3.2 instrumentation)

| File | Action | Why it was required |
|------|--------|---------------------|
| `src/agentsfleetd/http/library_read_bounds_integration_test.zig` | CREATE | Dimension 3.2's named test. It spans four API paths that live in four different handler files, so it belongs to none of them; the original table named no home for it. Only the tenant registry row is asserted — see the Dimension. |
| `src/agentsfleetd/http/response_size.zig` | CREATE | §3 requires a typed refusal when a body would exceed its ceiling, which means knowing the encoded size BEFORE the body exists. The routine already existed, private, inside `handlers/sensitive_response.zig`; it is now shared rather than copied, and that file is its second caller instead of its only one. §§2–3's three remaining ceilings need the same measurement. |
| `src/agentsfleetd/db/pg_query.zig` | EDIT | Feeds the statement tally. Every row-returning query in the process passes through `PgQuery.from` — the pg-drain gate guarantees it — so a helper added to a read path later is counted without anyone remembering to count it. A hand-placed counter measures the author's memory of the call graph, not the call graph. |
| `src/agentsfleetd/observability/library_read_counters.zig` | EDIT | Added the measured window (`beginRead`/`endRead`). The global hook above needs a scope, and the scope §3 states is "after middleware auth" — which is precisely handler entry. |
| `src/agentsfleetd/http/handlers/tenant_model_entries_list.zig` | EDIT | Opens the window, counts the connection/results/bytes, and enforces the 512 KiB ceiling with `UZ-LIBRARY-005`. The response half moved into `respond` — the ceiling check belongs beside the serialization whose size it governs, and the handler was at the function-length limit. |
| `src/agentsfleetd/http/handlers/sensitive_response.zig` | EDIT | Delegates to `response_size.zig` and names its serialization options, since the size and the write must use the same ones. |
| `src/agentsfleetd/types/model_identity.zig` | CREATE | `MODEL_ID_MAX` / `PROVIDER_MAX`, shared by the admin catalogue and tenant registry write paths. The catalogue bounded `model_id` at 256; the registry checked only non-emptiness, so the same field had two different rules. Not a new policy — the existing one, applied to the door that was missing it. |
| `src/agentsfleetd/http/handlers/tenant_model_entries.zig`; `handlers/admin/model_library_admin.zig` | EDIT | Enforce the bound on POST and PATCH (one `modelIdRejected` helper, so a verb cannot be missed); the admin handler now imports the shared constants instead of holding private copies. |

### Files Changed — amendments (memleak-lane determinism)

Outside the original blast radius: neither file is product code, but the memleak
lane is a P0 acceptance row (S2) and it was red on this branch about half the
time. Recorded per the amendment rule; the full verdict chain is in §Discovery.

| File | Action | Why it was required |
|------|--------|---------------------|
| `src/agentsfleetd/observability/otlp/Client_test.zig`; `src/agentsfleetd/queue/redis_subscriber_test.zig` | EDIT | The only two tests that dialled loopback on a test-local multi-threaded `std.Io.Threaded`. Zig 0.16's `HostName.connect` internally spawns `io.async` futures whose await parks on a stack futex word the worker can wake after the frame pops — the exact finding `make/bench.mk` deliberately refuses to suppress. On `common.globalIo()` the async runs inline, so the race is unrepresentable rather than unlikely. Also removes the otlp 200-test's valgrind skip: its stated reason (worker-spawn thread-local storage blocks) does not exist on the serial io. |

### Files Changed — amendments (§§2–4 test tier and the RULE FLL splits it forced)

| File | Action | Why it was required |
|------|--------|---------------------|
| `src/agentsfleetd/http/handlers/model_library_page_integration_test.zig` | CREATE | Dimension 2.1's named test — the normalized keyset (order, tie, exclusive resume, filters, literal LIKE wildcards), every §Error Contracts 400, and the conditional read (`ETag`/`Cache-Control`/`Vary` on BOTH answers, strong/weak/`*` → bodyless 304, non-match → 200). Two fixture rows share a `model_id` and differ only by provider: the vendor tiebreak is unreachable until the display key ties, so distinct ids would pass against a single-key sort. |
| `src/agentsfleetd/http/handlers/library/gallery_keyset_integration_test.zig` | CREATE | Dimension 3.1's named test — the merged gallery's three-part order and the 401/403/404 ladder. Three rows share one `created_at` and two of those also share a tier, so all three comparisons are reached; `limit=2` puts the page boundary mid-tie so resuming exercises the id clause specifically. A foreign-workspace row with the same timestamp and tier proves the tenant arm's scoping is the `WHERE` rather than an ordering accident, and the same `UZ-LIBRARY-007` is asserted for absent AND foreign so the non-enumeration property is tested rather than assumed. |
| `src/agentsfleetd/http/library_body_ceiling_integration_test.zig`; `http/library_bounds_test_fixtures.zig` | CREATE | `library_read_bounds_integration_test.zig` was at 370 lines — over the 350 cap, but invisible to RULE FLL, which only inspects CHANGED files. Correcting `EXPECTED_STATEMENTS` to the measured 6 touched it and the gate fired immediately. Split by question rather than by line count: the original keeps what a compliant read COSTS, the new file takes what an over-ceiling response DOES (`UZ-LIBRARY-005`) plus the `model_id` write bound, and the shared seed moves to a fixture module so two suites cannot drift into testing different pages. |
| `src/agentsfleetd/state/model_rate_batch.zig` | CREATE | The batch rate read, which pushed `model_rate_cache.zig` to 362 lines. Moving it out is not only the FLL fix: the module deliberately populates no cache, and having no cache in scope makes that structural instead of a promise in a comment. |

### Files Changed — amendments (§§2–4 implementation)

| File | Action | Why it was required |
|------|--------|---------------------|
| `src/agentsfleetd/http/handlers/model_library_page.zig`; `handlers/model_library.zig`; `state/model_library_cache.zig`; `state/model_library_cache_test.zig` | EDIT | The catalogue body was allocated from `hx.alloc` — the per-dispatch arena `server.zig` `deinit`s when `dispatchMatchedRoute` returns, which is BEFORE httpz writes the response (`worker.zig` resets `res_state`/`req_arena` in the keepalive loop, between requests). Every 200 served freed memory: scrubbed bytes on a small body, an unmapped page and a `writev` EFAULT on a large one. `Cache.fetch` additionally duped into the cache's own process-lifetime allocator, leaking a copy per hit; it now copies into a caller-supplied allocator. See §Discovery. |
| `ui/packages/app/lib/api/tenant_model_entries.ts`; `lib/api/tenant_model_entries.test.ts` | EDIT | The §1 half of this workstream gave `GET /v1/tenants/me/models` a 50-row default, and this client sent no `limit` and ignored `next_cursor` — so the dashboard's Models page silently stopped rendering a tenant's entries past the first page the moment #558 merged. The original table lists five UI paths; this client is not among them because §1's pagination was drafted as an API-only change. |
| `src/lib/common/cache_table.zig`; `common/cache_table_test.zig` | CREATE | §2's response cache is built on the shared fixed-capacity primitive instead of a bespoke intrusive least-recently-used list, by owner decision (see Discovery). Landing it here rather than importing it is the sequencing that decision required. |
| `src/lib/common/sync.zig`; `common/constants.zig`; `src/lib/tests.zig` | EDIT | `RwLock` (the cache's reads take a shared lock), the `common` module re-exports the table is reached through, and its test-root registration. |
| `src/agentsfleetd/http/handlers/library/catalogue_key.zig` | CREATE | §2's two derived identities — the page cursor and the HMAC cache key — in one testable place. §4 forbids raw selectors in observable cache keys, so the derivation is a security-shaped routine that deserves its own tests rather than living inline in a handler. |
| `src/agentsfleetd/http/handlers/model_library_page.zig` | CREATE | Paging pushed `model_library.zig` past the 350-line cap (RULE FLL). Split by the same question §1 used for `tenant_model_entries_list.zig`: the handler owns ASKING for a page, this file owns PRODUCING one. |
| `src/agentsfleetd/cmd/serve_caches.zig` | CREATE | `serve.zig` was at 347 of its 350-line cap, so it has no room for a construct/teardown pair. Process-lifetime caches get the seam `serve_r2.zig` / `serve_secrets.zig` / `serve_background.zig` already occupy, and every future cache now costs `serve.zig` nothing. |
| `src/agentsfleetd/cmd/serve.zig`; `http/handlers/common.zig`; `http/route_table_invoke.zig` | EDIT | Boot-wiring the response cache onto `Context` (optional — a cache's absence must change speed, never an answer), and passing `req` to the catalogue handler so it can read query parameters and `If-None-Match`. |
| `src/lib/common/clock.zig` | EDIT | `monotonicMillis`. §2 requires the cache's freshness bound be monotonic, and this module was wall-clock only; a wall clock stepping backwards resurrects an expired entry and stepping forwards expires a live one. |

### Files Changed — amendments (§2.2 rate cache on `common.CacheTable`)

| File | Action | Why it was required |
|------|--------|---------------------|
| `src/agentsfleetd/state/model_rate_cache.zig`; `state/model_rate_cache_key_test.zig`; `state/model_rate_cache_integration_test.zig` | EDIT | Owner decision (see Discovery): the rate cache moves onto the shared primitive. The snapshot hash map, its arena, `populate`, and the whole-cache scan in `contextCapForModel` are gone; the tests move with them — the key test now asserts the collision rule directly, and the leak soak audits key release on eviction instead of arena swap. |
| `src/agentsfleetd/state/model_catalogue_revision.zig` | EDIT | `Txn` / `beginMutation` — the generation lock-mutate-bump-commit protocol. `bumpLocked` existed but had **no production caller**; see Discovery. |
| `src/agentsfleetd/state/model_library/sql.zig` | EDIT | `LOAD_RATE_WITH_REVISION` (one statement, one snapshot, generation + row) and `MIN_CONTEXT_CAP_FOR_MODEL`. `LIST_RATES_FOR_CACHE` deleted with `populate`. |
| `src/agentsfleetd/state/tenant_billing.zig`; `fleet/renewal.zig`; `fleet/service_renew.zig`; `fleet/service_report.zig`; `fleet_runtime/metering.zig` | EDIT | Threading the caller's connection to the charge path, and splitting rate resolution from the pure slice arithmetic so the catalogue-free branches stay provable without a database. |
| `src/agentsfleetd/http/handlers/admin/model_library_admin.zig` | EDIT | All three mutations now run inside the generation transaction. They previously ran in autocommit and never bumped. |
| `src/agentsfleetd/http/handlers/tenant_provider.zig`; `handlers/tenant_model_entries_view.zig` | EDIT | Activation validates against the database (an evicted entry must not reject a valid model); the registry page keeps a resident-only reader so §3's statement budget is untouched. |
| `src/agentsfleetd/cmd/serve.zig` | EDIT | Boot warm removed — see Discovery. Frees 9 lines on a file that sat at exactly 350/350. |
| `src/lib/common/constants.zig` | EDIT | Re-exports `NEVER_EXPIRES`; its comment named this module as the caller that would earn it. |
| `src/agentsfleetd/errors/internal_op_error_sweep_test.zig` | EDIT | §2's catalogue page added 2 plain-English call sites. Baseline set to the MEASURED count (84), not bumped to 87 — #559 removed 4 sites without lowering it, and that slack is what the additions would otherwise have hidden behind. |
| `src/agentsfleetd/db/test_fixtures_provider.zig`; `http/secrets_json_integration_test.zig`; `fleet/service_token_splits_wire_test.zig`; `state/tenant_billing_edge_test.zig`; `fleet_runtime/metering_edge_test.zig`; `http/handlers/admin/model_library_admin_integration_test.zig` | EDIT | Call sites of the changed signatures. The fixtures' cache warm is deleted outright: rates load on demand, so seeding a row is now sufficient on its own. |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | §4.2 and §10 asserted the retired shape in four places ("never makes a … database call on the hot path", "re-populated at boot and after admin mutations"). |

### Files Changed — amendments (§§2–4 route registration, the conditional read, and two more RULE FLL splits)

Closes rubric R3 against the final diff: every path below is one the earlier
tables did not name. Recorded per the amendment rule above.

| File | Action | Why it was required |
|------|--------|---------------------|
| `src/agentsfleetd/http/etag.zig` | EDIT | Dimension 2.1's conditional read. The module carried only `If-Match` — the STRONG comparison, correct for writes. `If-None-Match` takes the WEAK one (RFC 9110 §8.8.3.2), so `ifNoneMatch`, `matchesIfNoneMatch` and `*` handling are added rather than the existing predicate reused: a revalidating cache holding `W/"x"` must still be told 304 when the current tag is `"x"`, while a write against a merely-equivalent representation must still be refused. |
| `src/agentsfleetd/http/handlers/library/api.zig` | EDIT | Re-exports `innerGalleryDetail`. The package façade is the only way the route table reaches a library handler, so a handler absent from it cannot be registered at all. |
| `src/agentsfleetd/http/route_admission.zig`; `route_template.zig`; `route_trace.zig` | EDIT | The three per-route tables the new `workspace_fleet_library_detail` variant must appear in — admission class, metric template, trace traits. Each switches exhaustively over `router.Route`, so the variant does not compile until all three name it; the template deliberately collapses BOTH path parameters, because spelling `{tier}` out would make the label's cardinality a function of the tier list rather than of the route. |
| `src/agentsfleetd/http/handlers/library/library_sink_scan_test.zig` | CREATE | Dimension 4.1's forbidden-egress half — the log/trace/metric/cache-key scan. `library_sink_policy_test.zig` proves the allowed field set structurally over the struct definitions; it cannot prove that nothing else reaches a sink. Registering a capture sink and scanning every emitted line for the seeded secret values is what covers the other direction. |
| `src/agentsfleetd/http/handlers/tenant_provider_cap.zig` | CREATE | §2.2 made activation validate the context cap against the database instead of the cache, which pushed `tenant_provider.zig` past the 350-line cap (RULE FLL). Split by question: that file owns the GET/PUT/DELETE request shape, this one owns "what context window does this (provider, model) get, and is the model catalogued at all" — the `UZ-PROVIDER-004` gate. |
| `src/agentsfleetd/state/tenant_billing_rates.zig` | CREATE | Threading the caller's connection to the charge path pushed `tenant_billing.zig` past the same cap. Split by question: the ledger keeps balance, grants, debits, exhaustion and the rate CONSTANTS — which `audits/cross-tier-rates.sh` pins to that exact path across four files — and this module turns a `(provider, model, posture, elapsed, tokens)` tuple into a number. The dependency runs one way, so the split cannot become a cycle. |
| `src/agentsfleetd/fleet/concurrency_renew_test.zig`; `fleet/renewal_metering_test.zig` | EDIT | Call sites of `SliceRates` / `sliceCharge`, which moved with the split above. Import and qualifier only; every asserted number is unchanged. |
| `ui/packages/app/tests/install-flow.test.ts`; `tests/fleets-install-states.test.ts`; `tests/fleets-install-flow.test.ts` | EDIT | Drop `support_files` from the gallery-summary fixtures. §3's amended summary sheds exactly that field, so a fixture still carrying it asserts a shape the API no longer returns (RULE ITF). |

### Files Changed — amendments (the requirement caps §Discovery flagged)

The write paths this brings into scope were called out as OUTSIDE the original
blast radius when the gap was flagged. They are in scope now — see §Discovery
*"The requirement caps, and why they are not a deferral"*.

| File | Action | Why it was required |
|------|--------|---------------------|
| `src/agentsfleetd/fleet_library/requirement_limits.zig`; `fleet_library/requirement_limits_test.zig` | CREATE | The count and length ceilings on `required_credentials`, `required_tools`, `network_hosts`, and `required_credentials_reasons`, in one module because TWO doors write them and both share one rule about what a credential name may be. `types/model_identity.zig` is the precedent, for the same reason: the alternative is the same field carrying two different rules. The numbers multiply out to a ~35 KB ceiling on the encoded `requirements` blob, which is what makes §3's per-item projection bounded by construction rather than by observation. |
| `src/agentsfleetd/fleet_library/importer.zig` | EDIT | Applies the ceilings to the lists parsed out of a bundle's `TRIGGER.md`, collapsed onto the existing `TooLarge` → 413 answer every other size cap on that path already gives. |
| `src/agentsfleetd/http/handlers/library/catalog_patch.zig` | EDIT | Bounds the curate path's `required_credentials_reasons` — entry count, key length, and copy length. It previously validated only that the value was an object of strings, so the text itself was unbounded. |
| `src/agentsfleetd/http/handlers/library/catalog_patch_integration_test.zig` | EDIT | The two refusals above, driven through the real PATCH route. `requirement_limits_test.zig` proves the predicate; this proves the wiring. The over-long case also patches a LEGAL reason on the same door and expects 200, so the refusal is pinned to the length rule rather than to the field being rejected outright — a validator that refused every reasons map would otherwise satisfy the negative test on its own. |

### Files Changed — amendments (the `origin/main` merge and the release bump)

| File | Action | Why it was required |
|------|--------|---------------------|
| `src/agentsfleetd/integration_tests.zig` | EDIT | `origin/main` (#562) split the daemon test root — `tests.zig` keeps unit tests and prod modules, this file owns every `*_integration_test.zig`, and a lane gate asserts the two stay disjoint. §§2–4's four new integration suites (`library_body_ceiling`, `library_page_bounds`, `model_library_page`, `gallery_keyset`) move here so the integration lane actually runs them; leaving them in the unit root would have made them skip for want of a database and turned four graded Dimensions into vacuous passes. `library_sink_scan_test.zig` deliberately stays in the unit root — it is a static source scan and needs no live service. |
| `ui/packages/app/tests/fleet-library-api.test.ts` | EDIT | The paged gallery client's walk. Its unbounded-walk guard had no test, so `lib/api/fleet-library.ts` sat at 88% statements and 50% branches and pulled the app's global coverage under its 100% threshold — `make test-unit-all` (S1) was red on exactly that. One test per half: the resume asserts the FIRST request carries no `starting_after` and the second carries the cursor the first returned (a client paging from the wrong cursor still produces two pages of rows, so row count alone would not catch it), and the bound asserts both the throw and that `fetch` ran exactly `GALLERY_MAX_PAGES` times, so "bounded" is checked rather than "eventually stops". |
| `VERSION`; `build.zig.zon`; `cli/package.json` | EDIT | 0.22.1 → 0.23.0 via `make sync-version`. The release template's matrix puts a feature milestone at a minor bump, and pre-`1.0` breaking changes at a minor bump as well; this workstream is both, since `GET /v1/models` and the workspace gallery each change from an unbounded list to a bounded page. `cli.js` reads the version at runtime and needs no edit. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — GRD, VLT, FLS, CNX, WAUTH, RTM, FLL, UFS, ITF, TNM, NDC, NLR, NLG, ORP.
- **`dispatch/write_zig.md`, `dispatch/write_any.md`** — ownership, drains, transactions, shape, constants, Linux builds.
- **`docs/REST_API_DESIGN_GUIDELINES.md` §§1–8, 10–12; `docs/AUTH.md`** — keysets, RFC 7807, routing, authorization, OpenAPI, secrets.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | focused modules, drains, allocator tests, both Linux builds |
| PUB / Struct-Shape | yes | PLAN records page/cursor/cache shapes |
| File & Function Length | yes | split route matcher/cache/detail; do not grow `router.zig` |
| UFS | yes | constants for limits, versions, tiers, normalization, headers |
| UI Substitution / DESIGN TOKEN | no | data clients only |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | every code in §Error Contracts registered in `errors/error_registry.zig` and reflected in generated OpenAPI; redacted sinks; transaction cleanup; migration embedded via `schema/embed.zig` |

## Prior-Art / Reference Implementations

- **API:** Events/Approvals keysets and `docs/REST_API_DESIGN_GUIDELINES.md`.
- **Secrets:** `state/vault.zig::markExisting`; **Fleet identity:** `docs/architecture/fleet_bundles.md`.

## Sections (implementation slices)

### §1 — Tenant registry page and reference transactions

Amend the API guideline before route work to permit opaque compound keysets where a resource ID alone cannot preserve a filtered mixed-key order; retain the standard request name `starting_after`, default 50/max 100, and exact list envelope `{items,total,next_cursor}` with `total:null`. `GET /v1/tenants/me/models?limit=50&starting_after=` orders `created_at DESC, id COLLATE "C" DESC`. Its cursor is unpadded base64url of UTF-8 canonical JSON with fixed key order/types `{"v":1,"created_at":int64,"id":string,"tenant_uuid":uuid,"limit":int}`. Malformed/version/tenant/limit mismatch is 400; a valid stale boundary continues and may return empty. Project only the current page and decrypt each distinct page `secret_ref` once.

**Reads decrypt nothing.** The page shows `secret_ref`, provider, kind, base URL, `has_key`, and presence booleans. Every one of those is metadata; not one is the secret value. Presence therefore comes from a single batch existence query — `vault.markExisting`, one `SELECT key_name FROM vault.secrets WHERE workspace_id = $1 AND key_name = ANY($2)` — not from decrypting each row. `http/handlers/connectors/catalog.zig::innerCatalog` already does exactly this and documents the budget: "exactly 2 (one batch existence query per set) — never the ~2·N sequential decrypting `loadJson` reads the naive shape would do."

This changes the tenant registry page from up to 100 decryptions to **zero**. It is the single largest cost on the path, and removing it also removes the exposure: no ciphertext is loaded, no plaintext is derived, so §4's rule that secret values never leave trusted erased memory holds on reads because nothing is ever read.

**Precondition the implementing agent verifies at PLAN.** This holds only while every displayed field lives in a column rather than inside the encrypted blob. Confirm against the vault payload actually written by `tenant_provider.upsertSelfManaged` and model creation. If a displayed field (base URL is the likely one) is stored inside the blob, do not fall back to per-row decryption — promote that field to its own column in `schema/035`, because a value the API displays to any authorized caller is not a secret and should never have been encrypted. Record the finding in Discovery either way.

Every reference producer—POST model creation, provider activation, `tenant_provider.upsertSelfManaged`, and `ensureEntry`—and deletion uses one transaction and exact lock order: `vault.secrets(workspace_id,key_name) FOR UPDATE`; matching `core.tenant_model_entries ORDER BY id FOR UPDATE`; `core.tenant_model_selection FOR UPDATE`; validate/mutate; commit. Producer-first makes delete reject; delete-first makes producer observe absence and roll back.

- **Dimension 1.1** — tenant keyset and current-page projection are exact → Test `test_tenant_registry_page_is_bounded` — **DONE.** Keyset, cursor codec, limit bounds, and the zero-decrypt projection are implemented; `state/tenant_model_entries_integration_test.zig` drives `view.buildList` end to end and asserts all three claims. The fixture seeds a deliberate `created_at` TIE, because that is the only case where the `id` tiebreak is load-bearing — distinct timestamps let a single-key sort pass by accident. Decrypts are asserted at exactly zero across a full unpaged walk and a paged one (Invariant 5), and the paged walk is checked for duplicates so an inclusive-instead-of-exclusive boundary cannot hide behind a correct row count. Unit tier: `http/handlers/library/library_cursor_test.zig` (`test_library_cursor_codec_roundtrip`).
- **Dimension 1.2** — all producers and delete serialize safely → Test `test_secret_reference_paths_serialize` — **DONE.** `state/secret_reference_txn.zig` owns the lock order; `state/secret_reference_txn_integration_test.zig` drives **two real connections** against real `FOR UPDATE` locks and asserts both interleavings: delete-first yields `SecretGone` with nothing written, producer-first makes the contender's step-1 lock TIME OUT while the transaction is open and succeed once it commits — with a baseline uncontended acquire first, so a `lock_timeout` firing for an unrelated reason cannot read as success. Also covers the count-taken-under-lock invariant and the platform-principal path that skips steps 2 and 3. Blocking is proven by timeout rather than threads, so the lane cannot hang.

### §2 — Global catalogue generation, search, and caches

The model API has only `q` and provider filters. Normalize `q` with NFKC, trim, whitespace collapse, casefold; normalized empty means absent; reject over 128 UTF-8 bytes. Provider trims ASCII whitespace, lowercases, treats empty as absent, and remains arbitrary catalogue text—an unknown provider is valid and returns no matches. Match a literal substring with escaped SQL LIKE wildcards over normalized `display_name=model_id` and `vendor=provider`. Sort normalized display, normalized vendor, and id ascending, each `COLLATE "C"`. `starting_after` is fixed-key canonical JSON `{"v":1,"display_key":string,"vendor_key":string,"id":string,"q":string|null,"provider":string|null,"limit":int}` encoded as above. Malformed/version/filter/limit mismatch is 400; a stale valid boundary may return empty.

After authentication, every request reads the database catalogue revision before cache selection. A mutation locks the singleton revision row, mutates the catalogue and increments revision in one transaction, and commits.

**The revision belongs in the cache key, and that removes the publish protocol.** An earlier draft also kept a process-published generation and allowed a candidate to publish only when its revision exceeded it, with a rule that concurrent publishers must never replace a newer generation with an older one. That protocol is unnecessary once the revision is part of the key, which it already is. A candidate built from revision N lands under a key containing N. Every later request reads revision N+1 first and looks up a different key, so a stale candidate is unreachable rather than dangerous — it simply ages out under LRU and TTL. Ordering between concurrent publishers stops mattering, and with it goes a class of races that is hard to test and easy to get wrong.

Rollback or commit failure discards the candidate, as before. What is deliberately **not** removed is billing reconciliation: the rate cache is keyed by `(provider, model_id)` rather than by revision, so it cannot use this trick and keeps its explicit reconcile-and-copy under the mutex. Billing uses its existing connection to reconcile revision and atomically copy a rate from the cache generation it observed; rebuild failure fails closed, never bills stale. Revision-read failure returns typed 503 without cached data. The existing top-level catalogue `version` string remains in every 200 page; internal `catalogue_revision` is not exposed.

The response cache key is revision plus an unlogged HMAC-SHA-256 digest, under a process-random key, of canonical q/provider/starting_after/limit selectors; no raw selector or credential metadata enters keys. It holds at most 256 entries and 8 MiB, with a monotonic 60-second TTL.

**Eviction is least-recently-used within a bucket, not globally** — amended from "true LRU" once the storage became the shared `common.CacheTable` primitive (see Discovery). The key is fixed-size, so it is stored inline: capacity is a compile-time slot count, which makes the 256-entry ceiling a property of the type rather than a counter any path could fail to check, and no key byte competes with a payload byte for the budget. What is given up is naming the exact victim under pressure; what is bought is that reads take the table's non-mutating `peek` under a shared lock and stop serializing every catalogue request behind one mutex. A miss costs one rebuild, never a wrong answer.

Byte accounting is defined rather than estimated, because "including allocator metadata" is not observable through a Zig allocator. The cache sums the exact `len` of the payload bytes it owns — the only per-entry allocation left — so the number the ceiling compares against is the number the test reads. Slot storage is preallocated and fixed; allocator-internal padding is outside the budget by construction. Insertion is rejected when admitting an entry would cross either ceiling; rejection is a bypass, never an eviction cascade. Entries already past their deadline are swept before that rejection, because lazy expiry otherwise lets dead payloads hold the budget against live ones. It contains only non-secret model responses; Fleet is never cached. Allocation failure or over-budget responses bypass insertion. A strong ETag hashes exact bytes. Both 200/304 send `ETag`, `Cache-Control: private, no-cache`, `Vary: Authorization`; after auth/revision, `If-None-Match: *`, exact, or weak list match returns bodyless 304, otherwise 200.

The billing decision linearizes at its revision read: under the rate-cache mutex, reconcile to that exact generation and copy the selected rate before unlock. A later catalogue commit applies only to later revision reads.

Rate-cache identity is a collision-safe structured `(provider,model_id)` key, never delimiter concatenation. Migration tests include provider/model strings containing the current `0x1f` separator and prove distinct tuples cannot alias or select another rate.

- **Dimension 2.1** — normalized search/keyset and headers are exact → Test `test_model_page_and_conditional_headers` — **DONE.** The normalization half is implemented and unit-tested (`http/handlers/library/query.zig`, `library_query_normalization_test.zig`): trim, whitespace collapse, the 128-byte bound, UTF-8 validation, and LIKE-wildcard escaping. Per the Discovery amendment, NFKC and casefold are SQL-side (`lower(normalize(col, NFKC))`), so Zig holds only the ASCII-safe half. The keyset/cursor wiring, the SQL comparison, and the conditional read are now implemented and the named test is written: `http/handlers/model_library_page_integration_test.zig` drives the real route for the order (with a display-key TIE, because the vendor comparison is unreachable until the first key ties), the exclusive resume, both filters, literal `%` matching, every §Error Contracts 400 with no unpaged fallback, and the conditional half — `ETag`/`Cache-Control`/`Vary` on BOTH answers, strong/weak/`*` each yielding a bodyless 304, and a non-matching tag yielding 200 rather than a wrong 304. The cache is deliberately absent under the harness for this dimension: a 304 is computed from the body's own tag, so it must hold identically with and without one.
- **Dimension 2.2** — response and billing caches converge or fail closed → Test `test_catalogue_revision_governs_both_caches` — **DONE.** `schema/037_model_catalogue_revision.sql` adds the singleton generation. `state/model_catalogue_revision.zig` gives the hot-path read (no lock), the mutation lock/bump (`FOR UPDATE`), and `Txn`/`beginMutation` — the lock-mutate-bump-commit protocol the three admin mutations now run inside. They previously ran in **autocommit and never bumped**, so the generation never left 0 and no catalogue mutation ever invalidated the response cache; `bumpLocked` had no production caller at all (§Discovery). `state/model_library_cache.zig` is the revision-keyed response cache with defined byte accounting, the 256-entry / 8 MiB ceilings, monotonic 60s TTL and over-budget bypass (`model_library_cache_test.zig`). `state/model_rate_cache.zig` is rebuilt on `common.CacheTable` by owner decision: the `(provider, model)` key stays two byte-compared fields so `0x1f` aliasing remains unrepresentable (`model_rate_cache_key_test.zig` asserts the policy directly), and the generation rides in the value, so a charge accepts an entry only at the generation it observed or later and otherwise reloads the row. That replaces the reconcile-and-copy protocol §2 specified — see §Discovery for why its premise no longer holds, and for how "fail closed" resolves differently at the charge path, the lease gate, and activation. The named integration test now covers both halves: publish-after-commit, rollback invisibility, two mutations cannot share a generation, **and** the billing half — a replica that never saw the mutation cannot bill the old rate, an uncommitted price change is never billed, and a deleted model reads as null rather than as its last known price.

### §3 — Fleet keyset, detail, and measured ceilings

Fleet `q` uses the same normalization, 128-byte maximum, empty-as-absent, and escaped literal substring matching only id/name/description. Set `tier_rank`: platform=0, tenant=1; order `created_at DESC, tier_rank ASC, id COLLATE "C" DESC`; seek exactly `created_at<c.created_at OR (created_at=c.created_at AND tier_rank>c.rank) OR (created_at=c.created_at AND tier_rank=c.rank AND id COLLATE "C"<c.id)`. `starting_after` is fixed-key canonical JSON `{"v":1,"created_at":int64,"tier_rank":0|1,"id":string,"workspace_uuid":uuid,"q":string|null,"limit":int}` encoded as above. Malformed/version/filter/workspace/limit mismatch is 400; stale valid boundaries may end empty. Foreign detail is 404 after workspace auth: unauthenticated 401, workspace access 403.

Measured application-data maxima after middleware auth are:

| API path | DB statements | Decryptions | Results | Encoded body | Connections |
|---|---:|---:|---:|---:|---:|
| tenant registry page | ≤6 | **0** | ≤100 | ≤512 KiB | 1 |
| global models cache hit / miss | ≤1 / ≤2 | 0 | ≤100 | ≤256 KiB | 1 |
| Fleet summary | ≤3 | 0 | ≤100 | ≤512 KiB | 1 |
| Fleet detail | ≤3 | 0 | 1 | ≤1 MiB | 1 |

The two Fleet rows were drafted at ≤1 and ≤2 and are corrected to the measurement (§Discovery). Both omitted `common.authorizeWorkspace`, which costs two statements and runs inside the window: the bearer chain authenticates, but only the handler knows which workspace the path names, so authorization is the handler's and `beginRead()` opens before it. Each read itself is one statement — the summary's merged `UNION ALL`, the detail's single select — and that is the number `limit` cannot move.

Projection returns `UZ-LIBRARY-005` (500) if encoding would exceed its ceiling; it never truncates. With `limit` capped at 100 and per-item projection bounded, the ceiling is unreachable in normal operation, so a production firing is a defect rather than a user-facing outcome.

- **Dimension 3.1** — Fleet matching, seek, identity, and foreign detail status are exact → Test `test_fleet_keyset_and_detail_status` — **DONE.** The ordering half is implemented and unit-tested: `http/handlers/library/fleet_keyset.zig` owns the `created_at DESC, tier_rank ASC, id DESC` order and the seek predicate that resumes it, with `library_keyset_test.zig` exercising every tie combination (the three comparisons are each unreachable until the keys before them tie, so distinct fixtures hide two of the three possible direction errors). Route matchers land in `http/route_matchers_library.zig` + `route_matchers_library_test.zig`. The merged two-table query now lives in `fleet_library/gallery_sql.zig` — ONE statement over a `UNION ALL` of both libraries, because a keyset boundary has to be resolvable against the combined sequence and neither half knows where the other's rows fall, so two independently paged reads cannot produce a resumable total order at all. The predecessor read both tables unbounded and concatenated them in Zig. `handlers/library/gallery.zig` is the paged collection and sheds `requirements`, `support_files` and `required_credentials_reasons` to the detail route: those are JSONB blobs with no per-row bound, so leaving them on the card made page size a function of content rather than of `limit`. `handlers/library/gallery_detail.zig` is the single-entry read, and the detail route is registered end to end (`routes.zig` union → `router.zig` → `route_scopes.zig` → `route_table.zig` → `route_table_invoke_library.zig`). The 401/403/404 ladder: 401 is the middleware's, 403 is `authorizeWorkspace`, and absent-or-foreign is one `UZ-LIBRARY-007` — the tenant query is scoped by `workspace_id`, so a foreign row returns no rows and takes the identical path an absent one does, which makes the non-enumeration property structural rather than a thing the handler remembers. The named test is now written: `http/handlers/library/gallery_keyset_integration_test.zig` drives the real routes. Its fixtures TIE deliberately — three rows share one `created_at` and two of those also share a tier — because each comparison in `created_at DESC, tier_rank ASC, id DESC` is dead code until the one before it ties, and the three directions are not the same. `limit=2` places the page boundary mid-tie so resuming exercises the id clause specifically, and the absent rows are named rather than counted, because an inclusive seek repeats a row while leaving the page full. A foreign-workspace row sharing the same timestamp and tier proves the tenant arm's scoping is its `WHERE` and not an ordering accident, and the identical `UZ-LIBRARY-007` is asserted for absent AND foreign so non-enumeration is tested rather than assumed.
- **Dimension 3.2** — every path stays within the numeric table → Test `test_library_read_resource_bounds` — **DONE.** All four rows of the table are measured and asserted. `observability/library_read_counters.zig` owns the tallies and the maxima, and gained a measured WINDOW (`beginRead`/`endRead`) so the statement tally could move to `db/pg_query.zig` — the one point every row-returning query passes through. Counting there rather than in the handler is what makes the budget a claim about the whole call graph instead of about the statements an author remembered to count; the window opens at handler entry, which is exactly §3's "after middleware auth" boundary. `http/library_read_bounds_integration_test.zig` drives the real HTTP route (two of the five columns — connections and encoded body — do not exist below the handler) and pins **5 statements, 0 decryptions, results bounded by `limit`, 1 connection, and `encoded_bytes` equal to the bytes the client actually received**. The empty page is pinned separately at 4, because `vault.loadMetadata` returns before querying when there are no rows to describe, and a "still under budget" assertion would not notice that guard being deleted. `http/response_size.zig` measures the encoded body before it exists, so `tenant_model_entries_list.zig` now refuses an over-ceiling page with `UZ-LIBRARY-005` instead of truncating. The remaining three rows now exist too, in `http/library_page_bounds_integration_test.zig` — a separate file because the first is at its 350-line cap, split by resource rather than by line count. The global-models row asserts BOTH generations of its budget: two statements on a miss, and one on a hit, which required wiring a real `model_library_cache.Cache` onto `h.ctx` (the harness leaves it null, so no other test exercises the cache-hit path at all). A hit is one statement and never zero, because the revision is read before cache selection on every request — asserting zero there would be asserting the generation check had been skipped. The two Fleet rows are corrected to 3 apiece against the measurement, and their `expectCommon` helper asserts the zero-decrypt invariant on every path rather than only the one that historically decrypted.

### §4 — Metadata sinks and synchronized surfaces

Secret/credential metadata carried by authenticated HTTP/UI is limited to canonical `secret_ref`, provider, kind, base URL, `has_key`, required/failing credential names, and presence booleans. Non-secret model page fields and the exact Fleet summary/detail fields in §Interfaces are also permitted. Neither field set may enter logs, traces, metrics, analytics, observable cache keys, or benchmark artifacts. Encrypted ciphertext may persist only in the vault; secret values/API keys never leave securely erased trusted Zig memory. Update routing, OpenAPI, CLI, public inventories, architecture, and consumers atomically; `make check-openapi` is the OpenAPI command.

`route_matchers_library.zig` exports `matchWorkspaceFleetLibraries(Path) ?workspace_id` for the three-segment collection and `matchWorkspaceFleetLibraryDetail(Path) ?{workspace_id,tier,id}` for the five-segment detail; router checks detail before collection. Admin catalogue matching remains in its existing owner. `route_matchers_test.zig` pins segment counts, tier enum, encoded IDs, methods, and near misses.

- **Dimension 4.1** — allowed HTTP metadata and forbidden sinks are enforced → Test `test_library_secret_and_metadata_sink_policy` — **DONE.** `http/handlers/library/library_sink_policy_test.zig` enforces the allowed field set structurally, over the struct definitions via `@typeInfo`, rather than by driving one request with one sentinel — the latter catches a leak only on the path a test happens to exercise, with the value it happens to choose. Both a deny list (secret-shaped names) and an allow list (any new credential-derived field must be classified deliberately) apply, because a deny list alone misses a field called `credential_blob`. Covers `EntryView` and the write-time `metadata.Projection`, and self-tests its own matcher so a broken guard cannot pass silently. The forbidden-egress half now lands in `http/handlers/library/library_sink_scan_test.zig`. It is a STATIC scan, deliberately, not a runtime capture: it walks every production `.zig` under `src/` — skipping `_test.zig` files, which plant sentinels on purpose, and itself, which spells every forbidden substring in its own deny list — and rejects a secret-shaped field name appearing in a log, trace, metric, or cache-key call site. A runtime sink capture proves only that the ONE path a test drives is clean with the ONE value it chose; the property §4 states is about every sink in the codebase, and only reading all of them can decide it. Two self-tests keep it from passing vacuously: one asserts the matcher actually matches a planted violation, and one asserts every allow-list entry is either something the deny list would otherwise catch or a §4-permitted metadata field, so an addition has to be argued rather than appended. `api_key_id` is allowed deliberately — it identifies a credential rather than being one, and the two are distinguishable by which can authenticate. Findings print through `std.debug.print` rather than `std.log.err`, because an error-level log fails the test that emits it and would replace a legible list of violations with a crash naming none of them. Needs no live service, so it stays in the unit root.
- **Dimension 4.2** — routes and all published/consumer inventories agree → Test `test_library_operation_surfaces_are_synchronized` — **DONE.** The route half exists: `http/route_matchers_library.zig` exports the collection and detail matchers, validating the tier at the matcher so an unknown value makes the route not exist rather than reaching a handler that would use it as a selector. §4's "router checks detail before collection" is satisfied **structurally** — the shapes differ in segment count, so no path matches both and evaluation order cannot matter; `route_matchers_library_test.zig` asserts that mutual exclusion directly, which is stronger than an ordering rule because a shape difference is a property of the matchers rather than of the call site. The remaining surfaces are now synchronized. Both routes are registered end to end (`routes.zig` → `router.zig` → `route_scopes.zig` → `route_table.zig` → `route_table_invoke_library.zig`, plus the three per-route tables in `route_admission.zig` / `route_template.zig` / `route_trace.zig`). OpenAPI carries both: `paths/fleet-library.yaml` documents the paged gallery and the detail route, and `paths/models.yaml` was rewritten for the paged catalogue — it still described the pre-§2 unbounded `{version,models}` with no query parameters, no `304`, and no conditional headers, which `make check-openapi` cannot catch because a served route being DOCUMENTED is all the coverage gate checks, not whether the document is true. The three public inventories (`llms.txt`, `skill.md`, `agentsfleet-manifest.json`) gain the four read operations with their paging and conditional-read notes; the manifest's `policyClasses` is kept in exact sync with its `operations` list. The CLI needed one change, not two: `agentsfleet library` reads `/v1/fleets/bundles`, which this workstream does not touch, while `fleet_install.ts` — which resolves an id against the gallery — now follows `next_cursor` to exhaustion, because reading only the first page would report a valid entry as absent.

## Interfaces

All lists use `?starting_after=&limit=50`, unpadded-base64url compound cursors, and `{items,total:null,next_cursor}`. Model pages additionally retain top-level `version:string` as the documented list-envelope exception.

`FleetSummary={tier:"platform"|"tenant",id,name,description,created_at}`.
`FleetDetail={tier,id,name,description,created_at,source_ref,requirements:{credentials,tools,network_hosts,trigger_present},required_credentials_reasons:Record<string,string>,support_files:[{path,size_bytes}],credential_presence:[{name,present}],missing_credentials:string[]}`.
`GET /v1/workspaces/{workspace_uuid}/fleet-libraries/{tier}/{id}` returns that single resource or RFC7807 401/403/404/503.

**Amended (see §Discovery): the summary sheds `support_files` only.** It retains `requirements` and `required_credentials_reasons`, and keeps `visibility` rather than `tier`. Both retained fields are rendered — the card's credential chips and the ConnectGate's per-credential purpose copy — so removing them would delete information at the moment a user decides whether to install. `support_files` is the field with no reader anywhere in `ui/`, `cli/`, or `src/runner/`, and measurement shows it is also the only one that breaches the 512 KiB ceiling (~6.3 KB/row bounded by `MAX_SUPPORT_FILES` × `MAX_SUPPORT_PATH_LEN`, ~630 KB across a full page). Dropping it alone achieves what this clause wanted. Detail still carries all three.

## Error Contracts

New namespace `UZ-LIBRARY-*`. `UZ-CATALOG-*` and `UZ-MODELS-*` are already allocated and are not extended here. Every row is registered in `src/agentsfleetd/errors/error_registry.zig`, which rejects a code lacking the `UZ-` prefix at compile time, and every row appears in the generated OpenAPI error schema.

| Code | Status | Fires when | Notes |
|---|---:|---|---|
| `UZ-LIBRARY-001` | 400 | `starting_after` is malformed, wrong `v`, or not canonical JSON in the fixed key order | Never falls back to an unpaged read |
| `UZ-LIBRARY-002` | 400 | cursor tenant, workspace, filter, or limit disagrees with the request | Identity mismatch, distinct from shape |
| `UZ-LIBRARY-003` | 400 | `limit` outside 1..100, or `q` over 128 UTF-8 bytes after normalization | Both are input bounds |
| `UZ-LIBRARY-004` | 503 | catalogue revision read fails before cache selection | Returns no cached data; billing fails closed |
| `UZ-LIBRARY-005` | 500 | a compliant response would exceed its §3 encoded-body ceiling | Internal invariant breach, never truncation. Unreachable while `limit` ≤ 100 and per-item projection stays bounded; a firing in production is a defect, and `test_projection_failures_are_safe` is what proves the guard refuses rather than truncates |
| `UZ-LIBRARY-006` | 503 | pool acquire or SQL execution fails transiently | Connection released before the response is built |
| `UZ-LIBRARY-007` | 404 | Fleet library detail is absent, or present in another workspace | One code for both, so the response cannot be used to enumerate foreign entries |
| `UZ-LIBRARY-008` | 409 | a secret-reference producer loses the §1 lock race to a concurrent delete | The producer rolls back; the client may retry |

A stale-but-valid cursor is **not** an error: it continues from its boundary and may return an empty page with `next_cursor:null`.

## Failure Modes

Every row is also a Test Specification row. The two tables name the same tests on purpose; neither is a subset of the other.

| Mode | Cause | Injection | Handling | Code | Named test |
|---|---|---|---|---|---|
| Invalid cursor/query | malformed/mismatch/oversize | cursor/query fixtures | 400, no fallback | `UZ-LIBRARY-001/002/003` | `test_invalid_library_inputs` |
| Revision/rebuild failure | DB or candidate fault | failpoint | 503/fail billing closed; no stale publish | `UZ-LIBRARY-004` | `test_catalogue_revision_governs_both_caches` |
| Projection/encoding fault | decrypt/allocation/body ceiling | allocator/envelope fixtures | typed failure, zero/free, no truncation | `UZ-LIBRARY-005` | `test_projection_failures_are_safe` |
| Reference race | producer/delete interleave | transaction barriers | valid serialization; loser rejects/rolls back | `UZ-LIBRARY-008` | `test_secret_reference_paths_serialize` |
| Pool/query fault | acquire/SQL failure | pool/SQL failpoint | typed transient; connection released | `UZ-LIBRARY-006` | `test_library_pool_query_failure` |
| Foreign detail | valid auth, foreign entry | foreign fixture | non-enumerating 404 | `UZ-LIBRARY-007` | `test_fleet_keyset_and_detail_status` |
| Stale boundary | boundary row changes | mutation fixture | continue, possibly empty | none — not an error | `test_stale_library_cursors_continue` |
| Forbidden egress | sentinel reaches sink | sink capture | test rejects emission | n/a — test-time guard | `test_library_secret_and_metadata_sink_policy` |

## Invariants

1. All pages, work, connections, and bodies obey §3 limits; typed builders enforce failure rather than truncation.
2. One catalogue generation governs response and billing caches. The response cache enforces it structurally by carrying the revision in its key; billing enforces it by reconciling under its mutex.
3. One lock order governs all reference producers/deletes; transaction helpers enforce it.
4. Sink-safe metadata types and sentinel scans enforce §4.
5. No library read path decrypts. Presence is a batch existence query; the read-path decryption counter is asserted at exactly zero.
6. The global model response cache is shared across tenants, so its payload must be byte-identical for every authorized caller. Any tenant-varying field entering that payload is a cross-tenant leak, and the cache key carries no tenant to catch it.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| test-only resource counters | **M143_001** | bounded tests run | counts/bytes only | no identifiers or metadata | `test_library_read_resource_bounds` |

**Counter ownership.** This workstream owns the counters, because Dimension 3.2 is its own P0 acceptance row and cannot be graded without them. They are deliberately minimal: monotonic `usize` tallies of statements, decryptions, results, encoded bytes, and connections, incremented only under `builtin.is_test`, read through a reset-per-test handle, and compiled out of production builds. No enum labels, no cardinality surface, no exporter.

M143_003 **consumes and extends** these counters into production telemetry under M139_004 semantics. It does not create them, and this workstream does not wait on it. Stated explicitly because an earlier draft assigned ownership to M143_003 while M143_003 depends on this workstream, which deadlocks any agent reading both.

## Test Specification (tiered)

This table is the complete set. Every row is mandatory, including the unit tier and the failure rows — an agent that implements only the dimension rows ships an incomplete workstream.

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | integration | `test_tenant_registry_page_is_bounded` | exact order/cursor/current-page distinct decrypts |
| 1.2 | integration | `test_secret_reference_paths_serialize` | every producer/delete interleaving is safe |
| 2.1 | integration | `test_model_page_and_conditional_headers` | search/order/cursor and 200/304 headers/body |
| 2.2 | integration | `test_catalogue_revision_governs_both_caches` | publish-after-commit and cross-replica fail-closed billing |
| 3.1 | integration | `test_fleet_keyset_and_detail_status` | exact seek and 401/403/404 behavior |
| 3.2 | integration | `test_library_read_resource_bounds` | exact numeric table and typed body-ceiling failure |
| 4.1 | integration | `test_library_secret_and_metadata_sink_policy` | allowlisted response metadata; all other sinks clean |
| 4.2 | end-to-end | `test_library_operation_surfaces_are_synchronized` | API, OpenAPI, CLI, UI, and public inventories agree |
| 1.1, 2.1, 3.1 | **unit** | `test_library_cursor_codec_roundtrip` | canonical JSON key order/types encode and decode losslessly; every malformed, wrong-version, and identity-mismatch input maps to its exact `UZ-LIBRARY-001`/`002` code |
| 2.1 | **unit** | `test_library_query_normalization` | NFKC, trim, whitespace collapse, casefold; empty-after-normalization is absent; over-128-byte input is `UZ-LIBRARY-003`; LIKE wildcards are escaped so `%` and `_` match literally |
| 3.1 | **unit** | `test_fleet_keyset_seek_predicate` | the three-part `created_at`/`tier_rank`/`id` seek orders correctly across every tie combination, `tier_rank` platform=0 before tenant=1 |
| 2.2 | **unit** | `test_response_cache_accounting_and_lru` | byte accounting per §2, 256-entry and 8 MiB ceilings, bucket-local LRU that never exceeds capacity and retains near-totally below it, 60-second monotonic TTL, over-budget bypass, expired-entry reclamation |
| — | **unit** | `cache_table_test.zig` | `common.CacheTable`: no wrong hits, no expired reads, an expired entry never costs a live one its slot, and every departing entry is released exactly once — proved by a counting spy per path and again against `std.testing.allocator` with an owned-memory value |
| 2.2 | **unit** | `test_rate_cache_key_is_collision_safe` | structured `(provider,model_id)` keys with `0x1f` in either field stay distinct and select no other rate |
| — | integration | `test_invalid_library_inputs` | every §Error Contracts 400 row returns its exact code with no unpaged fallback |
| — | integration | `test_projection_failures_are_safe` | decrypt, allocation, and body-ceiling faults return typed errors, zero and free owned memory, and never truncate |
| — | integration | `test_library_pool_query_failure` | pool acquire and SQL faults return `UZ-LIBRARY-006` with the connection released |
| — | integration | `test_stale_library_cursors_continue` | a boundary row mutated between pages continues and may end empty, never errors |
| — | integration | `test_library_reads_never_decrypt` | the read-path decryption counter is exactly zero across tenant registry, global models, Fleet summary, and Fleet detail; presence still resolves correctly for present, absent, and mixed key sets |
| — | integration | `test_global_cache_payload_is_tenant_invariant` | two different authorized tenants requesting identical selectors at the same revision receive byte-identical payloads, so the tenant-free cache key cannot leak across tenants |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Data/security tests pass | `make test-integration` | exit 0 | P0 | ✅ exit 0. Graded on the post-merge tree, with all five library integration suites registered in `integration_tests.zig` and none in the unit root — the lane gate pins that split, so none of them can silently skip. |
| R2 | OpenAPI and CLI agree | `make check-openapi && make test-unit-cli` | exit 0 | P0 | ✅ exit 0. Bundle + redocly lint + error-schema + URL shape + 78-route coverage green; CLI 1367 pass, 15 skip, 0 fail. |
| R3 | Diff is scoped | `git diff --name-only origin/main` | 0 unlisted paths | P0 | ✅ 0 unlisted across 100 changed paths, after four amendment tables recorded the paths the original blast radius did not name. |
| S1 | Unit/lint/conform | `make test-unit-all && make lint-all && make harness-verify` | exit 0 | P0 | ✅ exit 0 — but only on the second run. The first was RED: the paged gallery client's unbounded-walk guard had no test, so `lib/api/fleet-library.ts` sat at 88% statements / 50% branches and pulled the app under its 100% coverage threshold. Covered, then 100% on all four metrics. |
| S2 | Memory/build/secrets | `make memleak && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && gitleaks detect` | exit 0 | P0 | ✅ exit 0 on all four. All four memleak lanes clean (`agentsfleetd`, `runner`, `lib`, `boot→drain`); both Linux targets cross-compile; no leaks found. |

**Grading protocol (VERIFY):** run verbatim; record ✅/❌ and one decisive line. Every P0 must pass.

**Graded VERIFY block (§§2–4).**

```
Test Delta: unit 3051→3133 (+82) · integration 407→418 (+11) vs CHORE(open) baseline
Lacking:    none — every changed surface gained coverage. The two that were bare
            when this branch was picked up are now covered: the requirement
            ceilings (11 boundary tests plus two handler-level refusals driven
            through the real PATCH route) and the gallery client's walk (its
            resume and its bound).
```

`make memleak` evidence, verbatim:

```
✓ [agentsfleetd] memleak lane passed
✓ [runner] memleak lane passed
✓ [lib] memleak lane passed
✓ [boot-drain] boot→SIGTERM→drain ran leak-clean under the gate
✓ memleak gate passed (agentsfleetd + runner + lib lanes + boot→drain lifecycle)
```

**One artifact worth not mistaking for a failure.** A passing `zig build test`
prints `failed command: …agentsfleetd-tests` in this Zig version even when
nothing failed — the same run reports `Build Summary: 34/34 steps succeeded;
1944/2235 tests passed`. It cost real time here on the assumption that make and
Zig were disagreeing about an exit code. They were not: read the Build Summary
and make's own line gating, not that string.

## Dead Code Sweep

No file deletion. Removed unpaged helpers, unsupported filter, bare Fleet identities, and old cursors must have zero root-wide production references.

## Out of Scope

- M139_004 telemetry semantics; M143_002 UI state; M143_003 evidence.
- Authentication redesign, proxy layer redesign, token redesign, and trusted runner secret execution.

---

## Product Clarity (authoring record)

1. **Successful user moment** — bounded pages load and colliding Fleet identities open correctly.
2. **Preserved user behaviour** — model management, Fleet install, admin, CLI, authorization, and billing remain valid.
3. **Optimal-way check** — set-oriented keysets and shared generation remove root causes.
4. **Rebuild-vs-iterate** — refactor reads and transactions while retaining authoritative tables.
5. **What we build** — exact pages, locks, caches, ceilings, routing, and sink rules.
6. **What we do NOT build** — no Fleet cache, secret cache, unbounded alias, or timing gate.
7. **Fit with existing features** — extends vault presence, RFC 7807, and generated OpenAPI without stale billing.
8. **Surface order** — API/CLI first; UI consumes it in M143_002.
9. **Dashboard restraint** — no dashboard; only required response metadata.
10. **Confused-user next step** — typed status identifies input, auth, permission, missing entry, or retry.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** projection, catalogue/cache, Fleet, and sink slices align ownership.
- **Alternatives considered:** UI-only masking and process-only invalidation leave root failures.
- **Patch-vs-refactor verdict:** **refactor**, because keysets, generation, locks, identity, and cache ownership change together.

**Directed at CTO review, with reasons, so they are not re-litigated:**

- **Per-row decryption on read paths — removed.** The page displays metadata, never a secret value, and `vault.markExisting` already answers presence with one batch query. `connectors/catalog.zig::innerCatalog` proves the pattern in production. This is the largest single cost on the path and the only reason ciphertext was touched on a read.
- **The publish-generation protocol — removed.** The response cache key already carries the revision, which makes a stale candidate unreachable rather than dangerous. Keeping a published-generation guard on top of a revision-keyed cache adds an ordering race to defend against a problem the key already solves. Billing keeps its reconciliation because its cache is keyed by `(provider, model_id)` and cannot use the same trick.
- **Signed cursors — considered and rejected.** An HMAC over the cursor would collapse the shape and identity checks into one signature check and stop a client forging a boundary. Rejected because the key would have to outlive a process and be shared across replicas to avoid breaking pagination on every deploy, and the security gain is small: the cursor already carries the tenant or workspace identity, which is validated against the authenticated principal, so a forged cursor can only seek within data the caller may already read. Revisit only if cursors ever carry something the caller cannot otherwise obtain.
- **Approximate result counts — not built.** `total:null` is a keyset consequence, and an estimate is worse than an honest absence for a page users act on. Reconsider if a real complaint arrives.

## Discovery (consult log)

- **Consults** — Oracle second-pass blockers incorporated exactly.
- **Metrics review** — production telemetry belongs to M143_003; no funnel change.
- **Skill-chain outcomes** — populated during implementation.
- **Deferrals** — §§2–4 were left `IN_PROGRESS` when §1 merged; see the continuation entry below for the owner ack that authorized it.

### Continuation onto a second branch (§§2–4)

- **§1 shipped alone, by owner decision.** PR #558 merged as `b2ca2afa9` carrying §1, the test-infra isolation work, and the memleak-lane fix, with §§2–4 still `IN_PROGRESS`. Splitting the workstream across two branches rather than holding #558 open was Indy's call:

> Indy (2026-07-26 ~15:45 IST): "give me the /pickup prompt for the next agent to continue, on a new branch" — context: #558 merged with §§2–4 IN_PROGRESS, continued here.

- **Test Baseline reset.** `unit=3051 integration=407`, measured on this branch at its first commit. The VERIFY Test Delta for this branch's PR is graded against that, so #558's tests are not counted twice. The §1 baseline was `unit=2958 integration=393`.
- **The spec stays in `docs/v2/active/`.** It moves to `done/` at this branch's CHORE(close), when §§2–4 land — not at #558's.

### §2's response cache moved onto `common.CacheTable`

- **Owner decision, against the implementing agent's first recommendation.** Indy (2026-07-26) directed that the model and Fleet library caches use the shared `common.CacheTable` primitive rather than the hand-rolled intrusive list `state/model_library_cache.zig` shipped with in #558, and asked for the objections to be reviewed adversarially rather than defended. Two of the three did not survive that review and are recorded here so they are not raised again:
  - *"The clock must be monotonic and the table takes milliseconds"* — **wrong.** `now_ms` is a caller-supplied parameter, deliberately so ("a parameter rather than a clock read so expiry boundaries are provable without sleeping"). The unit is a naming convention; the epoch is the consumer's choice. The cache passes a monotonic source and reads no clock itself.
  - *"§2 requires true LRU"* — **weak.** It required it because the predecessor happened to provide it. Per-bucket LRU across four ways, at a 50% load factor, retains near-totally, and a miss costs one catalogue rebuild. The spec is amended (§2) rather than the design bent to it.
  - *"There is no byte dimension"* — **stands, and was the useful one.** The table bounds slots, not bytes, so the 8 MiB ceiling stays a wrapper-side bypass threshold. That is what §2 always specified ("a bypass, never an eviction cascade"); the predecessor evicted live entries under byte pressure, so this is the shape the spec asked for rather than a concession.
- **The primitive leaked on five paths, not the two first identified.** `Context.evicted` fired only on bucket overflow and `clear`. Every other departure — a same-key refresh, reuse of an expired slot, `remove`, `removeMatching`, and a `get` that reaped an expired entry — dropped its value with no hook and no return. Invisible while `V` is an integer, and a leak the moment `V` owns memory; the same-key refresh is the most common write a time-to-live cache makes. **Fixed** by routing every departure through one `release` choke point and deleting `put`'s `?Entry` return, which was a second channel reporting one of those events and no channel at all for the other four. An owner reading that return leaked on every refresh. `cache_table_test.zig` pins the rule per path with a spy and again end to end under `std.testing.allocator`.
- **`sweepExpired` was added because the byte ceiling needed it.** Expiry is lazy, so dead payloads keep holding budget against live ones, and `removeMatching` cannot express "expired" — it is handed `(key, value)` and the deadline lives on the entry. The cache sweeps once on the over-budget path before it refuses an insert.
- **The key stopped being a string.** A revision plus a 32-byte digest is fixed-size and stored inline, so no key is allocated, no key byte counts against the 8 MiB budget, and the 256-entry ceiling is `BUCKET_COUNT * BUCKET_SIZE` rather than a counter.
- **Sequencing.** `cache_table.zig` was untracked in the `m141-lease-fanout` worktree, on a branch never pushed, with no open Pull Request — so there was no landing timeline to wait for and nothing to import from. Indy chose to land it through this branch; `m141` rebases onto `main` and drops its own copies of `cache_table.zig`, `cache_table_test.zig`, the `RwLock` addition, and the `constants.zig` re-exports, all of which are taken verbatim here apart from the release fix.

### §2.2's rate cache moved onto `common.CacheTable` too — and what that cost

- **Owner decision, extending the response-cache one above.** Indy (2026-07-26): *"are there other Cache that we have custom made? i want it to be on CacheTable"* → *"for rate-cache"* → *"I want to use the same CacheTable across."* The implementing agent's first objection — that a bounded, evicting table cannot hold an authoritative catalogue — **stands as stated but does not block**, because it argues for changing what a miss MEANS rather than for keeping the old storage. Recorded here with the consequence, because the consequence is the interesting part.
- **`CacheTable`'s contract is the whole change.** Its module doc: *"For values that can always be recomputed. A miss is never an error."* The rate cache violated that. It held a COMPLETE snapshot rebuilt by `populate`, so a miss meant "not in the catalogue" and three callers acted on it — `computeStageCharge` **panicked**, renewal silently metered run-fee-only (the revenue leak this milestone exists to close), and provider activation rejected the model. Adopting the primitive means adopting its contract: a miss now loads the one row it asked about, and "not catalogued" became a database answer instead of a cache answer. That deletes the panic and the silent leak, so the migration is a net correctness gain rather than a like-for-like swap.
- **The generation rides in the VALUE, not the key.** The response cache puts the revision in its key, which is right for it: a page is per-selector and old generations are unreachable. Doing that here would hold one entry per model *per generation*, so every bump would strand a full catalogue of dead entries competing for slots. Storing `{revision, rate}` against a `(provider, model)` key gives one entry per model and lets the two consumers take different guarantees from it — billing accepts only `entry.revision >= observed`, the display path takes whatever is resident.
- **The key stayed two fields, deliberately, against the obvious optimization.** A fixed-size digest key would have been smaller and needed no key allocation (it is what the response cache does). Rejected: a digest makes aliasing *astronomically unlikely* where separate byte-compared fields make it *impossible*, and this is the module where the difference is billing a request at another model's price. Slice headers are fixed-size, so they store inline anyway; the cache owns the bytes and frees them in `evicted`.
- **This removes §2's stated reason for reconcile-and-copy.** §2 says billing "keeps its explicit reconcile-and-copy under the mutex" because "the rate cache is keyed by `(provider, model_id)` rather than by revision, so it cannot use this trick." Carrying the generation in the value *is* that trick, so the premise no longer holds and the protocol is gone. What replaces it: billing reads the generation on its own connection, and the cache serves only entries at or after it. §2 is amended accordingly.
- **Accepting a LATER generation is deliberate.** Revisions only increase, so an entry ahead of the caller's observation is fresher, never the stale direction the rule guards. Reloading to match an older observation exactly is impossible anyway — the database holds current state, not history.
- **`bumpLocked` had NO production caller, and that was a live bug.** `state/model_catalogue_revision.zig` shipped with `lock`/`bumpLocked` and an integration test, but nothing in `http/handlers/admin/model_library_admin.zig` called either — all three mutations ran in **autocommit**. So the revision never left 0, and since it forms the response-cache key, **an admin catalogue mutation never invalidated the response cache**. Found by grepping the production entry point for the symbol, which is the spec's own DONE test. Fixed with `Txn`/`beginMutation`: lock `FOR UPDATE`, mutate, bump, commit — one transaction, one home, rather than the protocol spelled three times.
- **The boot warm is gone, not ported.** `serve.zig` populated the cache at startup and exited non-zero if it failed. With load-on-miss, a bulk preload is a second way to fill one cache and the two would drift; it also made the daemon refuse to boot over a briefly unreadable catalogue. Removing it freed 9 lines on a file sitting at exactly 350/350.
- **`contextCapForModel` became a SQL aggregate.** It returns the MINIMUM context window across every provider carrying a model. A minimum over a bounded cache is a minimum over whichever rows survived eviction — an error that is one-directional and unsafe, because a context budget above the real window fails the request mid-run at the provider. `MIN(context_cap_tokens)` answers it correctly; it runs at activation, not on a hot path.
- **"Fail closed" resolved per call site, because the spec's one phrase covers three different decisions.** At **renew/settle** (the actual charges) an unverifiable generation meters run-fee-only and logs — the token component is dropped rather than guessed, and the run continues. Refusing the renewal instead would kill a live agent over a transient database fault, which is the trade `budgetRefusal` already rejected in this codebase with the same reasoning. At the **lease gate** it stays fail-OPEN, unchanged: that path computes an *estimate*, not a charge, and its own doc commits to never turning a metering outage into an availability incident. At **activation** an unreadable catalogue no longer rejects a valid model. The two charge-path failures are logged apart — one is a database fault to page on, the other a catalogue gap to fix.
- **Not done here, and why:** `credentials/broker.zig` is the one remaining hand-rolled-adjacent cache (external `karlseguin/cache.zig`, 64 segments). It is a genuine candidate — its refcount only spans a `dupe`, exactly what a shared lock covers, so the use-after-free objection raised first was **wrong**. The real gap is sharding, addressable with a small `ShardedCacheTable` wrapper. Left out: credential mint path, outside this spec's Files-Changed blast radius, and the operating model bans opportunistic bundling. Its own spec.

### §3's Fleet summary sheds one field, not three — measured, not assumed

- **What §3 asked for.** §Interfaces: *"summary never contains requirements/support/presence fields."* The stated basis is the resource table's 512 KiB summary ceiling against ≤100 rows.
- **The removal is a v1 field deletion, which `docs/REST_API_DESIGN_GUIDELINES.md` §9 forbids outright** — *"Once a field name is exposed, it's immortal until `/v2`"* and *"No silent removals. No removals that skip the `Deprecation` header period."* Same guideline that decided the `visibility` rename. So "follow the spec" and "follow the rules" pointed opposite ways, and the spec is the instance while the rules are the constant.
- **Resolved by asking what each field is FOR, not who references it.** Owner framing (2026-07-26): *"i am asking does it provide value to the user — example you display to the user or so? if its just a fields returned and has no value then remove the fields from the api and where they are being referenced."* That reframing is what split the three apart; a reference count would have kept all three.

| Field | Rendered to a user? | Verdict |
|---|---|---|
| `requirements` | **Yes** — `LibraryCard.tsx` renders `requirements.credentials` as the card's chips; `install-flow.ts` feeds tools / network hosts / trigger into the install gate | **kept** |
| `required_credentials_reasons` | **Yes** — `InstallStates.tsx` → `ConnectGate` renders the per-credential "why this fleet needs it" copy for every unmet credential | **kept** |
| `support_files` | **No** — a type declaration in `lib/types.ts` and one `support_files: []` test mock. No component renders it; `install-flow.ts` ignores it | **removed** |

- **The one field with no reader is the one that breached the ceiling.** `MAX_SUPPORT_FILES` (32) x `MAX_SUPPORT_PATH_LEN` (160) puts one manifest entry at ~197 bytes, a row at ~6.3 KB, and a 100-row page at ~630 KB — past 512 KiB on its own, before anything else is counted. Without it a realistic row is ~1 KB and a full page ~100 KB. So removing `support_files` alone achieves what §3 wanted; removing the other two would have saved roughly a kilobyte and cost the install gate its copy.
- **No deprecation window is needed after all.** The two fields with consumers stay, so nothing first-party breaks. `support_files` has zero readers in `ui/`, `cli/`, or `src/runner/` — it is still a documented v1 field, so its removal is recorded here as a deliberate, argued deletion rather than a silent one.
- **The runner was never a consumer, and the reason matters.** It does not call the gallery at all. `LeasePayload` carries `instructions` (the `SKILL.md` body), `policy` (the ENFORCED tool/secret policy), and `bundle.content_hash`; the runner then GETs `/v1/runners/me/bundles/{hash}` and `bundle_extract.materialize()` unpacks the real support-file bytes into the sandbox. So the gallery's `requirements` are display hints for a human deciding whether to install, while the runner's requirements are hard policy on a different field — and the gallery's `support_files` manifest describes bytes the runner fetches from object storage instead. Two channels, and only the human-facing one is in this spec's scope.
- **Flagged, not fixed:** neither kept field has an enforced cap. The importer sets no count limit on `required_credentials` / `required_tools` / `network_hosts`, and `catalog_patch.zig` validates only `MAX_NAME_LEN`, so `required_credentials_reasons` is unbounded operator-written text. Small in practice, unbounded in principle. Adding caps is additive and non-breaking; raised with Indy rather than bundled here, since the write paths are outside this spec's Files-Changed scope.

### §1 preconditions, verified at PLAN

- **Migration slot** — **036**. Slot 35 was taken by `035_workspace_create_idempotency.sql` (#556); no sibling worktree (`m141-lease-fanout`, `fleet-bubbles-reconciliation`) claims 036.
- **Blob-trapped fields — all four, not just `base_url`.** §1's precondition fired harder than drafted. `schema/027_core_tenant_model_entries.sql` states it in its own header: *"Provider labels, base_url, kind, and api_key remain vault JSON metadata, not table columns."* So `provider`, `kind`, `base_url`, and `has_key` were **all** inside the AES-GCM envelope. Per §1's standing instruction, promoted rather than decrypted per row — `schema/036_vault_secret_metadata.sql` adds `meta_kind`, `meta_provider`, `meta_base_url`, `meta_has_key`.
- **Columns live on `vault.secrets`, not a sidecar.** The metadata describes the credential, and one credential backs many model rows, so it belongs on the credential row. `markExisting`'s existing index serves the new read with no JOIN and no extra statement.
- **Written in the same statement as the ciphertext.** `INSERT_SECRET` carries the four `meta_*` values on both the INSERT and the ON CONFLICT arm, and `vault.storeJsonPlaintext` is the only producer — it projects the exact bytes it is about to encrypt. Drift is not guarded against; it is unrepresentable.
- **Backfill is a manual one-time operator sweep, not a boot-time task.** Indy (2026-07-25): pre-production, run manually against development, with the production run sequenced after M136_001. The read path therefore has **no** decrypt fallback for un-projected rows — a heal-on-read would make Invariant 5 conditional on history. An un-backfilled row reports as an opaque `custom_secret`.
- **`dispatch/write_sql.md` carries stale prose.** Its "Pre-v2.0.0 (teardown-rebuild era)" section forbids `ALTER TABLE` and directs editing shipped slot files. `docs/SCHEMA_CONVENTIONS.md` §Migration Model supersedes it (owner decision Jul 22, 2026: additive migrations, *"shipped slot files are frozen history: never edit an existing `schema/NNN_*.sql`"*), and `write_sql.md` itself names SCHEMA_CONVENTIONS the source-of-truth. Followed Conventions. **Governance fix is out of scope here** (`dispatch/edit_rules.md` + `make audit`) — flagged to Indy, not silently patched.
- **`secret_metadata.zig` moved to `secrets/metadata.zig`** — spec amendment to Files Changed. The projection became a **write-time** function, so `state/vault.zig` must call it; leaving it under `http/handlers/fleets/` would make `state/` import from `http/handlers/`. Now a leaf importing `std` only, and it owns `OPENAI_COMPATIBLE_PROVIDER` (classification is why the constant exists); `state/secret_probe.zig` re-exports it so the `tenant_provider` chain is untouched.
- **Measured effect on the tenant registry page.** Was: `resolvePrimaryWorkspace` + one envelope open **per row** (~2·N statements, N decryptions for N ≤ 100). Now: one selection read, one entry list, one workspace resolve, one metadata batch, one platform default — **five statements, zero decryptions**, independent of page size.
- **Fleet gallery `visibility` field — NOT renamed to `tier`.** Indy (2026-07-25): *"no rename needed."* §Interfaces' `FleetSummary={tier:…}` is amended to keep `visibility`, which already carries `platform`/`tenant`. Avoids a `/v1` field rename that `docs/REST_API_DESIGN_GUIDELINES.md` §9 forbids; no guideline amendment needed. The summary still sheds `requirements`/`support_files`/`required_credentials_reasons` to the detail route — that half was uncontested.
- **Two-table split retained.** Merging `core.fleet_library` and `core.tenant_fleet_library` behind a nullable owner column was raised and rejected by Indy (2026-07-25): *"I think this separation is good … I dont see a need to merge."* The tier label is the cost of a merged read and is cheaper than the migration.
### Test-infra verdicts, and why the lane was lying

- **An ephemeral host port is not a stable address.** The first isolation fix dropped both `container_name` and the fixed host port, letting Docker assign one per container. Only the name needed to go. A Docker-assigned published port is reallocated whenever the container is recreated **or restarted**, while `make/test-integration.mk` resolves it into a URL string exactly once per run — so any restart between resolution and the tests points the whole suite at a dead port. Measured across three consecutive invocations in this worktree: 61522 → 57390 → 63324. Ports are now derived from the Compose project name (`scripts/test-infra-ports.sh`), verified stable across `restart` and `stop`+`start`.
- **The failure this produced did not look like a port fault.** It surfaced as ten Redis pub/sub failures, and was recorded in PR #558 as *"ConnectionBusy — intra-suite pool contention."* That reading was wrong on every count. `ConnectionBusy` is defined only in `pg.zig` (`src/conn.zig`) — there is no Redis error by that name; it appeared solely as `[default] (warn): ignored: ConnectionBusy` log noise in captured stderr, and `_lint_zig_pg_drain` passes. The real error was a TCP connect failure in `redis_connection.zig::dialAndAuth`, before TLS and before any pool checkout. Confirmed by running the same three test files against the live port: 75 + 56 + 43 = **174 tests, all passing**, with the lane otherwise untouched.
- **Cross-worktree interference was ruled out too early.** The handoff into this session stated it was; it was not. The lane used `:57390` while the container listened on `:63324`. The general lesson for this repo: when the integration lane fails, confirm the address it actually dialled before reading the failures as behaviour. `_test-integration-redis` echoes its URL; `docker compose port` gives the truth to compare against.
- **The sibling worktrees are still destructive, and their failures are not evidence.** `agentsfleet-fleet-bubbles-reconciliation` and `agentsfleet-m141-lease-fanout` still carry the pre-fix compose (`container_name: agentsfleet-redis`, `- "6379:6379"`) and the `docker rm -f` sweep. They claim the same three fixed-name containers, so each run force-removes the other's container and remounts its own `<project>_redis-tls` volume — a **different** self-signed cert. Any cert extracted moments earlier is then stale, giving `error.CertificateSignatureInvalid`. This is the likely source of the "known pre-existing failure on `main`" recorded in PR #558; that claim should be re-tested from a worktree carrying this fix before it is trusted. Those worktrees need this branch's `docker-compose.yml` + `make/test-integration.mk`.
- **The integration suite is not idempotent without a database reset.** A second consecutive run against the same database goes 457/0 → 447/10; proven pre-existing and independent of this branch. `make test-integration` resets first and is unaffected — the trap exists only for an agent iterating with bare `zig build test` against live infra. Not a regression; do not chase it.

### Adversarial-review findings, verified and fixed

An independent Codex pass over the branch raised eight items. Each was verified against the code before acting — one was downgraded on inspection, and the verification turned up a defect Codex had not named.

- **The reference lock counted the wrong tenant's entries.** `secrets.zig` passed `hx.principal.tenant_id` into `secret_reference_txn.begin`, and step 2 filters entries on it. `crossTenantBypass` sets the *session* context to the target tenant but leaves the principal's own tenant in place, so a `workspace:any` operator deleting inside another tenant's workspace matched zero references and deleted a credential the victim's entries still named — the exact orphan the protocol exists to prevent, with the audit log recording it as authorized. A platform principal (null tenant) skipped steps 2 and 3 entirely, same outcome by a different route. **Fixed structurally:** `begin` now derives the tenant from the workspace (`core.workspaces.tenant_id` is `NOT NULL`) and the parameter is gone, so no caller can supply the wrong one. The other two call sites are unaffected — their workspace was resolved *from* that tenant.
- **`base_url` was promoted out of encryption without being sanitized.** `schema/036` moved it to a plaintext column on the argument that every projected field is metadata any authorized caller already sees. `state/base_url_guard.zig` validates the HOST and deliberately accepts userinfo — its own test asserts `https://user:pw@gw.example.com:8443/v1` is `.ok` — so a credential can carry a password inside its URL, and the promotion put it where any database reader can `SELECT` it without the Key Encryption Key. **Fixed:** `metadata.displayableBaseUrl` omits a `base_url` whose *authority* contains `@`. Omitted rather than rewritten, because stripping `user:pw@` yields a string that is not a subslice and this projector is deliberately allocation-free. A `@` in a path or query still projects — dropping those would hide legitimate endpoints for no gain.
- **Entry deletion could orphan the active selection.** `isActiveEntry` read with no lock, then `delete` ran as a separate statement; an activation committing between them left `core.tenant_model_selection` naming a row that had just been deleted. **Fixed** by running the check and the delete inside the shared reference transaction. Deliberately the full order (credential → entries → selection) rather than locking the selection alone: activation takes the selection *last*, so a selection-only lock here would invert the order and trade a race for a deadlock.
- **The backfill could overwrite fresher metadata.** It decrypts a whole workspace, then writes each projection later with no predicate, so a credential rotated in that window — new ciphertext and new metadata in one statement — was overwritten by the projection of the plaintext the sweep read beforehand. **Fixed** with `AND meta_kind IS NULL` on the UPDATE; a new `rotated_midway` tally reports when the sweep raced live traffic instead of hiding it.
- **`errdefer txn.abort()` was dead code**, and Codex had not spotted this one — it named the commit-failure path but called it a poisoned connection. The pool destroys non-idle connections on release, so it is not; the real defect is that both `errdefer`s sat in `void`-returning handlers, where `errdefer` can never fire. The rollback everyone reads as present had never once run, and the commit-failure path had none at all. **Fixed** by switching to `defer` (abort is idempotent, so it no-ops after a successful commit). The two remaining `errdefer` sites are in error-returning functions and are correct.
- **Downgraded on verification:** the "poisoned connection" severity above. **Deliberately not changed:** the encoded-body ceiling still builds the response before measuring it (Codex #7) — with `model_id` bounded, the work it bounds is bounded too — and the test-only counters remain process-global (Codex #8), which is documented and compiled out of production.

### `model_id` was unbounded, and the body ceiling was the symptom

- **Found by asking when `UZ-LIBRARY-005` could actually fire.** The answer should have been "never, it is a defect backstop" — the registry page's own §1 measurement puts a full 100-row page at ~40 KB against a 512 KiB ceiling. It was not never. `schema/027` declares `model_id` TEXT, and both tenant write paths checked only that it was non-empty, while the *catalogue* write path for the same field has always enforced 1–256 (`model_library_admin.zig`). One field, two doors, two rules.
- **The reproduction, and the wrong first guess.** A single 600 KB `model_id` does NOT work: `uq_tenant_model_entries_entry` is a btree over `(tenant_id, model_id, secret_ref)`, capped at 2704 bytes *after* compression, and 600 KB of repeated text still compresses to ~7 KB. Postgres refused it — surfacing as `503 Database unavailable`, a client input fault reported as a server one. What does work is **three rows of ~200 KB**: compressible enough to index, still 200 KB each in the response. Confirmed by inserting them and watching the page return 500.
- **The blast radius reaches other tenants, which is what made it urgent.** Every projected row calls `model_rate_cache.lookup_model_rate`, which hashes the whole `(provider, model_id)` pair **while holding the process-global rate-cache mutex that billing shares**. 100 rows × 200 KB is ~20 MB of hashing per request under a lock every tenant's charge computation waits on. The pre-`0b0094379` key encoding bounded this incidentally — its 512-byte key buffer returned null and skipped the hash — so removing that buffer to fix a correctness bug also removed an availability guard. Caught by the Codex adversarial pass, not by the review that approved the commit.
- **Bounded at the boundary, at the value the sibling route already used.** `types/model_identity.zig` gives both paths one home. 256 bytes is ~5.7× the longest real identifier (`nvidia/NVIDIA-Nemotron-3-Ultra-550B-A55B-BF16`, 45 bytes). A full page of maximal rows is now ~66 KB — the ceiling is arithmetically unreachable through the API, which is what its registry text always claimed.
- **That cost the integration tier its trigger, deliberately.** With no input able to breach the real ceiling, nothing would prove the handler maps `BodyCeilingExceeded` onto `UZ-LIBRARY-005` rather than a bare 500. `setTenantRegistryBodyCeilingForTest` lowers the ceiling under test only (the constant is untouched and separately pinned), mirroring `model_rate_cache.setBackingAllocatorForTest`. The alternative — deleting the test because the bug it covered is fixed — would drop the error contract along with it.

### §3's measured table, corrected against the measurement

- **The tenant registry row said ≤4 statements; it is 5.** The spec contradicted itself: §3's table was drafted at ≤4, while §1's own Discovery entry recorded the implemented read as *"five statements, zero decryptions"*. The table went to ≤5, because 5 is what the instrumentation measured and the five are each load-bearing — `activeSelfManagedRef` (which entry is active), `listPage` (the page, over-fetched by one), `resolvePrimaryWorkspace` (once for the page), `loadMetadata` (one batch for every row), `platformDefaultView` (the Default row's identity). Nothing was added to reach 5 and nothing could be removed to reach 4 without dropping a field the page renders. Correcting the code to match a number drafted before the shape was known would have been fitting the read to the spec's arithmetic rather than the other way round.
- **Then §2.2's cache migration made it 6, and that is a bug fix rather than a cost.** The row is now ≤6: `model_rate_batch.loadRatesForPairs`, one statement for every row's rate plus the platform default's. §2.2 rebuilt the rate cache as a load-on-miss table and removed both things that used to fill it in bulk — `serve.zig`'s boot warm and `test_fixtures_provider.populate()`. The registry page kept reading it through the resident-only `cachedRate`, which loads nothing. So after any restart every rate on the Models page rendered blank, permanently: only a billing charge for that exact `(provider, model)` would ever admit the row, and a display read has no reason to wait for one. Caught by `test_entries_list_default_identity`, which expects `"input_nanos_per_mtok":0` on the platform default and got `null`.
- **Fixed with a batch, not by restoring the warm.** A boot warm is a second way to fill one cache — the thing removing it was meant to end — and it does not survive eviction either, so the blank cells would come back under pressure rather than under restart. One set-oriented statement over the page's `(provider, model)` pairs keeps the count independent of `limit`, which is the property §3's budget actually pins. The absolute number has now been corrected upward twice, both times to the MEASUREMENT.
- **The default's pair rides in the same statement.** `platformDefaultView` is resolved before the batch rather than after, so the Default row's rate costs no seventh statement. That ordering is the only reason the number is 6 and not 7.
- **It reads the database and populates nothing**, and it lives in `state/model_rate_batch.zig` rather than in the cache module so that is structural instead of a comment. Admitting rows from a display read would fill a billing cache from a path that observed no revision; it would also allocate process-lifetime key strings on a request path, which is the hazard `backing_allocator`'s own note already records from the time the admin handler passed its request arena in.
- **`cachedRate` lost its last production caller and was kept anyway.** Its two remaining callers are in `model_catalogue_revision_integration_test.zig`, where it is how the suite observes that a charge's load-on-miss actually admitted the row — a property of `rateAtRevision` with no other external witness. Retitled as a residency probe rather than deleted; flagged to Indy as a judgment call rather than swept as dead code.
- **An empty page costs 4, and that is pinned separately.** `loadMetadata` and `loadRatesForPairs` BOTH return before querying when there is nothing to describe or price. Asserting only "≤ 6" would let either guard clause be deleted in a refactor without a single test noticing — the result is still under budget. The empty-page test now seeds its platform-default state explicitly instead of inheriting it: with the rate batch in the read, a default left active by a sibling suite supplies a pair and makes the count depend on execution order rather than on the guards under test.
- **The counters count where the queries are, not where the handler is.** The statement tally moved into `db/pg_query.zig`. §3's claim is about a call graph ("this read issues at most five statements"), and a claim about a call graph has to be counted at the bottom of it; a tally the handler increments only ever counts what its author remembered. The `beginRead`/`endRead` window is what keeps that global hook scoped to one endpoint, and opening it at handler entry — after the middleware chain — is what makes it mean what §3 says it means.
- **The two Fleet rows said ≤1 and ≤2; both are 3, and the reason is where the window actually opens.** §3 states its budget "after middleware auth", and the natural reading is that authorization is behind us by then. It is not. The bearer chain authenticates a principal; `common.authorizeWorkspace` decides whether that principal may reach THIS workspace, and only the handler knows which workspace the path names — so it runs in the handler, after `beginRead()`, and its two statements (`core.users` tenant resolve, then the workspace-belongs-to-tenant check) are inside the budget by construction. Each Fleet read then issues exactly ONE statement of its own: the summary's merged `UNION ALL`, the detail's single select. Corrected to the measurement, as the tenant registry row was, twice.
- **The detail row's ≤2 had folded in an authorization cost of one**, which is not what `authorizeWorkspace` charges. Worth recording because the first draft of the bounds test reproduced that mistake in the other direction — it added the table's `FLEET_DETAIL_MAX_STATEMENTS` (2) to the authorization pair and expected 4, against a measured 3. A number carried from a table into a test is not a measurement; it is the same guess twice.
- **Both Fleet reads are one statement whatever `limit` is**, which is the property the row exists to pin. The authorization pair is fixed overhead that does not scale with the page, and the `UNION ALL` is what keeps the summary from paying per table.
- **NFKC moves to SQL.** Zig's std ships no Unicode normalization tables and no dependency supplies them. Postgres `normalize(text, NFKC)` is built in and immutable, so `lower(normalize(col,NFKC))` is index-eligible. Zig keeps trim, whitespace-collapse, the byte bound, and LIKE-escaping — all ASCII-safe. Amends §2/§3's "normalize in the handler" to "normalize in the comparison"; user-visible behaviour is unchanged.

### The memleak lane's futex failure, reproduced and closed

- **The red check was real, and it was not this branch's defect.** Every failing run reported `0 failed` tests and exactly one valgrind error: `Syscall param futex(futex) points to unaddressable byte(s)` at `Io.Threaded.Future.start` (`Threaded.zig:757`) via `worker` (`Threaded.zig:1797`), address "on thread 1's stack … below stack pointer". The identical signature failed a docs-only branch (run 30149190866), and `make/bench.mk` already carries the owner's verdict on it — the earlier suppression was reverted because *"valgrind was right"*: Zig 0.16.0's awaiter parks on a futex word in its own stack frame, and the worker publishes the wake condition **before** dereferencing that word (`Threaded.zig:760-762`), so the awaiter can return and pop the frame first. A genuine upstream use-after-scope, deliberately unsuppressed.
- **What still created futures after the repo removed its own async call sites:** `std.Io.net.HostName.zig:283/343/353` — `HostName.connect` internally spawns `io.async` futures for the lookup and the parallel connect attempts, with no shortcut for IP literals. Every loopback dial in the test suite enters it.
- **Exactly two tests dialled on a test-local multi-threaded io** — `otlp/Client_test.zig` and the redis handshake-stages test — both relics of the removed `Io.Select` raced-dial era. Every other test already used `common.globalIo()`, whose `.failing` pool allocator makes `io.async` execute inline and return no future (`Threaded.zig:2089-2093`): no worker thread exists, so the wake-after-return race is structurally unrepresentable, not merely unlikely.
- **Reproduced before fixing, in the CI image locally** (`ci-zig-debian-trixie:0.16.0`, amd64 under emulation): the handshake-stages test filtered into its own binary went red **14/30 runs** under the exact `VALGRIND_LEAK_GATE` flags, all 43 tests passing on every run. After the io swap: **0/60** on the same loop, and 0/60 on the otlp-filtered binary. The full test binary A/B agrees: stock **2/5** red with frames identical to the CI failures, fixed **0/8**. Native `zig build test` on both edited files: green.
- **One unattributed observation, recorded rather than hidden:** a single fixed-full run printed the runner's `1 tests leaked memory` line with no valgrind error. It never recurred — seven subsequent full runs and 120 filtered runs (all under `std.testing.allocator`) were leak-clean, and no CI memleak run in the branch's history carries that line. Not attributable to the two edited files on this evidence; if it is real, a CI recurrence will name the test in full output.
- **The otlp 200-test's valgrind skip is removed**: its stated reason — a successful fetch spawning a worker whose glibc thread-local block reads as "possibly lost" — cannot occur on the serial io, so the success path is now leak-audited under the lane instead of skipped.
- **Residual, accepted and named:** the boot→drain lane runs the real daemon on a real multi-threaded io, whose live dials still enter `HostName.connect`; `exporter_test`, `subscription_hub_test`, `redis_pool_test`, and the runner suites keep real `Threaded` ios their subjects genuinely need (flush-thread spawn, hub fan-out, `std.process.run`'s allocator). Those can still lose the upstream race; none has been observed to. The durable close is an upstream report of the `Threaded.zig` wake-after-return defect.

### The requirement caps, and why they are not a deferral

§Discovery's *"§3's Fleet summary sheds one field, not three"* flagged that neither
kept field has an enforced cap, and left it: *"raised with Indy rather than bundled
here, since the write paths are outside this spec's Files-Changed scope."*

That was an agent-unilateral deferral. Indy was asked twice and never answered, so
no ack quote exists, and §Deferral discipline is explicit that an unacked deferral
is incomplete scope rather than a deferral. Indy's pickup direction settled it:
the caps are **in scope**. The write paths are brought in by the amendment table
above rather than left as an out-of-scope note.

**What was actually unbounded.** The importer capped `source_ref`, the support-file
count, each support file, and their total — but never the requirement LISTS it
parses out of `TRIGGER.md`. `catalog_patch.zig` checked that
`required_credentials_reasons` was an object whose values are strings, and nothing
else: not how many entries it held, not how long a key ran, not how long the copy
ran. So the two fields §3 deliberately KEPT on the gallery card were the two with
no size rule.

**Why one module rather than a check at each door.** Two paths write these fields
and they share exactly one rule about what a credential name may be. Spelling it
twice is how the same field acquires two different rules — the precise failure
`types/model_identity.zig` was created to fix earlier in this workstream, where the
catalogue bounded `model_id` at 256 and the registry checked only non-emptiness.

**The numbers, and why each one.** `MAX_REQUIRED_CREDENTIALS = 32` (one install-gate
row is one thing a user must go and connect); `MAX_REQUIRED_TOOLS` and
`MAX_NETWORK_HOSTS = 64` (declared, not connected, so no per-entry install cost);
`MAX_REQUIREMENT_NAME_LEN = 200` — not a new policy, it is the display-copy cap
`catalog_patch.zig` already applied to an entry's `name`, applied to the other names
on the same resource; `MAX_NETWORK_HOST_LEN = 253`, the RFC 1035 §2.3.4 maximum, so
a rejected host is one that could not have resolved anyway; `MAX_REASON_ENTRIES` is
pinned EQUAL to the credential ceiling by a test, because copy for a credential the
bundle never declares is copy the refetch prune drops; `MAX_REASON_LEN = 500`.

**What it buys §3.** The counts times the lengths give a hard ~35 KB ceiling on the
encoded `requirements` blob. That is what turns the per-item projection bound from
an observation into a construction, and keeps `UZ-LIBRARY-005` the unreachable
invariant breach §Error Contracts calls it rather than something a large enough
bundle can provoke.

**Behavioural change to know.** A bundle or a curate PATCH that crosses a ceiling is
now refused — 413 on import, 400 on curate — where it was previously accepted. No
existing row is re-validated, so nothing already stored becomes unreadable.

### The model_library full-lane failure: a test coupled to a page boundary

`integration(model_library): GET with a valid token returns the catalogue` failed in
the full lane and passed under `TEST_FILTER`. It was **the test, not the product**,
and every part of the signature that looked diagnostic was noise.

**The actual cause.** §2 made `GET /v1/models` a bounded page — 50 rows by default,
ordered by normalized `model_id` ascending. `core.model_library` is shared: sibling
suites and the platform seed put ~50 real rows in it. `kimi-library-read-fixture`
sorts AFTER `kimi-k2.7-code-highspeed` (`k` < `l` at the fifth byte), so unfiltered
it lands on page TWO — and the test asserted it appeared in the one page it read.
The answer was a correct `200` carrying a correct 50-row page and a `next_cursor`.

**Why filtering "fixed" it.** Under `TEST_FILTER='integration(model_library)'` the
suites that seed those ~50 rows never run, and this file's own empty-catalogue leg
deletes the table. Two rows remain, both fit page one, and the assertion holds. The
filter changed the fixture population, not the code path — which is exactly the
failure mode `make test-integration TEST_FILTER=` was warned about when it landed.

**Why two signatures wasted the most time, recorded so the next reader skips them.**

- **`RedisCommandError` at `redis_connection.zig:155`, in a test that issues no Redis
  command.** Zig's error return trace is a THREAD-LOCAL buffer that is not cleared
  between tests, and `test_runner.zig` dumps it on failure without printing the
  error name. The frames belonged to earlier tests that deliberately inject Redis
  faults — instrumenting the reply path showed `cmd=PING server_err=ERR fake_error`
  and `READONLY You can't write against a read only replica`, i.e. fixtures. A trace
  with no accompanying error name is not evidence about the failing test.
- **`harness server listen failed (retrying on a fresh port): AddressInUse`.** A
  `std.log.warn` from an unrelated harness, and `Step/Run.zig` only calls
  `stderr.tossBuffered()` when a test FAILS — so a failing test's report carries the
  accumulated stderr of every passing test since the previous failure. Most of that
  block is other tests' output, including deliberate negative-path noise.

**The fix.** The request filters to `?q=library-read-fixture`, the substring both
fixture ids share and nothing else in the shared catalogue does, so the assertion is
a property of the projection rather than of how many rows another suite seeded
first. `next_cursor` is asserted null, so "both fixtures fit one page" is checked
rather than assumed. Page boundaries, cursor resume and ordering stay Dimension
2.1's, proved against a self-seeded population in
`model_library_page_integration_test.zig`.
