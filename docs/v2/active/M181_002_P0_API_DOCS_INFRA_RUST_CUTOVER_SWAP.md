<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M181_002: Cutover §1 — every route serves from the Rust daemon, proven

**Prototype:** v2.0.0
**Milestone:** M181
**Workstream:** 002
**Date:** Aug 30, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — the family's payoff; everything before it is preparation
**Categories:** API | DOCS | INFRA
**Batch:** B6 — first half of the split family (siblings M181_003–006, created 2026-09-01); serial after M180_001 merged
**Branch:** `feat/m181-002-cutover-swap`
**Test Baseline:** `unit=4190 integration=not-run` — `make test-unit-all` on 2026-08-31: cargo workspace 2041 passed, `ui/packages/app` 1637, `ui/packages/design-system` 512. `verify.integration` is not run at CHORE(open): the stage table (`dispatch/lifecycle.md` line 21) declares `verify.unit` once, and the slow suites run only when the branch carries code.
**Depends on:** M180_001 **merged** (the ports extend surfaces it landed); M181_001 (the shipping binary, the lanes, the probe runner); M178_001, M179_001, M177_001, M176_001
**Provenance:** split from the single M181_001 cutover spec (LLM-drafted, Claude Fable 5, Aug 23, 2026) on the axis "needs the full route surface or does not"; this half does
**Canonical architecture:** `docs/architecture/tenant_provider_v2.md` + `docs/architecture/fleet_bundles.md`

---

## Overview

**Goal (testable):** every route × method the contract declares answers from the Rust daemon — `make test-parity LOCAL=1` green against the committed contract, the route table's declared verb set equal to what the router mounts, and every row-level outcome of the three ported surfaces graded by the integration lane.

**Problem:** six milestones of parity evidence were per-surface, and nine route × method pairs had no Rust handler at all — three feature ports, not annotations. The route surface is only gradeable once every route serves from Rust; this workstream is that surface, proven.

**Solution summary:** port the tenant provider triple, the model-entries quad and the workspace fleet-libraries pair as Rust-native constructs over the stores that own them; complete the route table with a declared verb set per route, graded against the actual mount rather than trusted; converge four crates onto the error-funnel shape; and close on the two lanes — parity RECORD mode against the committed contract, and the integration suite for everything decided from a row.

