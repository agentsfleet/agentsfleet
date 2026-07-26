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
**Status:** IN_PROGRESS
**Priority:** P1 — library reads repeat database and decrypt work and expose unbounded behavior
**Categories:** API, CLI
**Batch:** B1 — establishes interfaces consumed by later workstreams
**Branch:** `feat/m143-library-data-security`
**Test Baseline:** unit=2958 integration=393
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

The response cache key is revision plus an unlogged HMAC-SHA-256 digest, under a process-random key, of canonical q/provider/starting_after/limit selectors; no raw selector or credential metadata enters keys. It is true LRU, at most 256 entries and 8 MiB, with a monotonic 60-second TTL.

Byte accounting is defined rather than estimated, because "including allocator metadata" is not observable through a Zig allocator. A tracking allocator wraps the cache and sums the exact `len` of every live allocation it owns — key bytes, value bytes, and node storage — so the number the ceiling compares against is the number the test reads. Allocator-internal padding and bookkeeping are outside the budget by construction, and the ceiling is set with that headroom in mind. Insertion is rejected when admitting an entry would cross either ceiling; rejection is a bypass, never an eviction cascade. It contains only non-secret model responses; Fleet is never cached. Allocation failure or over-budget responses bypass insertion. A strong ETag hashes exact bytes. Both 200/304 send `ETag`, `Cache-Control: private, no-cache`, `Vary: Authorization`; after auth/revision, `If-None-Match: *`, exact, or weak list match returns bodyless 304, otherwise 200.

The billing decision linearizes at its revision read: under the rate-cache mutex, reconcile to that exact generation and copy the selected rate before unlock. A later catalogue commit applies only to later revision reads.

Rate-cache identity is a collision-safe structured `(provider,model_id)` key, never delimiter concatenation. Migration tests include provider/model strings containing the current `0x1f` separator and prove distinct tuples cannot alias or select another rate.

- **Dimension 2.1** — normalized search/keyset and headers are exact → Test `test_model_page_and_conditional_headers` — **IN_PROGRESS.** The normalization half is implemented and unit-tested (`http/handlers/library/query.zig`, `library_query_normalization_test.zig`): trim, whitespace collapse, the 128-byte bound, UTF-8 validation, and LIKE-wildcard escaping. Per the Discovery amendment, NFKC and casefold are SQL-side (`lower(normalize(col, NFKC))`), so Zig holds only the ASCII-safe half. **Not yet done:** the keyset/cursor wiring on the catalogue route, the SQL comparison itself, and the `ETag` / `If-None-Match` 200-vs-304 behaviour — so the named integration test is not written.
- **Dimension 2.2** — response and billing caches converge or fail closed → Test `test_catalogue_revision_governs_both_caches` — **IN_PROGRESS.** Three of the four parts exist. `schema/037_model_catalogue_revision.sql` adds the singleton generation; `state/model_catalogue_revision.zig` gives the hot-path read (no lock) and the mutation lock/bump (`FOR UPDATE`), with the named integration test proving publish-after-commit, rollback invisibility, and that two mutations cannot share a generation. `state/model_library_cache.zig` is the revision-keyed LRU with defined byte accounting, the 256-entry / 8 MiB ceilings, monotonic 60s TTL and over-budget bypass (`model_library_cache_test.zig`). `state/model_rate_cache.zig` now uses a structured `(provider, model)` key so the `0x1f` aliasing is unrepresentable (`model_rate_cache_key_test.zig`). **Not yet done:** wiring the revision read in front of cache selection on the catalogue route, and the billing reconcile-and-copy under the rate-cache mutex — so the fail-closed billing half is unproven.

### §3 — Fleet keyset, detail, and measured ceilings

Fleet `q` uses the same normalization, 128-byte maximum, empty-as-absent, and escaped literal substring matching only id/name/description. Set `tier_rank`: platform=0, tenant=1; order `created_at DESC, tier_rank ASC, id COLLATE "C" DESC`; seek exactly `created_at<c.created_at OR (created_at=c.created_at AND tier_rank>c.rank) OR (created_at=c.created_at AND tier_rank=c.rank AND id COLLATE "C"<c.id)`. `starting_after` is fixed-key canonical JSON `{"v":1,"created_at":int64,"tier_rank":0|1,"id":string,"workspace_uuid":uuid,"q":string|null,"limit":int}` encoded as above. Malformed/version/filter/workspace/limit mismatch is 400; stale valid boundaries may end empty. Foreign detail is 404 after workspace auth: unauthenticated 401, workspace access 403.

