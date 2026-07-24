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

# M143_001: Library pages become bounded, fluid, and secret-safe

**Prototype:** v2.0.0
**Milestone:** M143
**Workstream:** 001
**Date:** Jul 24, 2026
**Status:** PENDING
**Priority:** P1 — authenticated Model and Fleet Library navigation currently blocks on unbounded and duplicate work
**Categories:** API, Observability (OBS), User Interface (UI)
**Batch:** B1 — one end-to-end read-path refactor with measurement before optimization
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** M139_004 — its telemetry naming and cardinality rules must land before this work adds stage timings
**Provenance:** agent-generated after Oracle architecture review (Jul 24, 2026)
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §§8.2, 8.3, 10

---

## Overview
**Goal (testable):** Authenticated Models and Fleet Library navigation streams stable content, performs bounded set-oriented reads, never exposes decrypted secret material, and meets the pinned latency and pool-fairness budgets.
**Problem:** Models repeats workspace and secret reads per registry entry, decrypts the workspace secret list in parallel, and fetches the full catalogue after hydration. Fleet Library blocks browsing on decrypting every workspace secret, runs serial unpaged catalogue reads with repeated JSON decoding, and mounts every card. Generic spinners hide progress, broad catches misreport failures as empty states, cross-request cache semantics are absent, and the application duplicates Clerk session refresh work.
**Solution summary:** Measure the full authenticated path; collapse model projection into bounded bulk work; split model and Fleet Library reads into paged summary/search and selected detail; check only selected credential names without decryption; cache only global non-secret projections after authorization with strong Entity Tags (ETags) and cross-replica invalidation; stream stable skeletons and retain stale data; preserve typed failures; and remove the redundant session keeper after session-continuity proof.

## PR Intent & comprehension handshake
- **PR title (eventual):** feat(libraries): make authenticated reads fluid and bounded
- **Intent (one sentence):** Users can browse and act on Models and Fleet Library immediately without waiting on unrelated secret work or losing security isolation.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first
1. `docs/AUTH.md` — the server-side JavaScript Object Signing and Encryption Web Token (JWT) boundary and deferred Backend-for-Frontend (BFF) direction must remain intact.
2. `docs/architecture/billing_and_provider_keys.md` §§8.2, 8.3, 10 — secret projection, tenant model registry, and model catalogue ownership.
3. `docs/architecture/data_flow.md` fleet-install section — install checks credential names only and never resolves secret values.
4. `docs/architecture/observability.md` — bounded OpenTelemetry and World Wide Web Consortium (W3C) trace ownership.
5. `src/agentsfleetd/http/handlers/approvals/list.zig` plus the dashboard Approvals page — bounded API and streamed UI prior art.

