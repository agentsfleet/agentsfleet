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
**Status:** PENDING
**Priority:** P1 — library reads repeat database and decrypt work and expose unbounded behavior
**Categories:** API, CLI
**Batch:** B1 — establishes interfaces consumed by later workstreams
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
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
| `schema/035_model_library_revision.sql`; `schema/embed.zig` | CREATE/EDIT | Database-owned catalogue generation. |
| `public/openapi/root.yaml`; `paths/models.yaml`; `paths/fleet-library.yaml`; `components/schemas.yaml`; `public/openapi.json` | EDIT | Exact paths, pages, headers, errors, and generated API. |
| `public/llms.txt`; `public/skill.md`; `public/agentsfleet-manifest.json` | EDIT | Exact public operation inventories. |
| `cli/src/lib/api-paths.ts`; `commands/fleet_library.ts`; `commands/fleet_install_source.ts`; `commands/fleet_install.ts`; related tests | EDIT | Tier-qualified paged API use. |
| `ui/packages/app/lib/api/model_library.ts`; `lib/api/fleet-library.ts`; model/Fleet Server Actions; admin consumers | EDIT | Typed consumers and server-brokered detail; no broad types growth. |
| `docs/REST_API_DESIGN_GUIDELINES.md`; `docs/architecture/billing_and_provider_keys.md`; `fleet_bundles.md`; `data_flow.md`; `user_flow.md`; `docs/AUTH.md` | EDIT | Compound-keyset exception, billing, lock, and metadata rules. |
| `model_library_integration_test.zig`; admin model integration tests; `catalog_integration_test.zig`; `tenant_model_entries_integration_test.zig`; `state/model_rate_cache_integration_test.zig`; `route_scopes_test.zig`; `route_matchers_test.zig`; Fleet/API/CLI tests | EDIT/CREATE | Cursor, cache, race, routing, bounds, and sink proof. |

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
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | typed errors, redacted sinks, transaction cleanup, embedded migration |

## Prior-Art / Reference Implementations

- **API:** Events/Approvals keysets and `docs/REST_API_DESIGN_GUIDELINES.md`.
- **Secrets:** `state/vault.zig::markExisting`; **Fleet identity:** `docs/architecture/fleet_bundles.md`.

## Sections (implementation slices)

### §1 — Tenant registry page and reference transactions

Amend the API guideline before route work to permit opaque compound keysets where a resource ID alone cannot preserve a filtered mixed-key order; retain the standard request name `starting_after`, default 50/max 100, and exact list envelope `{items,total,next_cursor}` with `total:null`. `GET /v1/tenants/me/models?limit=50&starting_after=` orders `created_at DESC, id COLLATE "C" DESC`. Its cursor is unpadded base64url of UTF-8 canonical JSON with fixed key order/types `{"v":1,"created_at":int64,"id":string,"tenant_uuid":uuid,"limit":int}`. Malformed/version/tenant/limit mismatch is 400; a valid stale boundary continues and may return empty. Project only the current page and decrypt each distinct page `secret_ref` once.

Every reference producer—POST model creation, provider activation, `tenant_provider.upsertSelfManaged`, and `ensureEntry`—and deletion uses one transaction and exact lock order: `vault.secrets(workspace_id,key_name) FOR UPDATE`; matching `core.tenant_model_entries ORDER BY id FOR UPDATE`; `core.tenant_model_selection FOR UPDATE`; validate/mutate; commit. Producer-first makes delete reject; delete-first makes producer observe absence and roll back.

- **Dimension 1.1** — tenant keyset and current-page projection are exact → Test `test_tenant_registry_page_is_bounded`
- **Dimension 1.2** — all producers and delete serialize safely → Test `test_secret_reference_paths_serialize`

### §2 — Global catalogue generation, search, and caches