Measured application-data maxima after middleware auth are:

| API path | DB statements | Decryptions | Results | Encoded body | Connections |
|---|---:|---:|---:|---:|---:|
| tenant registry page | ≤5 | **0** | ≤100 | ≤512 KiB | 1 |
| global models cache hit / miss | ≤1 / ≤2 | 0 | ≤100 | ≤256 KiB | 1 |
| Fleet summary | ≤1 | 0 | ≤100 | ≤512 KiB | 1 |
| Fleet detail | ≤2 | 0 | 1 | ≤1 MiB | 1 |

Projection returns `UZ-LIBRARY-005` (500) if encoding would exceed its ceiling; it never truncates. With `limit` capped at 100 and per-item projection bounded, the ceiling is unreachable in normal operation, so a production firing is a defect rather than a user-facing outcome.

- **Dimension 3.1** — Fleet matching, seek, identity, and foreign detail status are exact → Test `test_fleet_keyset_and_detail_status` — **IN_PROGRESS.** The ordering half is implemented and unit-tested: `http/handlers/library/fleet_keyset.zig` owns the `created_at DESC, tier_rank ASC, id DESC` order and the seek predicate that resumes it, with `library_keyset_test.zig` exercising every tie combination (the three comparisons are each unreachable until the keys before them tie, so distinct fixtures hide two of the three possible direction errors). Route matchers land in `http/route_matchers_library.zig` + `route_matchers_library_test.zig`. **Not yet done:** the merged two-table query, the detail handler, and the 401/403/404 ladder — so the named integration test is not written.
- **Dimension 3.2** — every path stays within the numeric table → Test `test_library_read_resource_bounds` — **IN_PROGRESS.** The tenant registry row of the table is now measured and asserted; the other three rows are blocked on handlers that do not exist. `observability/library_read_counters.zig` owns the tallies and the maxima, and gained a measured WINDOW (`beginRead`/`endRead`) so the statement tally could move to `db/pg_query.zig` — the one point every row-returning query passes through. Counting there rather than in the handler is what makes the budget a claim about the whole call graph instead of about the statements an author remembered to count; the window opens at handler entry, which is exactly §3's "after middleware auth" boundary. `http/library_read_bounds_integration_test.zig` drives the real HTTP route (two of the five columns — connections and encoded body — do not exist below the handler) and pins **5 statements, 0 decryptions, results bounded by `limit`, 1 connection, and `encoded_bytes` equal to the bytes the client actually received**. The empty page is pinned separately at 4, because `vault.loadMetadata` returns before querying when there are no rows to describe, and a "still under budget" assertion would not notice that guard being deleted. `http/response_size.zig` measures the encoded body before it exists, so `tenant_model_entries_list.zig` now refuses an over-ceiling page with `UZ-LIBRARY-005` instead of truncating. **Not yet done:** the global-models, Fleet summary, and Fleet detail rows — writing them now would assert a budget for routes that 404, which passes for the wrong reason and keeps passing once the handler lands.

### §4 — Metadata sinks and synchronized surfaces

Secret/credential metadata carried by authenticated HTTP/UI is limited to canonical `secret_ref`, provider, kind, base URL, `has_key`, required/failing credential names, and presence booleans. Non-secret model page fields and the exact Fleet summary/detail fields in §Interfaces are also permitted. Neither field set may enter logs, traces, metrics, analytics, observable cache keys, or benchmark artifacts. Encrypted ciphertext may persist only in the vault; secret values/API keys never leave securely erased trusted Zig memory. Update routing, OpenAPI, CLI, public inventories, architecture, and consumers atomically; `make check-openapi` is the OpenAPI command.

`route_matchers_library.zig` exports `matchWorkspaceFleetLibraries(Path) ?workspace_id` for the three-segment collection and `matchWorkspaceFleetLibraryDetail(Path) ?{workspace_id,tier,id}` for the five-segment detail; router checks detail before collection. Admin catalogue matching remains in its existing owner. `route_matchers_test.zig` pins segment counts, tier enum, encoded IDs, methods, and near misses.