## Files Changed (blast radius)
| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/state/secret_probe.zig` | EDIT | Resolve the primary workspace once and bulk-project distinct registry secret references. |
| `src/agentsfleetd/http/handlers/tenant_model_entries_view.zig` | EDIT | Replace per-entry secret reads with one bounded projection. |
| `src/agentsfleetd/http/tenant_model_entries_integration_test.zig` | EDIT | Prove constant statement count, shared references, redaction, and failures. |
| `src/agentsfleetd/state/model_library/sql.zig` | EDIT | Add bounded provider/search/keyset projections and snapshot generation. |
| `src/agentsfleetd/state/model_library_store.zig` | EDIT | Map bounded pages and own immutable non-secret snapshots. |
| `src/agentsfleetd/http/handlers/model_library.zig` | EDIT | Serve authenticated conditional catalogue searches. |
| `src/agentsfleetd/http/handlers/model_library_integration_test.zig` | EDIT | Prove pagination, ETag, authorization, invalidation, and limits. |
| `src/agentsfleetd/fleet_library/sql.zig` | EDIT | Replace serial gallery reads with one aligned keyset union and detail projection. |
| `src/agentsfleetd/http/handlers/library/gallery.zig` | EDIT | Serve bounded summaries on one connection. |
| `src/agentsfleetd/http/handlers/library/entry_view.zig` | EDIT | Separate summary mapping from selected detail JSON decoding. |
| `src/agentsfleetd/http/handlers/library/gallery_detail.zig` | CREATE | Return one authorized entry detail plus non-decrypting required-credential presence. |
| `src/agentsfleetd/fleet_library/gallery_snapshot.zig` | CREATE | Own the atomically swapped global platform summary/detail snapshot. |
| `src/agentsfleetd/http/handlers/library/catalog.zig` | EDIT | Page the operator catalogue and invalidate platform snapshots after committed writes. |
| `src/agentsfleetd/http/handlers/library/catalog_patch.zig` | EDIT | Invalidate snapshots only after successful mutation. |
| `src/agentsfleetd/http/handlers/library/onboard.zig` | EDIT | Invalidate snapshots after committed onboarding. |
| `src/agentsfleetd/http/handlers/library/catalog_integration_test.zig` | EDIT | Prove union paging, detail isolation, presence checks, and cache coherence. |
| `src/agentsfleetd/http/handlers/library/catalog_etag_integration_test.zig` | EDIT | Prove conditional reads still authenticate and authorize. |
| `src/agentsfleetd/http/router.zig` | EDIT | Match workspace Fleet Library detail by canonical segments. |
| `src/agentsfleetd/http/routes.zig` | EDIT | Add the typed detail route. |
| `src/agentsfleetd/http/route_table.zig` | EDIT | Apply bearer and route-scope middleware to detail. |
| `src/agentsfleetd/http/route_table_invoke_library.zig` | EDIT | Invoke the workspace summary/detail handlers. |
| `src/agentsfleetd/observability/metrics.zig` | EDIT | Export fixed-cardinality read-stage measurements. |
| `src/agentsfleetd/observability/metrics_render.zig` | EDIT | Render bounded latency/pool/cache families. |
| `src/agentsfleetd/observability/metrics_trace_test.zig` | EDIT | Reject sensitive or high-cardinality stage attributes. |
| `src/agentsfleetd/http/server.zig` | EDIT | Measure warm JWT verification and normalized route stages without raw paths. |
| `tests/bench/micro.zig` | EDIT | Add catalogue mapping, serialization, and warm verification benchmarks. |
| `make/bench.mk` | EDIT | Extend the existing benchmark lane rather than add a duplicate target. |
| `ui/packages/app/lib/api/client.ts` | EDIT | Time normalized upstream requests and propagate available W3C trace context. |
| `ui/packages/app/lib/api/model_library.ts` | EDIT | Expose bounded model search and conditional-read types. |
| `ui/packages/app/lib/api/fleet-library.ts` | EDIT | Expose paged summaries and selected details; remove misleading request-only cache naming. |
| `ui/packages/app/lib/types.ts` | EDIT | Split Fleet Library summary/detail and cursor shapes. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/page.tsx` | EDIT | Stream registry content without catalogue or full-secret barriers. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/lib/reads.ts` | EDIT | Remove the initial full secret read. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/actions.ts` | EDIT | Search models lazily and preserve typed failures. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider.tsx` | EDIT | Become an intent-triggered paged search state with retained results. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderModelSelect.tsx` | EDIT | Keep one stable control while options load. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/loading.tsx` | EDIT | Render a stable registry skeleton. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/page.tsx` | EDIT | Stream summaries and resolve deep links without listing secrets. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/InstallFleet.tsx` | EDIT | Initialize deep-link selection synchronously and fetch selected detail. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/InstallSourceSelector.tsx` | EDIT | Page summaries and retain cards during refresh. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/InstallConfirm.tsx` | EDIT | Render selected credential presence without secret values. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/loading.tsx` | EDIT | Replace the generic spinner with a stable card skeleton. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/page.tsx` | EDIT | Consume bounded operator pages with typed errors. |
| `ui/packages/app/app/layout.tsx` | EDIT | Remove the redundant session-keeper mount. |
| `ui/packages/app/lib/auth/client.ts` | EDIT | Remove application-owned periodic session reload. |
| `ui/packages/app/lib/auth/client.test.tsx` | EDIT | Replace timer tests with the absence of application-owned refresh behavior. |
| `ui/packages/app/lib/analytics/events.ts` | EDIT | Add a sanitized library-ready timing event with no identifiers. |
| `ui/packages/app/tests/models-registry-table.test.tsx` | EDIT | Prove streamed registry and lazy stable picker behavior. |
| `ui/packages/app/tests/fleets-install-flow.test.ts` | EDIT | Prove summary/detail and selected-only credential checks. |
| `ui/packages/app/tests/fleet-library-api.test.ts` | EDIT | Pin cursor/detail/ETag request shapes. |
| `ui/packages/app/tests/loading-states.test.ts` | EDIT | Pin route-specific structural skeletons. |
| `ui/packages/app/tests/analytics-events.test.ts` | EDIT | Prove timing-event allow-list and sensitive-property rejection. |
| `ui/packages/app/tests/e2e/acceptance/performance-library-navigation.spec.ts` | CREATE | Walk authenticated Models/Fleet navigation, errors, deep links, and retained transitions. |
| `ui/packages/app/tests/e2e/acceptance/performance-loading-motion.spec.ts` | CREATE | Emulate reduced motion and prove loading transitions remain stable. |
| `public/openapi/paths/models.yaml` | EDIT | Document model search, cursor, limit, and conditional semantics. |
| `public/openapi/paths/fleet-library.yaml` | EDIT | Document summary/detail pages and credential-presence projection. |
| `public/openapi/components/schemas.yaml` | EDIT | Add bounded page and detail schemas. |
| `public/openapi/root.yaml` | EDIT | Register the Fleet Library detail path. |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | Replace per-session full catalogue and repeated secret projection claims. |
| `docs/architecture/data_flow.md` | EDIT | Record summary/detail install reads and name-only presence checks. |
| `docs/architecture/observability.md` | EDIT | Record library timing, cache, cardinality, and privacy rules. |

## Applicable Rules
- **`docs/greptile-learnings/RULES.md`** — GRD, VLT, CNX, WAUTH, RTM, FLS, FLL, UFS, NDC, NLR, NLG, ORP, ITF, TNM, ASE, DID, and PTK.
- **`dispatch/write_zig.md`** — PostgreSQL drain discipline, allocator ownership, snapshot concurrency, public-surface shape, and both Linux builds.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** §§1–5, 7, 8, 11, 12 — bounded pagination, Request for Comments (RFC) 7807 errors, segment routes, authorization, and measured performance.
- **`dispatch/write_ts_adhere_bun.md` and `docs/DESIGN_SYSTEM.md`** — stable component shape, promise handling, design primitives, tokens, motion, and reduced-motion behavior.
- **`docs/AUTH.md` and `docs/LOGGING_STANDARD.md`** — preserve server-only token transport and redact credentials from every signal.

## Applicable Gates
| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | format, focused tests, full unit/integration, memory gate, and both Linux targets |
| PUB / Struct-Shape | yes | PLAN records shape for snapshot/page/detail types before edits |
| File & Function Length (≤350/≤50/≤70) | yes | keep query, cache, projection, handler, and UI state responsibilities separate |
| UFS (repeated/semantic literals) | yes | shared cursor limits, cache generation, headers, metrics, and event names use canonical constants |
| UI Substitution / DESIGN TOKEN | yes | design-system skeletons/transitions only; no arbitrary values; reduced-motion variant required |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | logging and lifecycle | no new error code or schema by default; zero owned buffers and snapshot teardown; redact all signal fields |

## Prior-Art / Reference Implementations
- **Bounded reads:** Approvals and Events API/UI paths — keyset pages, hard limits, Suspense data regions, and stable headers.
- **Safe credential presence:** `src/agentsfleetd/state/vault.zig::markExisting` — one non-decrypting query over selected names.
- **Immutable cache:** `src/agentsfleetd/state/model_rate_cache.zig` — build off-lock and atomically swap; this spec adds multi-replica invalidation and post-auth serving.
- **Conditional responses:** `src/agentsfleetd/http/etag.zig` plus Fleet and platform-catalog handlers — strong ETag and `If-Match`/header ownership.

## Sections (implementation slices)
### §1 — Latency is attributable and bounded
Measure normalized Next fetch, warm JWT verification, pool wait, authorization, SQL, secret projection, JSON mapping, serialization, and cache outcome. Attributes are fixed enums and route classes; raw paths, query text, identifiers, tokens, secret names, and plaintext are prohibited. Extend the existing benchmark lane with production-shaped cardinalities and pool concurrency.

- **Dimension 1.1** — sampled traces and metrics account for the authenticated read stages with fixed cardinality → Test `test_library_stage_signals_are_bounded`
- **Dimension 1.2** — Next requests propagate available W3C context and emit sanitized navigation-to-usable timing → Test `test_library_timing_contains_no_identifiers`
- **Dimension 1.3** — benchmarks cover warm JWT, 1/10/100/500 model entries, mixed gallery tiers, serialization, and concurrent pool pressure → Test `test_library_benchmarks_enforce_budgets`

### §2 — Model registry projects secrets once
Resolve the tenant primary workspace once, bulk-load distinct referenced secret rows, unwrap the key-encryption key once, and decrypt each distinct value once. The response type structurally excludes `api_key`; plaintext and parsed storage are zeroed on success and every error. Model creation performs atomic collision/existence checks so the page no longer needs every workspace secret.

- **Dimension 2.1** — registry statement count remains constant while entry count grows and shared references decrypt once → Test `test_model_registry_bulk_projects_distinct_secrets`
- **Dimension 2.2** — plaintext, API keys, ciphertext, and key names never enter response, logs, traces, metrics, or analytics → Test `test_model_registry_secret_material_is_nonobservable`
- **Dimension 2.3** — model writes remain race-safe without a client full-secret preflight → Test `test_model_create_resolves_secret_collision_atomically`

### §3 — Model catalogue is lazy, paged, and conditionally cached
`GET /v1/models` remains bearer-authenticated and gains provider/search filters, a hard limit, opaque keyset cursor, exact generation, and strong ETag. A normal Models visit renders the registry without this request. Add/Edit intent opens one stable search control, prefetches on focus/hover, retains prior results during revalidation, and never morphs an edited input.

- **Dimension 3.1** — catalogue pages are stable, bounded, searchable, and reject malformed cursors/limits → Test `test_model_library_keyset_search_is_bounded`
- **Dimension 3.2** — authenticated snapshot hits and 304 responses recheck auth and invalidate after every committed admin mutation → Test `test_model_snapshot_is_post_auth_and_coherent`
- **Dimension 3.3** — page load makes no catalogue request; picker intent lazy-loads without layout or input replacement → Test `test_model_picker_loads_on_intent_without_morphing`

### §4 — Fleet Library separates summary, detail, and credential presence
One authorization statement and one `UNION ALL` keyset query return compact summaries. One selected-detail read decodes expanded requirements and checks only required credential names through `markExisting`; it never decrypts. The operator catalogue is also bounded. Global platform projections use immutable snapshots; tenant rows and credential presence remain fresh.

- **Dimension 4.1** — summary pages use one connection, one gallery statement, deterministic cross-tier cursors, and hard limits → Test `test_fleet_gallery_union_is_bounded_and_fair`
- **Dimension 4.2** — selected detail returns only submitted requirement presence and performs zero decryptions → Test `test_fleet_detail_checks_required_names_without_decryption`
- **Dimension 4.3** — tenant isolation and platform/admin visibility hold across summary, detail, pagination, and cache hits → Test `test_fleet_library_pages_preserve_authorization`
- **Dimension 4.4** — committed onboard/publish/unpublish/patch/delete invalidates every replica; missed signals converge by the bounded backstop → Test `test_platform_gallery_snapshot_converges_across_replicas`

### §5 — Navigation remains useful while data changes
Models and Fleet Library stream exact headers and structural skeletons, retain authorized prior content during refresh, expose `aria-busy`, and transition without layout shift. Fleet deep links render selected detail directly. Authentication, permission, missing-resource, and transient failures have distinct UI; only successful empty arrays produce empty states. Motion is subtle and disabled when reduced motion is requested.

- **Dimension 5.1** — headers and stable table/card skeletons stream before data and prior content remains during refresh → Test `test_library_navigation_streams_stable_regions`
- **Dimension 5.2** — `?library=<id>` renders selected detail without a gallery flash or hydration effect → Test `test_fleet_library_deep_link_is_server_selected`
- **Dimension 5.3** — 401/403/404/5xx never render a successful empty state and transient retry retains stale content → Test `test_library_failures_are_typed_not_empty`
- **Dimension 5.4** — animations preserve layout and reduced-motion users receive no shimmer/transform transition → Test `test_library_loading_respects_reduced_motion`

### §6 — Session security and concurrency stay predictable
Remove `AuthSessionKeeper`; Clerk remains the session owner and JWT bytes remain server-only. Prove visible, suspended, offline/resumed, and focus-restored sessions can submit Server Actions. Cache keys never contain tokens, authorization always precedes cache/304, no request acquires two PostgreSQL connections, and the performance budgets hold under load.

- **Dimension 6.1** — session continuity passes without application timers, focus reloads, or forced token refresh → Test `test_session_continues_without_application_reload_loop`
- **Dimension 6.2** — unauthorized/cross-workspace requests cannot receive cached 200/304 and tokens never become cache keys → Test `test_library_cache_cannot_cross_auth_boundaries`
- **Dimension 6.3** — warm reads meet latency, payload, statement-count, and pool-wait budgets at concurrent load → Test `test_library_read_path_meets_performance_budgets`

## Interfaces
```text
GET /v1/models?provider=<slug>&q=<text>&limit=<1..50>&cursor=<opaque>
200 { version: <exact-generation>, models: ModelSummary[], next_cursor: string|null }