The model API has only `q` and provider filters. Normalize `q` with NFKC, trim, whitespace collapse, casefold; normalized empty means absent; reject over 128 UTF-8 bytes. Provider trims ASCII whitespace, lowercases, treats empty as absent, and remains arbitrary catalogue text—an unknown provider is valid and returns no matches. Match a literal substring with escaped SQL LIKE wildcards over normalized `display_name=model_id` and `vendor=provider`. Sort normalized display, normalized vendor, and id ascending, each `COLLATE "C"`. `starting_after` is fixed-key canonical JSON `{"v":1,"display_key":string,"vendor_key":string,"id":string,"q":string|null,"provider":string|null,"limit":int}` encoded as above. Malformed/version/filter/limit mismatch is 400; a stale valid boundary may return empty.

After authentication, every request reads the database catalogue revision before cache selection. A mutation locks the singleton revision row, mutates the catalogue and increments revision in one transaction, builds a candidate tagged with the returned revision, commits, then publishes only when its revision exceeds the process's published generation; rollback/commit failure discards it. Billing uses its existing connection to reconcile revision and atomically copy a rate from the cache generation it observed; rebuild failure fails closed, never bills stale. Revision-read failure returns typed 503 without cached data. The existing top-level catalogue `version` string remains in every 200 page; internal `catalogue_revision` is not exposed.

The response cache key is revision plus an unlogged HMAC-SHA-256 digest, under a process-random key, of canonical q/provider/starting_after/limit selectors; no raw selector or credential metadata enters keys. It is true LRU, at most 256 entries and 8 MiB including key/value/allocator metadata, with a monotonic 60-second TTL. It contains only non-secret model responses; Fleet is never cached. Allocation failure or over-budget responses bypass insertion. A strong ETag hashes exact bytes. Both 200/304 send `ETag`, `Cache-Control: private, no-cache`, `Vary: Authorization`; after auth/revision, `If-None-Match: *`, exact, or weak list match returns bodyless 304, otherwise 200.

The billing decision linearizes at its revision read: under the rate-cache mutex, reconcile to that exact generation and copy the selected rate before unlock. A later catalogue commit applies only to later revision reads; concurrent publishers may never replace a newer generation with an older one.

Rate-cache identity is a collision-safe structured `(provider,model_id)` key, never delimiter concatenation. Migration tests include provider/model strings containing the current `0x1f` separator and prove distinct tuples cannot alias or select another rate.

- **Dimension 2.1** — normalized search/keyset and headers are exact → Test `test_model_page_and_conditional_headers`
- **Dimension 2.2** — response and billing caches converge or fail closed → Test `test_catalogue_revision_governs_both_caches`

### §3 — Fleet keyset, detail, and measured ceilings

Fleet `q` uses the same normalization, 128-byte maximum, empty-as-absent, and escaped literal substring matching only id/name/description. Set `tier_rank`: platform=0, tenant=1; order `created_at DESC, tier_rank ASC, id COLLATE "C" DESC`; seek exactly `created_at<c.created_at OR (created_at=c.created_at AND tier_rank>c.rank) OR (created_at=c.created_at AND tier_rank=c.rank AND id COLLATE "C"<c.id)`. `starting_after` is fixed-key canonical JSON `{"v":1,"created_at":int64,"tier_rank":0|1,"id":string,"workspace_uuid":uuid,"q":string|null,"limit":int}` encoded as above. Malformed/version/filter/workspace/limit mismatch is 400; stale valid boundaries may end empty. Foreign detail is 404 after workspace auth: unauthenticated 401, workspace access 403.

Measured application-data maxima after middleware auth are:

| API path | DB statements | Decryptions | Results | Encoded body | Connections |
|---|---:|---:|---:|---:|---:|
| tenant registry page | ≤4 | ≤distinct page refs (≤100) | ≤100 | ≤512 KiB | 1 |
| global models cache hit / miss | ≤1 / ≤2 | 0 | ≤100 | ≤256 KiB | 1 |
| Fleet summary | ≤1 | 0 | ≤100 | ≤512 KiB | 1 |
| Fleet detail | ≤2 | 0 | 1 | ≤1 MiB | 1 |

Projection returns a typed failure if encoding would exceed its ceiling; it never truncates.

- **Dimension 3.1** — Fleet matching, seek, identity, and foreign detail status are exact → Test `test_fleet_keyset_and_detail_status`
- **Dimension 3.2** — every path stays within the numeric table → Test `test_library_read_resource_bounds`