- **Dimension 4.1** — allowed HTTP metadata and forbidden sinks are enforced → Test `test_library_secret_and_metadata_sink_policy` — **IN_PROGRESS.** `http/handlers/library/library_sink_policy_test.zig` enforces the allowed field set structurally, over the struct definitions via `@typeInfo`, rather than by driving one request with one sentinel — the latter catches a leak only on the path a test happens to exercise, with the value it happens to choose. Both a deny list (secret-shaped names) and an allow list (any new credential-derived field must be classified deliberately) apply, because a deny list alone misses a field called `credential_blob`. Covers `EntryView` and the write-time `metadata.Projection`, and self-tests its own matcher so a broken guard cannot pass silently. **Not yet done:** the log/trace/metric/cache-key sink scan — the forbidden-egress half of the row.
- **Dimension 4.2** — routes and all published/consumer inventories agree → Test `test_library_operation_surfaces_are_synchronized` — **IN_PROGRESS.** The route half exists: `http/route_matchers_library.zig` exports the collection and detail matchers, validating the tier at the matcher so an unknown value makes the route not exist rather than reaching a handler that would use it as a selector. §4's "router checks detail before collection" is satisfied **structurally** — the shapes differ in segment count, so no path matches both and evaluation order cannot matter; `route_matchers_library_test.zig` asserts that mutual exclusion directly, which is stronger than an ordering rule because a shape difference is a property of the matchers rather than of the call site. **Not yet done:** OpenAPI, CLI, UI, and the public inventories (`llms.txt`, `skill.md`, `agentsfleet-manifest.json`) are untouched, so the surfaces are not yet synchronized and the routes are not registered in `route_table.zig`.

## Interfaces

All lists use `?starting_after=&limit=50`, unpadded-base64url compound cursors, and `{items,total:null,next_cursor}`. Model pages additionally retain top-level `version:string` as the documented list-envelope exception.