GET /v1/workspaces/{workspace_id}/fleet-libraries?limit=<1..50>&cursor=<opaque>
200 { items: FleetLibrarySummary[], next_cursor: string|null }

GET /v1/workspaces/{workspace_id}/fleet-libraries/{entry_id}
200 FleetLibraryDetail { summary fields, requirements, credential_presence, support_files, source_ref }

GET /v1/admin/fleet-libraries?limit=<1..50>&cursor=<opaque>
200 { entries: PlatformCatalogEntry[], next_cursor: string|null }

Conditional reads: ETag on successful GET; If-None-Match may yield 304 only after authentication, scope, and workspace checks.
Credential presence contains configured booleans only for the selected entry's required names. No response type contains secret plaintext, api_key, ciphertext, object-store keys, or token bytes.
```

## Failure Modes
| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Missing/expired session | Clerk session unavailable | 401/sign-in state; no upstream cache response |
| Cross-workspace access | Principal does not own path workspace | 403 summary or non-enumerating 404 detail; no cache probe |
| Malformed cursor/limit | Invalid or oversized pagination input | RFC 7807 400; no unbounded fallback |
| Snapshot invalidation loss | Replica misses mutation signal | generation/age backstop reloads atomically; stale snapshot remains bounded |
| Snapshot rebuild failure | Database or allocation failure | retain last authorized snapshot, count failure, return typed transient error when none exists |
| Corrupt secret envelope | Referenced model secret cannot decrypt | affected metadata degrades safely; plaintext never logged; healthy rows still return |
| Secret disappears during projection | Concurrent delete after registry read | row reports unavailable metadata; write/install remains authoritative |
| Pool saturation | Concurrent reads exhaust available connections | bounded wait/typed 503; one request never holds two connections |
| Catalogue mutation race | Read overlaps publish/update/delete | page generation and cursor remain internally consistent; next read observes invalidation |
| UI refresh failure | Background revalidation fails | retain stale authorized content and expose retry, never empty success |
| Reduced-motion preference | Browser requests reduced motion | no shimmer or transform animation; status remains accessible |
| Session resumes stale | Tab wakes after token expiry/offline period | Clerk refresh path restores session or shows sign-in; no application polling loop |

## Invariants
1. Decrypted secret bytes exist only in zeroing request-owned `agentsfleetd` memory and never enter an HTTP response, log, trace, metric, analytics event, cache, benchmark artifact, or browser state — enforced by response types, zeroing allocators, sink-capture tests, and redaction audits.
2. Authentication, scope, and workspace ownership run before every snapshot hit and every 304 — enforced by middleware ordering and unauthorized conditional-read integration tests.
3. Cache keys contain canonical global generation or workspace identity only, never JWT/token bytes or secret names — enforced by typed key constructors and tests.
4. A gallery request holds at most one PostgreSQL connection and executes bounded statements — enforced by OnConn APIs, statement probes, and pool-size-one tests.
5. Every list has a server maximum, deterministic keyset order, and opaque cursor; malformed input never falls back to a full list — enforced at parse boundaries.
6. Only a successful empty API result renders an empty state; security and dependency failures remain distinguishable — enforced by typed result rendering tests.
7. No application-owned interval, focus, online, or visibility handler reloads the Clerk user/session — enforced by source and browser tests.

## Metrics & Observability
| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `agentsfleet_library_read_duration_seconds` | operations | normalized library read stage completes | fixed surface, stage, outcome, cache outcome | no identifiers, query text, JWT, secret name/value, or raw path | `test_library_stage_signals_are_bounded` |
| `agentsfleet_library_pool_wait_seconds` | operations | a library read acquires or times out | fixed surface and outcome | no workspace/tenant labels | `test_library_benchmarks_enforce_budgets` |
| `agentsfleet_library_snapshot_events_total` | operations | snapshot hit/miss/rebuild/invalidation occurs | fixed catalogue and outcome | global non-secret snapshots only | `test_platform_gallery_snapshot_converges_across_replicas` |
| `library_view_ready` | product | Models or Fleet primary content becomes usable | surface, outcome, duration bucket | no user/workspace/model/fleet/credential identifiers | `test_library_timing_contains_no_identifiers` |

## Test Specification (tiered)
| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_library_stage_signals_are_bounded` | every stage emits fixed labels; sensitive/high-cardinality candidates are rejected |
| 1.2 | unit | `test_library_timing_contains_no_identifiers` | traced fetch and ready event contain only allow-listed coarse fields |
| 1.3 | integration | `test_library_benchmarks_enforce_budgets` | production-shaped cardinalities and concurrent reads stay inside pinned ceilings |
| 2.1 | integration | `test_model_registry_bulk_projects_distinct_secrets` | 500 entries and shared refs keep constant statements and one decrypt per distinct ref |
| 2.2 | integration | `test_model_registry_secret_material_is_nonobservable` | sentinel secret is absent from response and captured sinks on success/failure |
| 2.3 | integration | `test_model_create_resolves_secret_collision_atomically` | concurrent create/delete yields one valid result and no client preflight dependency |
| 3.1 | integration | `test_model_library_keyset_search_is_bounded` | valid pages are stable; malformed cursor, zero, and over-limit requests return 400 |
| 3.2 | integration | `test_model_snapshot_is_post_auth_and_coherent` | valid ETag returns 304 after auth; mutation changes generation; unauthorized gets 401/403 |
| 3.3 | unit | `test_model_picker_loads_on_intent_without_morphing` | initial render makes no catalogue call; focus opens stable loading/select state |
| 4.1 | integration | `test_fleet_gallery_union_is_bounded_and_fair` | mixed tiers page without duplicates using one connection and bounded statements |
| 4.2 | integration | `test_fleet_detail_checks_required_names_without_decryption` | selected requirements return booleans with zero decrypt calls and no unrelated names |
| 4.3 | integration | `test_fleet_library_pages_preserve_authorization` | foreign workspace/detail/admin/cursor/cache combinations cannot disclose rows |
| 4.4 | integration | `test_platform_gallery_snapshot_converges_across_replicas` | every committed mutation signals peers; dropped signal converges by generation backstop |
| 5.1 | end-to-end | `test_library_navigation_streams_stable_regions` | authenticated navigation paints header/skeleton then content without blank refresh |
| 5.2 | end-to-end | `test_fleet_library_deep_link_is_server_selected` | direct detail URL never paints the gallery first |
| 5.3 | end-to-end | `test_library_failures_are_typed_not_empty` | injected 401/403/404/503 show distinct recovery and never empty-success copy |
| 5.4 | end-to-end | `test_library_loading_respects_reduced_motion` | emulated reduced motion disables shimmer/transforms while preserving accessible status |
| 6.1 | end-to-end | `test_session_continues_without_application_reload_loop` | visible, suspended, offline/resumed, and focused sessions submit or sign in cleanly |
| 6.2 | integration | `test_library_cache_cannot_cross_auth_boundaries` | foreign token plus matching ETag never receives another tenant's 200/304 |
| 6.3 | integration | `test_library_read_path_meets_performance_budgets` | warm p95, pool wait, payload, statements, and mounted-item caps all pass |