**Split record (2026-09-01):** this spec's §§2–4 and the OpenAPI tooling half of §1 moved to four sibling workstreams on Indy's parallelization call — M181_003 (the coverage gate), M181_004 (OTLP export), M181_005 (collectors under Zig), M181_006 (soak and swap). The quotes and the dependency graph are in Discovery; the sections below that moved carry pointers, not prose.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): the full route surface serves — three ports, the verb table, and the lanes that prove it
- **Intent (one sentence):** every route the contract declares serves from the Rust daemon, with the method set declared in the route table and graded against the real mount.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. §1's correction below — the served-versus-documented gate this work was once going to extend no longer exists; building its replacement is M181_003's, and only the SERVED half (the route table's verb set) lands here.
2. `rustd/crates/afd_api/src/router/mount.rs` + `router/mod.rs` — the total match from route variant to handler. §1 must not break it; the reason is in §1's implementation default.
3. Discovery, in this file — the family's decision record: the utoipa adoption and its external review (inherited by M181_003), the backfill deferral and its measurement, and the split that produced M181_003–006.
4. `docs/RUST_ERROR_STANDARD.md` — the error-funnel convergence in this diff is graded against it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_http/src/route/**` | EDIT | `Route::verbs` completes the route table — the served half of the family's gate, which no other fact answers; `Verb` split to its own module at the length cap |
| `rustd/crates/afd_api/**` · `afd_api_tenant` · `afd_api_operator` | EDIT | the three ports (tenant provider triple, model-entries quad, workspace fleet-libraries pair), their mounts, the router's test-util seam, and the suites that grade routing and refusals |
| `rustd/crates/afd_credential/**` · `afd_library` · `afd_vault` · `afd_tenant` · `afd_fleet` | EDIT | the stores the ports read and write: registry verbs, the activation ladder, the onboarding pipeline, the vault reads that never decrypt on a refusal |
| `rustd/crates/afd_wire/**` | EDIT | wire types for the ported surfaces — no schema derives; that is M181_003's |
| `rustd/crates/agentsfleetd/**` | EDIT | composition wiring for the stores the ports reach |
| `rustd/crates/afd_core/**` · `afd_crypto` · `afd_admin` · `afd_fleet_ops` | EDIT | the error-funnel convergence Indy asked for — four crates carried the hull `error_shell!` exists to generate, and the macro's own home crate was one of them |
| `docs/RUST_ERROR_STANDARD.md` | EDIT | records what `error_shell!`/`error_lifts!` apply to, and the plane crates the conformance table predated |
| `scripts/parity_lane.sh` · `scripts/parity_lane_test.sh` | EDIT | the reviewer finding on the declared-divergence branch, inherited with the merged fix — the absent side is graded against its own daemon's unmatched-route shape rather than against the other daemon. The lane is this spec's oracle, so it lands here rather than waiting for a lane fix nobody owns |
| `make/test-integration-rustd.mk` | EDIT | the lane the row-graded dimensions run under |
| `docs/architecture/**` | EDIT | the store and provider shapes the ports settled |
| `rustd/Cargo.lock` · `codecov.yml` | EDIT | dependency and coverage bookkeeping for the crates above |
| `docs/v2/pending/M181_003..006_*.md` | CREATE | the split's four sibling specs, travelling with the scope amendment that references them |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC, UFS, TST-NAM, MSID, FLL, ORP; ECL (a datastore outage in a lane is an environment condition, not a parity defect); the legacy family's PORT discipline — Zig constructs are deleted or replaced with Rust-native shapes, never transliterated.
- **`docs/RUST_ERROR_STANDARD.md`** — the ports and the error-funnel convergence are graded against it: one error type per crate, `#[from]` composition, no lossy `map_err`.
- `dispatch/write_rust.md` — ownership, preserved error variants, deterministic concurrency tests; REVIEW cites the reference guideline identifiers.
- `dispatch/write_http.md` → `docs/REST_API_DESIGN_GUIDELINES.md` — the ported handlers are public surface graded against the design guide.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the architecture edits are published prose.
- `dispatch/verify.md` — done-claims here are exactly the rubric rows; no package-scoped substitutes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes | `Verb` split to its own module at the cap; ports split along store/handler seams |
| LOGGING | yes | ported handlers keep the Zig event names; no new secret surfaces; refusals log codes, never key material |
| MILESTONE-ID | yes | none in source |
| UFS | yes | templates, registry codes and page ceilings as named constants; SQL literals shared by macro, not copied |
| SPEC TEMPLATE GATE | yes — this file | required sections filled |
| SCHEMA GUARD | no | no schema change — that is the family's rollback story |
| ERROR REGISTRY | yes | every refusal answers an existing `UZ-PROVIDER-*` registry code, referenced not invented |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_observability/src/export.rs` — the bounded, drop-counting export wrapper. §2's transport plugs into it rather than beside it; the wrapper's stated property is the property the transport must not break.
- **Reference:** `rustd/crates/afd_api/src/router/mount.rs` — the total match from route variant to handler, which is what makes an unported endpoint a compile error. §1's tooling choice is made to preserve it.
- **Reference:** `.github/workflows/deploy-dev*.yml` + `deploy/` — the existing staged deploy, verify and acceptance shape; the cutover reuses its verification pattern rather than inventing one.
- **Reference:** the M175–M180 rubrics — every per-surface oracle re-runs here as a pre-swap checklist; this milestone adds only whole-system proofs.

## Sections (implementation slices)

### §1 — Full-route parity gate

**`make test-parity LOCAL=1` is MANDATORY for this section.** It exists, it runs,
and it is red — this section is not done until it is green, and a green run is
the evidence, not a claim that the routes were added.

**Inherited from M181_001 — Dimension 2.3.** `test_runtime_on_production_base`:
the daemon serves from the distroless image, proven by the parity lane's
single-target mode against a container-hosted daemon. M181_001 built the lane
and ran it; the DISTROLESS half is already evidenced there — the image built for
`linux/arm64`, the container booted, `/healthz` answered, and the declared
`GET /metrics` divergence was honoured. What it could not do is pass the lane.

**Re-measured after M180 merged (2026-08-31): 31 unmounted routes fell to 13.**
M180 mounted 18 of them, which corrects the earlier reading that the missing
surface was M180's — that was true of 18 and not of the rest. The 13 that
remain, by family:

| Family | Routes |
|---|---|
| `tenants` (7) | `GET/POST /v1/tenants/me/models`, `PATCH/DELETE /v1/tenants/me/models/{id}`, `GET/PUT/DELETE /v1/tenants/me/provider` |
| `workspaces` (2) | `GET/POST /v1/workspaces/{workspace_id}/fleet-libraries` |
| `ingress` (1) | `POST /v1/ingress/{provider}` |
| `connectors` (1) | `GET /v1/connectors/{provider}/callback` |
| `webhooks` (1) | `POST /v1/webhooks/{fleet_id}/grant-approval` |
| `auth` (1) | `GET /v1/auth/sessions/{session_id}` |

Ten of those were never M180's; three are stragglers in families M180 otherwise
landed. Whatever mounts them, the acceptance test is the same command, and 2.3
is graded by re-running it rather than by inspecting a route table.

> Indy (2026-08-31): "2.3 fine defer"


The Rust daemon dumps its served route × method set from the route enum, emits an OpenAPI document generated from its own handlers, and a checker compares the two in both directions. The operations subcommands reach behaviour parity so tooling does not fork.

**Correction — the gate this work was written to extend is gone.** The served-versus-documented checker was deleted along with the whole OpenAPI checking family: the error checker, the URL-shape checker, the bundler, the split YAML sources, the make target and its Continuous Integration job. The reason was structural rather than incidental — the checker read the Zig daemon's route table as its source of truth for what is SERVED, and that daemon is being retired, with no Rust generator to repoint it at.

Two consequences. The committed OpenAPI document now has nothing generating or grading it, so the served-versus-documented direction is unguarded on both daemons. And this section BUILDS the gate rather than extending one.

**Implementation default — annotations over the existing handlers, and NOT the router-integrated variant.** The router-integrated idiom binds path and handler together at the registration site. This router does the opposite deliberately: the mount maps a route variant to a handler as a TOTAL match, and the router mounts from the enumerated route set with templates and scopes coming from route metadata. That totality is load-bearing — it is what makes an unported endpoint a compile error instead of a silent 404, and what the operator route inventory and scope tests key off. Plain annotations give the same generated document while keeping it.

**Sizing, measured while the split was authored:** 97 route variants across 11 enums, 46 mounted, 72 handler functions, 147 public wire types of which 115 carry a lifetime, 97 documented failure codes, against a current document of 70 paths and 45 schemas from 30 hand-written source files. The annotation pass is the bulk; reconciling hand-written prose against generated output is the part that is judgment rather than typing. The wire crate's manifest states it deliberately depends on nothing but its serializer — adding a derive macro there is a decision to take explicitly, not by default.

**Amendment — §1 is a PORT before it is an annotation pass.** The sizing above
counts 46 mounted routes against 97 variants and calls the annotation pass the
bulk. That is true of the routes that HAVE handlers. Nine route × method pairs
have none, and they are not annotations: they are three feature ports totalling
1,391 lines of Zig production code — the tenant provider triple, the model-entries
quad, and the workspace fleet-libraries pair. Dimension 1.1 grades whether the
route dump matches, which those ports must land for; it does not grade whether
they behave. Dimensions 1.5–1.7 below grade the behaviour, so a feature port
cannot pass this section by mounting a route that answers wrongly.

- **Dimension 1.1** — the route table declares the method set of every route, and it equals what the router mounts, with the difference empty in both directions → Test `test_declared_verbs_match_the_mounted_router` — **IN_PROGRESS.** `Route::verbs` is landed: all ten families declare their methods, total at both levels, and `Verb` moved to `route/verb.rs` because `route/mod.rs` was at its length cap. `TenantRoute::fleet_bundle_verbs` is subsumed by the general accessor.

  **Amendment — the Zig oracle is the parity lane, not the Zig source.** This dimension read "the route dump equals the Zig daemon's served set", which assumed the Zig table could be compared statically. It cannot: `src/agentsfleetd/http/router.zig:12` matches on method, but several routes accept any method there and 405 INSIDE the invoke function (`:33`, `:93` say so in their own comments), so the served set is `match` ∩ what each invoke allows and is not readable from the table. The Rust-versus-Zig half is therefore graded black-box, by the lane built for exactly that — `make test-parity BASE_URL=<zig> COMPARE_URL=<rust>` — and this dimension grades the half a fast lane can: that the declaration and the mount are one set.
- **Dimensions 1.2–1.4 — MOVED to M181_003** (the coverage gate, the generated artifact, `doctor` parity) with the utoipa design this spec's Discovery records. `backfill` parity is **deferred** outright, with the reasoning and the measurement in Discovery — it moved nowhere.
- **Dimension 1.5** — the tenant provider surface serves all three methods: the view composes the tenant's own selection with the live platform default and never 404s, the reset writes an explicit platform row from the live default, and the activation's ladder answers each of its five refusals with the registry code the Zig answers — every refusal a client can provoke decided from a value rather than raised as an error → Test `test_tenant_provider_ladder` — **IN_PROGRESS.** All three methods are served and mounted (`afd_api_tenant/src/handler/tenant/provider.rs`). The refusal matrix in front of the verbs and the ladder's FIRST rung are green at router tier: `cargo test -p afd_api --test tenant_plane` → 208 passed, 8 of them `tenant_provider_route.rs`. Rungs two through five answer from real rows, so they are graded by `integration_rotation/activation.rs` — written, `#[ignore]`d, and NOT YET RUN: the lane needs live Postgres and Redis.
- **Dimension 1.6** — the activation is one transaction that decrypts once, and never at all on the refusals a client provokes: a credential absent or not a provider key is decided from `vault.secrets` metadata on the already-locked row, and the catalogue gate and the selection write are ONE statement, so no model can be deleted between checking it and storing its ceiling → Test `test_activation_is_atomic_and_decrypts_once` — **IN_PROGRESS.** The shape is built: one transaction, one decrypt, the gate and the write as one `INSERT … SELECT … RETURNING`. Both properties are datastore-observable only, so the evidence is the six `#[ignore]`d cases in `integration_rotation/activation.rs`, which have compiled but not run.
- **Dimension 1.7** — the model-entries quad and the workspace fleet-libraries pair serve their route × method set, each refusal answering its registry code → Test `test_model_entries_and_libraries_ported` — **IN_PROGRESS.** BOTH are served and mounted (`afd_api_tenant/src/handler/{tenant/model_entry,workspace_library}.rs`), which closes §1's nine route × method gaps at the router. Everything in front of the verbs is green: `cargo test -p afd_api --all-features --test tenant_plane` → 232 passed, 13 of them `tenant_model_entry_route.rs` and 11 `workspace_fleet_libraries.rs`. What is NOT yet graded is every outcome decided from a row — a duplicate pair, an id that resolves to nothing, an entry that is the active selection, the merged gallery's order and its seek predicate, an onboarding's round trip. All are datastore-observable only and need the integration lane, which has still never run.

### §2 — MOVED to M181_004 (OTLP export)

All eight dimensions, the transport decision, the producer strategy and the
vendor-alias posture moved verbatim to
`docs/v2/pending/M181_004_P0_API_OBS_OTLP_EXPORT.md`. One tie remains: the
seven `agentsfleet_library_*` producers land there against handlers THIS spec
carries — the declared divergence in Discovery below stands until M181_004
closes it.

### §3 — MOVED to M181_006 (staging soak)

The soak, its budgets, the chaos probes and the bidirectional state handoff
moved verbatim to
`docs/v2/pending/M181_006_P0_API_INFRA_OBS_STAGING_SOAK_AND_SWAP.md` §1.

### §4 — MOVED to M181_005 and M181_006 (collectors; swap and rollback)

The collector-first step is
`docs/v2/pending/M181_005_P0_INFRA_OBS_COLLECTORS_UNDER_ZIG.md`; the rehearsal,
the refusal invariant, the runbook probes and the swap are M181_006 §2.

## Parallelization & execution map

(The split of 2026-09-01 made this map FAMILY-level; each sibling spec sequences its own interior.)

| Batch | Workstream | Runs | Why the edge is real |
|---|---|---|---|
| B6 | M181_002 (this spec) — close and PR | now | the 28 ready commits; only the two lanes remain |
| B6 | M181_004 — OTLP export | now, parallel | near-zero file overlap with this branch; one producer slice waits on the merge |
| B6 | M181_005 — collectors under Zig | now, parallel | deploy configuration only, serving the export that exists today |
| B7 | M181_003 — coverage gate | after this merges | every annotation sits on a handler this branch carries |
| B8 | M181_006 — soak and swap | after all siblings | every dimension needs the merged whole on staging |

The production swap stays operator-executed from the runbook — the agent prepares and rehearses; Indy pulls the trigger (M181_006).

## Interfaces

```
Route::all() × Route::verbs()     the served route × method set, declared beside the other
                                  route facts and graded against the mount — the family's
                                  served-side interface, read by M181_003's gate
the nine ported route × methods   /v1/tenants/me/provider (GET/PUT/DELETE) ·
                                  /v1/tenants/me/models (GET/POST) · /v1/tenants/me/models/{id}
                                  (PATCH/DELETE) · /v1/workspaces/{workspace_id}/fleet-libraries
                                  (GET/POST) — each refusal answering its UZ-* registry code
make test-parity LOCAL=1          RECORD mode against the committed contract — §1's mandatory gate
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| A verb declared but not mounted, or mounted but not declared | the route table and a plane's mount expression drifting | `test_declared_verbs_match_the_mounted_router` probes the un-layered mount and names the template and both sets |
| Orphaned credential reference | a `DELETE /workspaces/{ws}/secrets/{name}` committing between an activation's credential check and its write | impossible by lock order, not by retry: both paths take the `vault.secrets` row lock FIRST. Producer first, the delete observes the new entry and refuses; delete first, the activation finds no row and answers `UZ-PROVIDER-002` having written nothing. The treaty exists only because `secret_ref` is TEXT rather than a foreign key — `docs/architecture/tenant_provider_v2.md` §V2-1 deletes it |
| Ceiling stored for a model that is gone | an admin catalogue delete between an activation's gate check and its write | impossible by statement shape: the gate and the write are one `INSERT … SELECT … RETURNING`, so `rows_affected() == 0` IS the refusal. A separate `SELECT` first would be the race |
| Activation refused after a decrypt | a refusal path that opens an envelope to answer | the two credential rungs read `meta_provider`/`meta_has_key` on the locked row, so a plaintext key never enters the process on the way to a 400 — graded by Dimension 1.6 |

## Invariants

1. The route table's declared verb set equals the mounted set, graded against the UN-layered mount — `test_declared_verbs_match_the_mounted_router`; the layered router cannot answer this (the guard wraps the 405 fallback; measured, recorded in Discovery).
2. Every client-provokable refusal is a VALUE matched exhaustively at the handler — a new outcome variant cannot compile until the handler says what a client is told. Compiler-enforced, not review-enforced.
3. The activation decrypts at most once, and never on a refusal a client can provoke: the credential rungs decide from `meta_provider`/`meta_has_key` on the already-locked row, and the catalogue gate and selection write are ONE statement — `test_activation_is_atomic_and_decrypts_once`.
4. Every declared divergence is recorded in Discovery before the PR, and the parity oracles read the register — a declared divergence never surfaces as a regression and an undeclared one always does.

## Metrics & Observability

No product or operational signal changes in this workstream. The one declared gap — the seven `agentsfleet_library_*` families the Zig registry page emits and the Rust port does not — is recorded as a divergence in Discovery and stands until M181_004 constructs the meter and closes it against handlers this branch carries.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_declared_verbs_match_the_mounted_router` + `test_every_route_declares_a_verb` | every route's declared verb set equals what its un-layered mount serves, probed per verb across all 81 identities; no route declares an empty set |
| 1.5 | integration (negative) | `test_tenant_provider_ladder` | a body naming no `secret_ref` under `self_managed` → `UZ-PROVIDER-001` before any pool is touched; a name the vault does not hold → `UZ-PROVIDER-002`; a row whose metadata is not a provider key, and a body that will not read as a credential → `UZ-PROVIDER-003`; an uncatalogued or blank/padded model → `UZ-PROVIDER-004`; a refused endpoint → `UZ-PROVIDER-005`; no primary workspace → `UZ-PROVIDER-010`; a reset with no active default → `UZ-PROVIDER-009` |
| 1.6 | integration | `test_activation_is_atomic_and_decrypts_once` | a concurrent credential DELETE racing an activation leaves no selection row naming a deleted credential, in either arrival order; an uncatalogued model writes NEITHER a registry entry nor a selection row; a refused activation performs zero decrypts |
| 1.7 | integration (negative) | `test_model_entries_and_libraries_ported` | each route × method answers its Zig counterpart's status and registry code on the same seeded state |

Dimensions 1.2–1.4's rows moved to M181_003 with the dimensions themselves.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every contract route answers from the Rust daemon (§1) | `make test-parity LOCAL=1` | exit 0 | P0 | |
| R2 | Declared verbs equal the mounted set (§1) | `cd rustd && cargo test -p afd_api --all-features --test http_substrate route_verbs` | exit 0, `2 passed` | P0 | |
| R3 | Row-decided outcomes graded (§1) | `make test-integration-rustd` | exit 0 | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.lint`, `verify.unit`, `verify.version`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates (secret scan, oversize sweep), deliberately outside the declared set; R-rows name oracles this spec's own Files Changed create, so every command is copy-paste by merge time.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE. The production swap additionally requires Indy's explicit go in Discovery.

## Dead Code Sweep

N/A — no files deleted. The Zig daemon's retirement is a separate post-cutover milestone, and its binary remains the rollback for the whole of this one. M181_001 carried the family's only sweep.

## Out of Scope

- Deleting Zig source. Its lanes are already gone; the source and binary stay, because the binary IS the rollback.
- Any behaviour improvement on a live surface — see the parity rule below, which bounds what the port owes rather than freezing every superseded path into it.
- New dashboards, or canary infrastructure beyond the binary-selection knob.
- The four sibling workstreams' entire scope: the coverage gate and generated document (M181_003), OTLP export (M181_004), collectors (M181_005), soak and swap (M181_006).
- Public docs (`~/Projects/docs`): the ported endpoints are surface the Zig daemon already publishes; whether any published page names behaviour this port settles differently is graded at CHORE(close) against the published pages — landed or explained there, not waved off here.

**Single-implementation parity.** The Rust daemon implements exactly ONE implementation of each behaviour — the current one. Where the Zig daemon carries a superseded or compatibility path alongside it, the Rust port implements only the current path; the Zig copy is left in place and retires with that daemon. Live observable behaviour stays at parity: anything a client actually reaches today behaves identically, and the parity oracles compare the current path. "Superseded" is a claim requiring evidence recorded in Discovery — no in-tree emitter, plus Indy's sign-off on the specific path — never the implementing agent's judgment alone, and every instance is written into the declared-divergence register the cutover reads.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a tenant manages their provider, model registry and workspace libraries against the Rust daemon and cannot tell it from the Zig one: same statuses, same registry codes, same pages.
2. **Preserved user behaviour** — everything; that is the entire milestone.
3. **Optimal-way check** — an all-at-once swap with a rehearsed rollback beats a rolling mixed fleet: the invariants tolerate mixing, but a single boundary keeps triage unambiguous, and the canary path is named as the contingency in the runbook. Deploying the collectors under the incumbent binary first beats deploying them with the swap, because it turns one ambiguous change into two attributable ones.
4. **Rebuild-vs-iterate** — N/A at this milestone; it ships proof and process, not new architecture.
5. **What we build** — the three ports over their owning stores, the route table's verb set graded against the mount, the error-funnel convergence, and the two lanes' evidence.
6. **What we do NOT build** — anything a sibling owns (gate, export, collectors, soak, swap); Zig retirement; behaviour changes.
7. **Fit with existing features** — rides the existing deploy and verify workflow shape; must not destabilize the release path for the Zig binary, which remains the rollback.
8. **Surface order** — N/A — operational milestone; no new user surface.
9. **Dashboard restraint** — nothing new to show; continuity is the deliverable, and a new panel at cutover would be indistinguishable from a regression.
10. **Confused-user next step** — an operator mid-incident opens the runbook; every step has a probe and an abort criterion, and the divergence register tells them what genuinely differs between the binaries.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape (amended 2026-09-01):** this workstream narrowed to the route surface and its proof; the family's remaining slices became M181_003–006 so three worktrees can run in parallel — every boundary sits on a real dependency edge, recorded in the Parallelization map.
- **Alternatives considered:** running this as one milestone with the preparation work (rejected: the preparation half is blocked on nothing and carries both unknowns, so serializing it behind the ingress port idles the risky work); a rolling per-machine cutover as the plan (rejected: a mixed fleet doubles the drift surface for little gain; kept as the contingency); repointing the Zig integration corpus at the Rust daemon (rejected on three independent structural grounds recorded in §3 — it would report a pass rate for the implementation being retired).
- **Patch-vs-refactor verdict:** this is a **patch** to the operational layer — pipelines, lanes, runbook — plus one genuinely new surface, the generated OpenAPI document. The refactor was M176–M180; this milestone proves it and moves the traffic.

## Discovery (consult log)

**Split — 2026-09-01.** §§2–4 and the OpenAPI half of §1 moved to M181_003–006.

> Indy (2026-09-01): "You are working on Section 1, i wanna see what can be batched parallelized and break to smaller PRs?" … "Yes, 5 specs as drawn" … "I want these specs to go in this PR as we are nearly getting to completion?" — context: the four sibling specs ride this PR in `docs/v2/pending/`; M181_004 and M181_005 are runnable in parallel on merge, M181_003 follows this branch, M181_006 closes the family.


**Declared divergence — the registry page emits no metric families yet.** The
Zig list handler opens a read window and a stage scope, and between them they
emit seven declared families for `surface=tenant_models`
(`agentsfleet_library_stage_duration_seconds_total` and its observation
denominator, `_read_outcome_total`, `_pool_result_total`, `_payload_bytes_total`,
`_results_total`, `_cache_outcome_total`). The Rust port emits none of them, and
the port did NOT hand-roll a `ReadScope` equivalent to do so.

Both halves of that are deliberate. `afd_observability` already carries the
typed, census-backed registry those families are declared in
(`docs/metrics.census.tsv` lines 67-73) over `opentelemetry_sdk`, and what it
does not carry is a Meter — because §2 is the section that builds the OTLP
transport, and the Rust daemon exports nothing at all until it lands. Emitting
into a meter that does not exist is dead code (RULE NDC), and re-implementing
the Zig's stage timers by hand to feed it later is the transliteration RULE PORT
forbids.

So the gap is named here rather than closed twice: §2 constructs the meter, and
the surface label these families are keyed by is what the registry page will
report under. Until then the page's observability is `tracing` alone. This is a
divergence for the register, not a defect — but it IS a divergence, and the
parity differ must read it as declared.

**Decision — OpenAPI generation is utoipa 5.5.0, feature-gated out of production.**

> Indy (2026-09-01): "I am leaning towards Option 2 — full generation" — context:
> the §1 fork between emitting paths alone and emitting the whole document
> including `components.schemas`.

> Indy (2026-09-01): "Yes go lets follow the practicse by the crates" — context:
> taking the optional-dependency + `cfg_attr` pattern below rather than an
> unconditional dependency in `afd_wire`.

The manifest objection §1 records — `afd_wire` carries serde and serde_json
deliberately — is answered by the pattern comparable crates already use.
Measured on crates.io: `json-patch` (6.1M downloads), `compact_str` (2.1M) and
`fastnum` (541K) each declare `utoipa` as `optional = true`; `mistralrs-mcp`
(`Cargo.toml:28,33`) and the generated `google-apis-rs` crates
(`gen/pubsub1/Cargo.toml:30,39`) do the same and gate every derive behind
`#[cfg_attr(feature = "…", derive(utoipa::ToSchema))]`. The invariant that
matters is that the DEFAULT wire dependency graph and serialization behaviour
carry only serde and serde_json. A disabled schema compiler does not weaken it,
and `ToSchema` cannot move a byte because it does not touch `Serialize`.

**External review — Tarzy (ChatGPT CTO), 2026-09-01.** Verdict: adopt, with two
corrections, both taken.

1. **Lifetimes are not a wall.** The reading that reached this spec — that no
   production reference derives `ToSchema` on a lifetime-carrying type — was
   measured (google-pubsub1: 114 public structs, 47 with lifetimes, 66
   `ToSchema` derives, zero overlap; mistral.rs: zero) and wrongly generalised.
   Their DTO style is owned; utoipa does not require it. utoipa 5's own
   `ToSchema` documentation derives on
   `struct Person<'p, T, P> { name: Option<Cow<'p, str>> }`, and utoipa 5
   removed the trait's lifetime parameter outright. The 139 lifetime-carrying
   types in `afd_wire` need **no** `value_type` override merely for being
   borrowed; `value_type` is for types whose SERIALIZED form differs from their
   Rust form, which is what `json-patch` uses it for. One papercut stands and is
   spiked first: a standalone top-level `Cow<'a, str>` as a reusable component
   is suspect, because the impl is `impl<'a, T: ToSchema + Clone> ToSchema for
   Cow<'a, T>` with `T: Sized` implied. As a FIELD it is fine.
   **Converting the wire types to owned is rejected** — it manufactures a second
   contract that can drift from the borrowed one.
2. **Do not measure the production cost, remove it.** `openapi` is a non-default
   feature; the shipped daemon builds without it and carries zero generated
   code, zero schema strings and zero OpenAPI allocation. CI pays the compile
   cost for 176 derives and 72 path macros, which is where it belongs. This
   replaces the binary-size and compile-time budget rows this section would
   otherwise have needed.

Confirmed by the review and adopted: **`utoipa-axum`'s `OpenApiRouter` is
rejected**, for the reason §1 already gives — the total match from route variant
to handler is a stronger invariant than registration-site binding. Lemmy's
report (juhaku/utoipa#662) is what drove utoipa toward `OpenApiRouter`; it is a
cure this router does not need. Production users of the stack as adopted:
crates.io itself (axum 0.8.9 + utoipa 5.5.0), Kanidm, Restate, Lakekeeper.

**Consequence — the collector is one per plane crate, not one per daemon.** The
review's prescription is a single gated `#[derive(OpenApi)]` inside the daemon
library exposing one public `document()`, because `__path_*` items cannot be
resolved across a crate boundary when the handler's module is private. This
daemon's handlers are `pub(crate)` across FIVE plane crates (`afd_api_tenant`,
`afd_api_runner`, `afd_api_operator`, `afd_api_ingress`, `afd_api`), so the
shape is one gated collector per plane, each exposing `document()`, merged at
the composition root. Making 72 handlers and their modules public to serve a
build-time tool is rejected.

**Amendment — Dimension 1.2's gate is a Rust test, not a script pair.** The
served set (`Route::all()` × `Route::verbs()`) and the documented set
(`ApiDoc::openapi().paths`) are typed values in one process, so set equality in
both directions is a `#[test]` — which is what Dimension 1.2 always named it.
`agentsfleetd routes --json` existed only to carry the served set across a
process boundary and is dropped; `scripts/check_route_coverage.py` and
`scripts/check_route_coverage_test.py` are dropped from Files Changed with it.
The comparison is NOT redundant: `#[utoipa::path(path = "…")]` restates the path
string, so the route table and the annotations are two declarations of route
identity and utoipa cannot prove they agree.

**Amendment — `agentsfleetd openapi` is a CI build, not a production
subcommand.** Following from the feature gate: the shipped binary does not carry
the emitter, so an operator cannot ask a running daemon what it serves.
Dimension 1.3's `emitted == committed` diff runs in CI against a
`--features openapi` build.

**Deferral — `backfill` parity (Dimension 1.4).** `doctor` stays in scope.

> Indy (2026-09-01): "so defer backfill command its not needed." — context: §1
> Dimension 1.4's `doctor` and `backfill` parity.

The argument that made `backfill` look load-bearing is withdrawn, and the
measurement is why.

> Indy (2026-09-01): "ignore legacy, this is crap and not this regression
> assumption." — context: the claim that unprojected `vault.secrets` rows make
> the Rust activation ladder refuse valid provider keys.

Measured against planetscale-dev, 167 rows in `vault.secrets`: `meta_kind` NULL
= 0, `meta_has_key` NULL = 0, fully unprojected = 0. The distribution is 135
`custom_secret` (no provider, no key — correctly `NotAProviderKey`), 30
`custom_endpoint` and 2 `provider_key` (both correctly `ProviderKey`). The
`(None, None)` state is not merely absent but unconstructible: `afd_vault`'s
`insert_row!()` macro (`src/sql.rs:31-41`) expands to a literal shared by both
write arms so create and replace cannot disagree about the column set, and all
four `meta_*` columns are positional parameters in it; `UPDATE_SECRET`
(`:76-89`) rewrites them alongside the envelope. The dashboard
(`@/lib/api/secrets` `listSecrets`/`replaceSecret`) and the command line
(`cli/src/lib/api-paths.ts:103,106`) both reach `vault.secrets` through exactly
those two statements. Adding an `Unknown` rung to `SecretKind::of` would be a
branch nothing can reach (RULE NDC), and it is not added.

**Finding — the verb declaration must be graded against the UN-LAYERED mount.**
`Route::verbs()` is a declaration; the mount is a separate expression; nothing
in the type system binds them, because `axum`'s `MethodRouter` is opaque once
built. `test_declared_verbs_match_the_mounted_router` therefore probes. Probing
the SERVED router does not work, and this was measured rather than assumed:
`MethodRouter::layer` wraps the 405 fallback as well as the handlers, so on a
guarded route an unserved method is refused 401 by the guard before it can reach
that fallback — every `Guard::Bearer` template reported all five verbs as
mounted. The probe runs against `mount::handler_for`'s bare `MethodRouter` with
state applied and no layers.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