### §4 — Metadata sinks and synchronized surfaces

Secret/credential metadata carried by authenticated HTTP/UI is limited to canonical `secret_ref`, provider, kind, base URL, `has_key`, required/failing credential names, and presence booleans. Non-secret model page fields and the exact Fleet summary/detail fields in §Interfaces are also permitted. Neither field set may enter logs, traces, metrics, analytics, observable cache keys, or benchmark artifacts. Encrypted ciphertext may persist only in the vault; secret values/API keys never leave securely erased trusted Zig memory. Update routing, OpenAPI, CLI, public inventories, architecture, and consumers atomically; `make check-openapi` is the OpenAPI command.

`route_matchers_library.zig` exports `matchWorkspaceFleetLibraries(Path) ?workspace_id` for the three-segment collection and `matchWorkspaceFleetLibraryDetail(Path) ?{workspace_id,tier,id}` for the five-segment detail; router checks detail before collection. Admin catalogue matching remains in its existing owner. `route_matchers_test.zig` pins segment counts, tier enum, encoded IDs, methods, and near misses.

- **Dimension 4.1** — allowed HTTP metadata and forbidden sinks are enforced → Test `test_library_secret_and_metadata_sink_policy`
- **Dimension 4.2** — routes and all published/consumer inventories agree → Test `test_library_operation_surfaces_are_synchronized`

## Interfaces

All lists use `?starting_after=&limit=50`, unpadded-base64url compound cursors, and `{items,total:null,next_cursor}`. Model pages additionally retain top-level `version:string` as the documented list-envelope exception.

`FleetSummary={tier:"platform"|"tenant",id,name,description,created_at}`.
`FleetDetail={tier,id,name,description,created_at,source_ref,requirements:{credentials,tools,network_hosts,trigger_present},required_credentials_reasons:Record<string,string>,support_files:[{path,size_bytes}],credential_presence:[{name,present}],missing_credentials:string[]}`.
`GET /v1/workspaces/{workspace_uuid}/fleet-libraries/{tier}/{id}` returns that single resource or RFC7807 401/403/404/503; summary never contains requirements/support/presence fields.

## Failure Modes

| Mode | Cause | Injection | Handling | Named test |
|---|---|---|---|---|
| Invalid cursor/query | malformed/mismatch/oversize | cursor/query fixtures | 400, no fallback | `test_invalid_library_inputs` |
| Revision/rebuild failure | DB or candidate fault | failpoint | 503/fail billing closed; no stale publish | `test_catalogue_revision_governs_both_caches` |
| Projection/encoding fault | decrypt/allocation/body ceiling | allocator/envelope fixtures | typed failure, zero/free, no truncation | `test_projection_failures_are_safe` |
| Reference race | producer/delete interleave | transaction barriers | valid serialization; loser rejects/rolls back | `test_secret_reference_paths_serialize` |
| Pool/query fault | acquire/SQL failure | pool/SQL failpoint | typed transient; connection released | `test_library_pool_query_failure` |
| Foreign detail | valid auth, foreign entry | foreign fixture | non-enumerating 404 | `test_fleet_keyset_and_detail_status` |
| Stale boundary | boundary row changes | mutation fixture | continue, possibly empty | `test_stale_library_cursors_continue` |
| Forbidden egress | sentinel reaches sink | sink capture | test rejects emission | `test_library_secret_and_metadata_sink_policy` |

## Invariants

1. All pages, work, connections, and bodies obey §3 limits; typed builders enforce failure rather than truncation.
2. One catalogue generation governs response and billing caches; transaction and billing checks enforce it.
3. One lock order governs all reference producers/deletes; transaction helpers enforce it.
4. Sink-safe metadata types and sentinel scans enforce §4.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| test-only resource counters | M143_003 | bounded tests run | counts/bytes only | no identifiers or metadata | `test_library_read_resource_bounds` |

## Test Specification (tiered)

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

## Discovery (consult log)

- **Consults** — Oracle second-pass blockers incorporated exactly.
- **Metrics review** — production telemetry belongs to M143_003; no funnel change.
- **Skill-chain outcomes** — populated during implementation.
- **Deferrals** — none.