## Acceptance Rubric (single scoring surface)
| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Models and Fleet reads meet the benchmark budgets (§§1–4,6) | `make bench` | exit 0; warm JWT p95 <5ms, warm read p95 ≤75ms, pool-wait p99 <25ms | P0 | |
| R2 | Initial UI avoids catalogue/secret waterfalls and handles failures (§§3–5) | `bun --cwd ui/packages/app test tests/models-registry-table.test.tsx tests/fleets-install-flow.test.ts tests/loading-states.test.ts` | exit 0 | P0 | |
| R3 | Secret sentinel never reaches observable sinks (§2) | `make test-integration` | exit 0 with secret non-observability case executed | P0 | |
| R4 | Authenticated browser navigation is stable and reduced-motion safe (§§5–6) | `bun --cwd ui/packages/app run test:e2e -- tests/e2e/acceptance/performance-library-navigation.spec.ts tests/e2e/acceptance/performance-loading-motion.spec.ts` | exit 0 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint and authoring gates are clean | `make lint-all && make harness-verify` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | No leaks | `make memleak` | exit 0 | P0 | |
| S5 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | Orphan and size sweeps are clean | `make harness-verify` | exit 0 with ORP and LENGTH rows green | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep
No files are predeclared for deletion. Removed symbols must have zero references:

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `AuthSessionKeeper` | `git grep -n -w AuthSessionKeeper -- ':!docs/v2/done/**' ':!docs/v2/pending/**'` | 0 matches |
| `listSecretsCached` in Models | `git grep -n -w listSecretsCached -- 'ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/**'` | 0 matches |
| initial `getModelLibraryAction` mount path | `git grep -n 'getModelLibraryAction()' -- 'ui/packages/app/**'` | 0 matches |