`FleetSummary={tier:"platform"|"tenant",id,name,description,created_at}`.
`FleetDetail={tier,id,name,description,created_at,source_ref,requirements:{credentials,tools,network_hosts,trigger_present},required_credentials_reasons:Record<string,string>,support_files:[{path,size_bytes}],credential_presence:[{name,present}],missing_credentials:string[]}`.
`GET /v1/workspaces/{workspace_uuid}/fleet-libraries/{tier}/{id}` returns that single resource or RFC7807 401/403/404/503; summary never contains requirements/support/presence fields.

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
| 2.2 | **unit** | `test_response_cache_accounting_and_lru` | byte accounting per §2, 256-entry and 8 MiB ceilings, true LRU eviction order, 60-second monotonic TTL, over-budget bypass |
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
| R1 | Data/security tests pass | `make test-integration` | exit 0 | P0 | |
| R2 | OpenAPI and CLI agree | `make check-openapi && make test-unit-cli` | exit 0 | P0 | |
| R3 | Diff is scoped | `git diff --name-only origin/main` | 0 unlisted paths | P0 | |
| S1 | Unit/lint/conform | `make test-unit-all && make lint-all && make harness-verify` | exit 0 | P0 | |
| S2 | Memory/build/secrets | `make memleak && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run verbatim; record ✅/❌ and one decisive line. Every P0 must pass.

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
- **Deferrals** — none.

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

- **The tenant registry row said ≤4 statements; it is 5.** The spec contradicted itself: §3's table was drafted at ≤4, while §1's own Discovery entry recorded the implemented read as *"five statements, zero decryptions"*. The table is now ≤5, because 5 is what the instrumentation measures and the five are each load-bearing — `activeSelfManagedRef` (which entry is active), `listPage` (the page, over-fetched by one), `resolvePrimaryWorkspace` (once for the page), `loadMetadata` (one batch for every row), `platformDefaultView` (the Default row's identity). Nothing was added to reach 5 and nothing can be removed to reach 4 without dropping a field the page renders. Correcting the code to match a number drafted before the shape was known would have been fitting the read to the spec's arithmetic rather than the other way round.
- **An empty page costs 4, and that is pinned separately.** `loadMetadata` returns before querying when there are no rows to describe. Asserting only "≤ 5" would let that guard clause be deleted in a refactor without a single test noticing — the result is still under budget.
- **The counters count where the queries are, not where the handler is.** The statement tally moved into `db/pg_query.zig`. §3's claim is about a call graph ("this read issues at most five statements"), and a claim about a call graph has to be counted at the bottom of it; a tally the handler increments only ever counts what its author remembered. The `beginRead`/`endRead` window is what keeps that global hook scoped to one endpoint, and opening it at handler entry — after the middleware chain — is what makes it mean what §3 says it means.
- **NFKC moves to SQL.** Zig's std ships no Unicode normalization tables and no dependency supplies them. Postgres `normalize(text, NFKC)` is built in and immutable, so `lower(normalize(col,NFKC))` is index-eligible. Zig keeps trim, whitespace-collapse, the byte bound, and LIKE-escaping — all ASCII-safe. Amends §2/§3's "normalize in the handler" to "normalize in the comparison"; user-visible behaviour is unchanged.

### The memleak lane's futex failure, reproduced and closed

- **The red check was real, and it was not this branch's defect.** Every failing run reported `0 failed` tests and exactly one valgrind error: `Syscall param futex(futex) points to unaddressable byte(s)` at `Io.Threaded.Future.start` (`Threaded.zig:757`) via `worker` (`Threaded.zig:1797`), address "on thread 1's stack … below stack pointer". The identical signature failed a docs-only branch (run 30149190866), and `make/bench.mk` already carries the owner's verdict on it — the earlier suppression was reverted because *"valgrind was right"*: Zig 0.16.0's awaiter parks on a futex word in its own stack frame, and the worker publishes the wake condition **before** dereferencing that word (`Threaded.zig:760-762`), so the awaiter can return and pop the frame first. A genuine upstream use-after-scope, deliberately unsuppressed.
- **What still created futures after the repo removed its own async call sites:** `std.Io.net.HostName.zig:283/343/353` — `HostName.connect` internally spawns `io.async` futures for the lookup and the parallel connect attempts, with no shortcut for IP literals. Every loopback dial in the test suite enters it.
- **Exactly two tests dialled on a test-local multi-threaded io** — `otlp/Client_test.zig` and the redis handshake-stages test — both relics of the removed `Io.Select` raced-dial era. Every other test already used `common.globalIo()`, whose `.failing` pool allocator makes `io.async` execute inline and return no future (`Threaded.zig:2089-2093`): no worker thread exists, so the wake-after-return race is structurally unrepresentable, not merely unlikely.
- **Reproduced before fixing, in the CI image locally** (`ci-zig-debian-trixie:0.16.0`, amd64 under emulation): the handshake-stages test filtered into its own binary went red **14/30 runs** under the exact `VALGRIND_LEAK_GATE` flags, all 43 tests passing on every run. After the io swap: **0/60** on the same loop, and 0/60 on the otlp-filtered binary. The full test binary A/B agrees: stock **2/5** red with frames identical to the CI failures, fixed **0/8**. Native `zig build test` on both edited files: green.
- **One unattributed observation, recorded rather than hidden:** a single fixed-full run printed the runner's `1 tests leaked memory` line with no valgrind error. It never recurred — seven subsequent full runs and 120 filtered runs (all under `std.testing.allocator`) were leak-clean, and no CI memleak run in the branch's history carries that line. Not attributable to the two edited files on this evidence; if it is real, a CI recurrence will name the test in full output.
- **The otlp 200-test's valgrind skip is removed**: its stated reason — a successful fetch spawning a worker whose glibc thread-local block reads as "possibly lost" — cannot occur on the serial io, so the success path is now leak-audited under the lane instead of skipped.
- **Residual, accepted and named:** the boot→drain lane runs the real daemon on a real multi-threaded io, whose live dials still enter `HostName.connect`; `exporter_test`, `subscription_hub_test`, `redis_pool_test`, and the runner suites keep real `Threaded` ios their subjects genuinely need (flush-thread spawn, hub fan-out, `std.process.run`'s allocator). Those can still lose the upstream race; none has been observed to. The durable close is an upstream report of the `Threaded.zig` wake-after-return defect.