## Out of Scope
- agentsfleet-native capability tokens and the full BFF migration — `docs/AUTH.md` keeps that security redesign coupled to its final token shape.
- Secret reveal APIs or browser access to secret values — this work only reduces and proves the existing non-secret projections.
- PostgreSQL schema/index changes without production-shaped `EXPLAIN (ANALYZE, BUFFERS)` evidence; a proven need requires amending Files Changed before execution.
- Public/shared Content Delivery Network caching of authenticated responses; snapshots remain inside `agentsfleetd` after authorization.

---

## Product Clarity (authoring record)
1. **Successful user moment** — clicking Models or Fleet Library immediately paints the right structure, then usable rows/cards settle without a spinner wall or control morph.
2. **Preserved user behaviour** — sign-in, scopes, model add/edit/activate, Fleet onboarding/install, admin publish/edit, secret rotation, and typed API errors keep working.
3. **Optimal-way check** — set-oriented bounded APIs plus streamed intent-driven UI remove the real waits; animation is feedback, never a substitute for latency.
4. **Rebuild-vs-iterate** — refactor the read model across UI and daemon while preserving authoritative writes and the existing authentication boundary.
5. **What we build** — stage measurements, bulk model projection, paged model search, Fleet summary/detail, selected credential presence, post-auth snapshots, conditional reads, stable loading, and session-continuity proof.
6. **What we do NOT build** — no duplicate BFF, token cache, decrypted-secret cache, shared authenticated edge cache, or second database connection per request.
7. **Fit with existing features** — compounds with Events/Approvals pagination and Suspense, vault `markExisting`, strong ETags, and model-rate snapshots; must not destabilize install or billing.
8. **Surface order** — API and UI land atomically because neither bounded payloads nor fluid rendering alone removes the complete wait.
9. **Dashboard restraint** — show skeletons and measured states only; do not claim speed in product copy or expose internal timing controls.
10. **Confused-user next step** — typed retry, sign-in, permission, not-found, and credential-connect actions replace empty-state ambiguity.

## Decomposition & alternatives (patch vs refactor)
- **Chosen shape:** six Sections follow the latency path from measurement through data projection, bounded catalogues, rendering, and security/concurrency proof so each user-visible gain has backend evidence.
- **Alternatives considered:** spinner polish alone leaves unbounded and duplicate work; adding database connections harms pool fairness; an immediate BFF duplicates authorization before capability tokens; full secret caching increases blast radius.
- **Patch-vs-refactor verdict:** this is a **refactor** because the lag is distributed across query shape, payload boundaries, cache ownership, and rendering; local loading tweaks would conceal rather than remove it.

## Discovery (consult log)
- **Consults** — Oracle reviewed the JWT, Next.js, Zig, PostgreSQL, cache, concurrency, security, and UX paths on Jul 24, 2026; verdict: auth is sound, refactor library reads and loading boundaries, and do not add a BFF now.
- **Metrics review** — add bounded operational stage/cache/pool signals and one sanitized product-ready timing event; update the analytics event registry and architecture, with no identifiers or secret material.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, and `kishore-babysit-prs` results are populated during implementation in mandatory order.
- **Deferrals** — none.
