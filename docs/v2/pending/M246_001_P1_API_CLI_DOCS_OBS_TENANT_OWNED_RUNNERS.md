<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M246_001: A tenant enrols its own runner, which takes only that tenant's work and never a platform model key

**Prototype:** v2.0.0

**Milestone:** M246

**Workstream:** 001

**Date:** Nov 06, 2026 (rescoped Nov 17, 2026)

**Status:** PENDING

**Priority:** P1, customers will point their own hosts at the cloud `agentsfleetd`. Today only an operator can enrol a host, and any enrolled host receives every tenant's work.

**Categories:** API, CLI, DOCS, OBS

**Batch:** B1, milestone 6 of the push-scheduler programme; ships as its own Pull Request because it moves a security boundary (SPEC-BOUNDARY)

**Depends on:** M243_001 (done): its runner stream authenticates once per connection, re-checks on every heartbeat and closes on revoke. M251_001 carries the work this rescope moved out, including a public route to the stream listener.

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the Elephant-Goldfish Model (EGM) brief and source research on main 14539a73e. Rescoped Nov 17, 2026 against main 97224ed74 at Indy's request, then revised after a four-reviewer EGM review.

**Canonical architecture:** `docs/AUTH.md` §"Runner token (`agt_r`): the machine principal" and `docs/architecture/runner_fleet.md` §"Registering a runner" own the trust model this changes. `docs/architecture/runner_protocol.md` owns the stream handshake this reuses unchanged.

---

## Overview

**Goal (testable):** A tenant-enrolled runner leases only its tenant's fleet and setup work, and never receives a platform model key.

**Problem:** Indy wants people to "spin a runner and point to the agentsfleetd (in cloud) and keep using it". Today a tenant cannot enrol that host, and the host would be unsafe if it were enrolled:
- Only platform operators can enrol. `POST /v1/admin/runners` requires `runner:enroll` (`afd_http/src/route/runner_ops.rs`), and `TENANT_OWNER_GRANT` lacks it (`afd_auth/src/scope.rs`).
- Nothing keeps another tenant's work off a host. Placement checks only `required_tags <@ labels` (`select_ledger_candidates.sql`), so an untagged fleet matches any runner. The claim and issue re-check neither labels nor owner. Authoring's `candidates.sql` takes no runner at all.
- Platform keys reach runners two ways. A fleet on the platform default model carries the key in its lease (`ExecutionPolicy.api_key`, `afd_gate/src/policy/build/mod.rs`). A runner assigned a platform decision model receives the platform decision key (`afd_fleet/src/task/decision/dispatch.rs`). A tenant holds root on its own host and can read both.

**Solution summary:** `runner.runners` gains a nullable `tenant_id`; NULL is a platform runner. Tenant owners enrol and manage their own runners under a new `runner:tenant` scope at `/v1/tenants/me/runners`, through the handlers the admin routes use. One fixed rule decides placement: a tenant runner takes only its tenant's work, and a platform runner takes any tenant's work. The claim statement applies the rule, and issue re-checks it under the host lock. A tenant runner never takes work that would carry a platform key, and a database check keeps a platform decision model off it.

**Design flow:**

```
owner ─ `agentsfleet runners enroll` ─► POST /v1/tenants/me/runners
   (runner:tenant, Idempotency-Key) ─► receipt + runners row (tenant_id = caller's) ─► agt_r once
host: AGENTSFLEET_RUNNER_TOKEN + stream URL + CA file ─► agentsfleet-runner ─► M243 stream (unchanged)
admission for fleet F of tenant T, offered to runner R:
   scan + claim ─ R.tenant NULL, or R.tenant = T and the revision that runs is not platform-default
   issue / authoring admit ─ lock_host(R, T) returns `foreign` under FOR UPDATE ─► refuse, log
   issue ─ R tenant-owned and Billed.posture = platform ─► refuse; the slot's fence releases it
revoke: PATCH {"action":"revoke"} ─► status gate ─► M243 stream closes
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): tenant-owned runners behind an ownership filter
- **Intent (one sentence):** let a customer run fleets on their own machine without that machine ever seeing another customer's work or a platform model key.
- **Authoring handshake:** Indy asked to prioritise "tenant runs a runner" and to move the audit and the rest to a spec after M250. He chose to fold the scope-catalogue rewrite into this Pull Request. ASSUMPTIONS I'M MAKING:
  1. The tenant routes (§2) and the ownership filter (§3) stay. Without them no tenant can create a runner, and a tenant runner would receive every tenant's work.
  2. A tenant runner reaches the stream through a route its operator provides. The public listener route and its certificate authority (CA) choice are M251_001 §1.
  3. Existing accounts receive `runner:tenant` by an identity-provider edit (`docs/AUTH.md` §"Provisioning grants"). Pre-launch, there is no backfill job.
  4. Runner and daemon ship together. No runner wire shape changes in this spec.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE). CHORE(open) re-verifies Files Changed against `origin/main` and takes slot and error-code numbers fresh (SPEC-NUM).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` and `select_ledger_candidates.sql`: the claim and the scan the ownership predicate enters. The claim is the control; the scan only filters.
2. `rustd/crates/daemon/afd_runner/src/capacity/mod.rs` and `capacity/sql/lock_host.sql`: the host lock that `Leases::issue` and the authoring `admit` take.
3. `rustd/crates/daemon/afd_fleet/src/lease/issue.rs` and `afd_fleet/src/task/decision/`: where a lease's posture and a decision key reach a runner.
4. `rustd/crates/daemon/afd_api_operator/src/handler/operator/` and `afd_api_runner/src/handler/runner/enrolment.rs`: the admin handlers the tenant routes share.
5. `cli/src/program/tree/platform-runners.command.ts` and `cli/src/program/entry/discovery.ts`: the operator command group, and how the CLI learns a session's scopes.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M246_001_P1_API_CLI_DOCS_OBS_TENANT_OWNED_RUNNERS.md` | EDIT, then MOVE to `active/` and `done/` | this spec |
| `docs/v2/pending/M251_001_P1_API_CLI_DOCS_INFRA_OBS_UI_TENANT_RUNNERS_CONNECT_ROTATE_PROXY.md` | CREATE | the Sections this rescope moved out |
| `schema/20261108123100_runner_runners.sql` | EDIT | §1: `tenant_id`, its index, the trigger that fixes it, and the tenant-or-decision-model check. No production deploy exists, so the table file is edited (`.github/workflows/` holds only `deploy-dev-*`) |
| `schema/<UTC>_runner_enrolment_receipts.sql` | CREATE | §1: the enrolment retry receipt (SQL-RETRYKEY) |
| `rustd/crates/daemon/afd_db/src/migration.rs`, `afd_db/tests/migrations.rs` | EDIT | §1: register the receipt slot (MIG) |
| `rustd/crates/daemon/afd_fleet/tests/integration_runner_row.rs` | EDIT | §1: the tenant, check and erasure tests |
| `rustd/crates/daemon/afd_auth/src/scope.rs`, `afd_auth/tests/scope_catalogue.rs` | EDIT | §2: the `scopes!` catalogue, `ScopeSet` as `u64`, `Scope::RunnerTenant`, `TENANT_OWNER_GRANT` of 11 |
| `rustd/crates/foundation/af_core/src/error_code/fleet.rs`, `error_code/mod.rs`, `problem/fleet.rs` | EDIT | §2, §3: `UZ-RUN-025`, `UZ-RUN-026`, `UZ-RUN-027` |
| `rustd/crates/daemon/afd_runner/src/reach.rs`, `lib.rs` | CREATE, EDIT | §2: `Reach` and `Reach::admit` |
| `rustd/crates/daemon/afd_runner/src/store.rs`, `admin.rs`, `edit.rs`, `rotate.rs`, `view/store.rs`, `view/decode.rs` | EDIT | §2: each management call takes `&Reach`; views carry `tenant_id` |
| `rustd/crates/daemon/afd_runner/src/sql/runner/insert_runner_with_event.sql`, `select_enrolment_receipt.sql` (new), `select_tenant_runner.sql` (new), `sql/runner/mod.rs` | CREATE, EDIT | §2: enrolment with its receipt, the replay read, the ownership proof |
| `rustd/crates/daemon/afd_runner/src/sql/runner_view/list_runners_first.sql`, `list_runners_after.sql`, `runner_detail.sql`, `list_tenant_runners_first.sql` (new), `list_tenant_runners_after.sql` (new), `count_tenant_runners.sql` (new), `sql/runner_view/mod.rs` | CREATE, EDIT | §2: the tenant list and its own total |
| `rustd/crates/daemon/afd_runner/src/capacity/mod.rs`, `capacity/choice.rs`, `capacity/sql/lock_host.sql` | EDIT | §3: the host lock binds the work's tenant and returns `foreign` |
| `rustd/crates/daemon/afd_http/src/route/runner_ops.rs`, `afd_http/src/handler/reach.rs` (new), `afd_http/src/handler/mod.rs` | CREATE, EDIT | §2: tenant route identities and the tenant lookup that builds a `Reach` |
| `rustd/crates/daemon/afd_api_operator/src/handler/operator/runners.rs`, `runner_patch.rs`, `runner_delete.rs`, `runner_enrol.rs` (moved from `afd_api_runner`), `mod.rs`, `afd_api_operator/src/lib.rs`, `openapi.rs` | CREATE, EDIT | §2: one handler set for both reaches |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/enrolment.rs`, `afd_api_runner/src/handler/runner/mod.rs`, `lib.rs`, `openapi.rs` | DELETE, EDIT | §2: enrolment moves to its sibling handlers |
| `rustd/crates/daemon/afd_api/src/router/mount.rs` | EDIT | §2: drop the enrolment special case |
| `rustd/crates/daemon/afd_api_wire/src/operator.rs`, `rustd/crates/protocol/af_wire/src/runner/mod.rs` | EDIT | §2: `tenant_id` on views; the token-free replay answer |
| `rustd/crates/daemon/afd_api/tests/route_inventory.rs`, `route_meta_total.rs`, `route_meta_scopes.rs`, `integration_tenant_runners.rs` (new) | CREATE, EDIT | §2: inventory, counts and route tests |
| Every caller of `Runners::register`, `list_runners`, `runner_detail`, `transition`, `edit`, `rotate_token`, `request_selftest` and `delete_revoked`: the `afd_fleet`, `afd_runner`, `afd_gateway` and `main` test fixtures and `afd_bench/src/lane/lease/seed/mod.rs` | EDIT | §2: pass `&Reach::Platform` (NLG) |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql`, `claim_admission_head.sql`, `lease/assign/ledger.rs`, `lease/issue.rs`, `lease/store.rs`, `lease/renew/extend.rs` | EDIT | §3, §4: the predicate, its binds, the issue re-checks |
| Every test and bench that binds the claim or scan statement positionally: `afd_fleet/tests/integration_lease_plan/`, `integration_ledger_queue/deadlines.rs`, `integration_claim_delivery.rs`, `integration_tasks/decisions/configured/support.rs`, `afd_bench/src/statements/tests.rs`, `afd_bench/src/lane/cardinality/probe.rs` | EDIT | §3: the new binds |
| `rustd/crates/daemon/afd_authoring/src/lease/claim/sql/candidates.sql`, `claim/mod.rs`, `claim/selection.rs`, `afd_authoring/src/draft/mod.rs`, `afd_authoring/tests/draft/admission/discovery.rs` | EDIT | §3: the same filter for setup work |
| `rustd/crates/daemon/afd_fleet/tests/integration_runner_tenant.rs`, `integration_runner_tenant/` | CREATE | §3, §4: matrix, claim, issue, key |
| `rustd/crates/daemon/afd_authoring/tests/draft/admission/tenant.rs` | CREATE | §3: setup work placement |
| `rustd/crates/daemon/main/tests/integration_rust_runner_tenant.rs`, `main/tests/daemon_suite.rs` | CREATE, EDIT | §4: the end-to-end run |
| `cli/src/commands/platform/runner-reach.ts` | CREATE | §5: `RunnerReach` and its two values |
| `cli/src/commands/platform/runners.ts`, `common.ts`, `cli/src/program/tree/platform-runners.command.ts`, `platform.command.ts`, `index.ts`, `platform-flags.ts`, `cli/src/program/entry/discovery.ts` | EDIT | §5: one command factory; the tenant group and its scope discovery |
| `cli/test/runners.unit.test.ts` (new), `cli/test/platform-runners.unit.test.ts`, `cli/test/scoped-discovery.unit.test.ts` | CREATE, EDIT | §5 |
| `docs/AUTH.md`, `docs/architecture/runner_fleet.md`, `postgres_schemas.md`, `runner_protocol.md` | EDIT | §6 |
| `public/openapi.json` | EDIT (regenerated with `--no-banner`) | §6 |
| `~/Projects/docs`, branch `chore/m246-tenant-runners-changelog` | EDIT | §6: changelog and a "Run your own runner" page; API pages after merge |
| `.orly/**`, `.claude/skills/orly-*/`, `.claude/agents/orly-standards-reviewer.md`, `.agents/skills/orly-*/`, `.opencode/skills/orly-*/` | EDIT, DELETE | the managed rules re-materialised by `orly update --no-hooks` at orly 0.50.0, at Indy's request; no `orly_version` pin |

## Applicable Rules

- **`domains/auth.md`** — AUTH-DOC, AUTH-GUARD, AWO, AUTH-SECRET, AUTH-NOLOG, CTM, CTC
- **`domains/sql.md`** — SCH, MIG, SQL-SLOT, STS, NSQ, SQL-READER, SQL-ERASE, SQL-ID, SQL-FKNAME, SQL-RETRYKEY, SQL-FILE, SQL-PREPARE, SGR
- **`domains/http.md`** — HTTP-NOUN, HTTP-NEST, HTTP-VERB, HTTP-PARAM, HTTP-PATCH, HTTP-IDEMKEY, HTTP-GARDE, HTTP-VALID, HTTP-ROUTE
- **`domains/http-responses.md`** — HTTP-STATEEXT, HTTP-NEWCODE, HTTP-LEAK, HTTP-PAGE; **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-SURFACE
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-NEWTYPE; **`languages/typescript.md`** — TS-SCHEMA, TS-ERRSTYLE, TS-BUNTEST
- **`code.md`** — CODE-SEAM, FLL, NLG, CODE-ERRREG; **`domains/logging.md`** — LOG-SECRET, LOG-EVENT, LOG-CODE
- **`tests.md`** — TEST-NEG, TEST-REAL, TEST-ABSENT, TEST-RACE, TEST-CONTEND, TEST-LANE; **`docs.md`** — DOC-02, DOC-03, DOC-11; **`domains/architecture.md`** — ARCH-LAND, ARCH-PUBLISHED

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `gitleaks` at commit and `orly gate pr` | yes, the key-sentinel tests | sentinels are drawn from `af_crypto::entropy` at test time; no key-shaped literal is committed |
| `make lint-all` (clippy `-D warnings`, `cargo shear`, TypeScript lint) | yes, ten crates and the CLI | `lint-rustd` and the CLI lint per Section |
| Route inventory, `route_meta_total` and `route_meta_scopes` | yes, two templates, five operations | inventory, counts and scopes move in §2's commit |
| Error registry and problem parity | yes, three codes | const, registry row and problem entry in one commit |
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, `AUTH.md` and three architecture pages | sentences under 25 words, paragraphs of at most three |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, the operator handlers and the claim binds | enrolment gets its own file; `reach.rs` stays separate |

## Prior-Art / Reference Implementations

- **Reference:** GitLab's runner scopes (https://docs.gitlab.com/ci/runners/runners_scope/). Project runners serve only their project, and instance runners serve everyone: the one-way widening here.
- **Reference:** `afd_api_runner/src/handler/runner/enrolment.rs` and `afd_runner::Runners::register`: the mint, the digest-only row and the reveal-once response. Divergence: a tenant, and an `Idempotency-Key` receipt.
- **Reference:** `schema/20261108124300_authoring_replay_receipts.sql`: a receipt keyed by an operation identifier, holding a request digest, with no foreign keys and an append-only trigger. The enrolment receipt mirrors it.
- **Reference:** `drafts.tenant_model_id` (`docs/architecture/postgres_schemas.md` §"Model pointers"): a nullable tenant pointer where NULL means the platform.
- **Reference:** `cli/src/commands/platform/runners.ts`. The tenant group follows the six CLI practices this code already keeps: command, handler and error layers apart; handlers that neither print nor exit; one rendering layer; structured JSON errors with suggestion and retry fields; handler, in-process and subprocess tests; JSON by default when stdout is not a terminal.

## Sections (implementation slices)

### §1 — The runner row names its tenant

`runner.runners` gains `tenant_id`, referencing `identity.tenants(id) ON DELETE CASCADE`, with a partial index where it is set. NULL is a platform runner. `tenant_id` names the tenant everywhere else in the schema (SQL-FKNAME). A trigger refuses any change to `tenant_id` after insert. A check refuses a tenant runner with a decision model: `tenant_id IS NULL OR decision_platform_model_id IS NULL`.

`runner.enrolment_receipts` mirrors `authoring.replay_receipts`: a UUIDv7 `id`, `tenant_id`, `operation_id`, `request_digest`, `runner_id` and `created_at`. It is unique on `(tenant_id, operation_id)`, has no foreign keys and refuses updates. `api_runtime` gets `SELECT` and `INSERT` (SGR). `operation_id` is the request's `Idempotency-Key`. `request_digest` is the SHA-256 of the request re-serialised from its parsed form, so whitespace never changes it. The receipt outlives its runner, so a retried key never enrols twice.

The deletion matrix gains the tenant's runners under Tenant, with their leases, history and counters. Receipts stay, holding opaque identifiers only, as other receipts do. **Implementation default:** the tenant lives on the row and rides the host lock, never on `Principal::Runner`. The lock read is the authority at placement, and a principal copy would be a staler second one. **Implementation default:** a trigger keeps the tenant fixed. A tenant runner flipped to NULL would become a platform runner and receive every tenant's work.

- **Dimension 1.1** — the tenant cannot change → Test `runner_tenant_cannot_change` (integration): an `UPDATE` setting a tenant runner's `tenant_id` to NULL raises, and the row still names its tenant. The same `UPDATE` on a platform runner's `host_id` succeeds, so a trigger that refuses every update fails.
- **Dimension 1.2** — a tenant runner holds no decision model → Test `tenant_runner_refuses_a_decision_model` (integration): setting `decision_platform_model_id` on a tenant runner violates the check; on a platform runner it succeeds.
- **Dimension 1.3** — erasure takes only the tenant's runners → Test `tenant_erasure_takes_its_runners` (integration): deleting tenant A removes A's two runners and their leases. A platform runner, tenant B's runner and A's receipts remain.

### §2 — Tenants enrol and manage their own runners

**The scope catalogue is one list.** A `scopes!` `macro_rules!` takes each variant, its doc comment and its claim string once. It generates the enum, `Scope::ALL` and the `const fn wire()`, so adding a scope is one line. `ScopeSet` becomes a `u64`, and a compile-time assertion bounds `Scope::ALL.len()` by 64. `KNOWN_BITS` goes, because a list generated from one input cannot miss a variant. `HIERARCHY` stays hand-written. Every claim string stays byte-identical, because the identity provider matches them verbatim. **Implementation default:** `macro_rules!`, not `strum`. `strum`'s conversions are not `const`, and a procedural-macro crate is a build unit every consumer waits on (`af_core/src/error_shell.rs`).

`Scope::RunnerTenant` (`runner:tenant`) joins the catalogue and `TENANT_OWNER_GRANT`. It covers enrolment, reading and changes together. Platform enrolment is its own scope because that host receives every tenant's work (`runner_ops.rs`); a tenant runner receives only its own tenant's.

**One handler set, two reaches.** `afd_runner::Reach` is `Platform` or `Tenant(tenant_id)`. It says how far a caller reaches into runners. Every management call takes `&Reach`: `register`, `list_runners`, `runner_detail`, `transition`, `edit`, `rotate_token`, `request_selftest` and `delete_revoked`.
- `Reach::admit` runs first in each call on one runner. `Platform` admits every runner with no query. `Tenant` proves ownership with `select_tenant_runner.sql`, a primary-key read, and answers `404 UZ-RUN-014` otherwise (AWO).
- `list_runners` picks its statements by variant. `Tenant` reads `list_tenant_runners_*.sql` and `count_tenant_runners.sql` with the tenant as a required bind, so `total` counts only that tenant's runners.
- On a tenant runner, `Platform` may read, cordon, drain, revoke, delete and request a self-test. It may not edit policy or labels, or rotate the token: those answer `409 UZ-RUN-026`. Rotation would hand an operator a working token for the tenant's host.
- `register` and `edit` refuse a `decision_model_id` for any tenant runner, from either reach, with `400 UZ-REQ-001`.

Enrolment moves from `afd_api_runner` into `afd_api_operator`, beside the list, detail, patch and delete handlers. Each admin handler passes `Reach::Platform`. Each tenant handler first calls `afd_http::handler::reach::tenant`, which resolves the caller's tenant through the shared `tenant_of` lookup, then calls the same core function. The tenant route identities join `RunnerOpsRoute`, so one route family maps to one handler crate. Events and leases stay admin-only.

**Enrolment and its retry.** A tenant enrolment requires a UUIDv7 `Idempotency-Key`, read by `afd_http::handler::setup_input::operation`. One statement inserts the receipt with `ON CONFLICT DO NOTHING`, then the runner and its event from the receipt's `RETURNING`. When no receipt was inserted, a second statement reads the stored receipt. A matching digest answers `200` with the original `runner_id` and `assigned_policy` and no token. A different digest answers `409 UZ-RUN-025`. A concurrent duplicate waits on the unique index, inserts nothing, and then reads the committed receipt.

**Implementation default:** composition by value, not a trait with platform and tenant implementations. The two reaches differ in one value and one statement choice, not in behaviour. **Implementation default:** the replay answers without the token. AUTH-SECRET forbids keeping the token, so HTTP-IDEMKEY's replay returns everything else. **Implementation default:** separate tenant routes, not a tenant filter on `/v1/admin/runners`, because a nullable filter turns a missing tenant into a full listing.

- **Dimension 2.1** — an owner enrols a host → Test `tenant_enrols_a_tenant_runner` (integration): a session holding `runner:tenant` posts a valid body → `201`, an `agt_r` once, `Cache-Control: no-store`, and the row's `tenant_id` is the caller's tenant. A platform enrolment's row has a NULL `tenant_id`, so a test that never sets it fails.
- **Dimension 2.2** — enrolment needs the scope → Test `enrolment_without_runner_tenant_is_refused` (integration): an owner grant without `runner:tenant` → `403 UZ-AUTH-022` and no row; an `agt_r` bearer is refused `401` before any datastore read.
- **Dimension 2.3** — a retry enrols nothing twice → Test `enrolment_retry_replays_without_the_token` (integration): the same key and body twice → the second answers `200` with the same `runner_id` and no `runner_token`, one row. The same key with other labels → `409 UZ-RUN-025`, still one row. No key, or a key that is not a UUIDv7 → `400 UZ-REQ-001`. A body naming `decision_model_id` → `400 UZ-REQ-001`, no row.
- **Dimension 2.4** — concurrent retries enrol once → Test `concurrent_enrolments_with_one_key_leave_one_runner` (integration): eight tasks post the same key and body behind a barrier → exactly one `201`, seven `200` with the same `runner_id`, and one row.
- **Dimension 2.5** — a tenant manages only its own → Test `tenant_lists_and_manages_only_its_runners` (integration): A owns two, B one, the platform one. A's list returns exactly A's two ids with `total: 2`, and `?limit=1` pages them. A's GET, cordon, rotate and DELETE on its own runner succeed. The same calls on B's runner each answer `404 UZ-RUN-014`, and B's row is unchanged.
- **Dimension 2.6** — an operator cannot take over a tenant's host → Test `platform_reach_cannot_rotate_or_edit_a_tenant_runner` (integration): an operator's rotate, labels edit and decision-model edit on A's runner each answer `409 UZ-RUN-026` or `400`, and the row and token digest are unchanged. The operator's cordon on the same runner succeeds.
- **Dimension 2.7** — a tenant revoke ends the host's stream → Test `tenant_revoke_closes_its_runner_stream` (integration): A revokes its runner → the M243 stream closes, and a reconnect answers `401 UZ-RUN-009`.
- **Dimension 2.8** — operators see who owns a host → Test `operator_list_names_each_tenant` (integration): the platform row reads `tenant_id: null`; the tenant row reads its tenant.
- **Dimension 2.9** — every claim string survives the catalogue rewrite → Test `scope_claim_strings_are_unchanged` (unit, `afd_auth/tests/scope_catalogue.rs`): the 29 existing scopes' `wire()` values equal a pinned list byte for byte, `runner:tenant` is the 30th, `from_wire` round-trips every entry, and every `HIERARCHY` scope appears in `Scope::ALL`.

### §3 — The ownership filter in the claim, at issue and for setup work

One fixed rule: a tenant runner takes only its tenant's work, and a platform runner takes any tenant's work. The authority is `runner.runners.tenant_id`, read only from Postgres, so every replica decides alike. Three places apply it:
- The candidate scan filters on the runner's tenant and the fleet's tenant. It is a performance filter only.
- The claim statement's `fleet` step joins the runner, and keeps the fleet only when the runner's tenant is NULL or matches. The slot upsert already requires that row. Called with a foreign fleet, the claim returns no row and leaves the slot and its fencing token untouched. This is the control.
- `lock_host.sql` binds the work's tenant and returns `foreign` under `FOR UPDATE`. `Capacity` and `Held` carry the boolean and stay `Copy`. `Leases::issue` and the authoring `admit` refuse a foreign host before any write. They log `runner_placement_foreign_refused` at `error` with `error_code` `UZ-RUN-027` (LOG-CODE). Any occurrence means the claim has a hole. Renewal binds no tenant and reads `foreign` as false.

`candidates.sql` gains a runner bind and the same predicate over `authoring.drafts.tenant_id`. A refused authoring `admit` answers `Placement::Skipped`, so the walk moves to the next draft. **Implementation default:** no preference for a tenant's own runners over platform runners, because labels and stickiness already order the scan. A tenant's choice of placement is M251_001.

- **Dimension 3.1** — the live matrix matches the rule → Test `cross_tenant_placement_matrix` (integration): tenant T's fleet has one admission, and three runners are online: T's own, a platform runner and tenant U's. Each is driven through the scan, the claim and issue. T's and the platform runner lease; U's yields no candidate, no slot write and no lease row. A filter that refuses everything fails the test.
- **Dimension 3.2** — the claim alone refuses → Test `claim_statement_refuses_a_foreign_runner` (integration): `claim_admission_head.sql` bound directly with T's fleet and U's runner → no row. The slot's fencing token and expiry and the admission's `lease_fence` are unchanged. The same bind with T's runner returns a row.
- **Dimension 3.3** — issue refuses under the host lock → Test `issue_refuses_a_foreign_runner_under_the_host_lock` (integration): an `Acquired` for T's fleet handed straight to `Leases::issue` with U's runner → `None`, no `runner.leases` row, and one `runner_placement_foreign_refused` line carrying `UZ-RUN-027` and `lease_type="fleet"`.
- **Dimension 3.4** — setup work is filtered too → Test `authoring_placement_refuses_a_foreign_runner` (integration): `candidates.sql` never offers T's queued draft to U's runner. `claim_queued` with U's runner returns `None` and logs `lease_type="authoring"`. T's runner then claims the draft.

### §4 — No platform key reaches a tenant runner

A tenant runner never takes work whose revision runs on the platform default model. The revision that runs is the task's accepted revision, or else the fleet's head (`claim_admission_head.sql`'s `installed` step). The scan's head step and the claim's `head` step both read that revision. For a tenant runner they require its `runtime_model_ref` to be set and not `afd_credential::provider::PLATFORM_MODEL_REF`, bound by position per NSQ. Such work stays for platform runners.

`Leases::issue` re-checks under the host lock: a tenant runner with `Billed.posture` of `platform` returns `None`. The claim's fence then releases the slot, as a lost claim does. The decision key path is closed by §1's check: a tenant runner never has a decision model, so `lock_host` reports `decision_selected` false. `ExecutionPolicy` keeps its shape.

**Implementation default:** refuse, not proxy. A daemon-side model proxy would let platform-model fleets run on tenant runners, but it needs a new transport and daemon-side metering; that is M251_001. Token charges stay safe without it: self-managed posture charges no tokens (`afd_billing/src/price/rate.rs`).

- **Dimension 4.1** — the platform key never reaches a tenant runner → Test `platform_key_never_reaches_a_tenant_runner` (integration): the platform default key is a sentinel drawn per run. T's fleet on the platform default has an admission, and only T's runner is online. Two consecutive polls give T's runner no candidate, no slot write and no lease. A platform runner then leases the fleet, and its issued `ExecutionPolicy.api_key` equals the sentinel, so a test that never builds a key fails.
- **Dimension 4.2** — a tenant runner carries its tenant's key → Test `tenant_runner_carries_the_tenant_key` (integration): T's fleet on T's own model leases on T's runner. The issued `ExecutionPolicy.api_key` equals T's vault key, and the posture is `self_managed`.
- **Dimension 4.3** — a platform task revision is filtered and re-checked → Test `platform_task_revision_never_leases_on_a_tenant_runner` (integration): T's head revision names T's model while a task's accepted revision names `platform-default`. Two polls give T's runner no candidate. `Leases::issue` handed that task's `Acquired` with T's runner returns `None` and writes no lease.
- **Dimension 4.4** — a real tenant host completes a chat → Test `tenant_runner_completes_a_tenant_model_chat` (e2e, `integration_rust_runner::`): a tenant enrols through `POST /v1/tenants/me/runners`, the runner binary boots with that token, leases T's chat on T's own model, completes it against the fake model, and its report lands.

### §5 — The tenant `runners` command group

`agentsfleet runners` serves a tenant what `agentsfleet platform runners` serves an operator. Both groups come from one definition. A `RunnerReach` value names a reach's API path, the scope each verb needs and the verbs it offers. `PLATFORM_RUNNERS` keeps today's path, scopes and twelve verbs. `TENANT_RUNNERS` uses `/v1/tenants/me/runners` and `runner:tenant`, and offers all verbs except `events` and `leases`: ten in total. Each handler in `cli/src/commands/platform/runners.ts` takes a `RunnerReach`. One factory in `platform-runners.command.ts` builds a group from it and hides a verb the session's scopes do not grant. `discovery.ts` learns scopes for the `runners` path as it does for `platform`.

`enroll` on the tenant reach sends a fresh UUIDv7 `Idempotency-Key`. It saves the token through the existing `--token-out` private file (mode 0600) and prints the token file's path. A replayed enrolment prints no token and exits 1, suggesting `agentsfleet runners rotate-token <runner_id> --token-out <path>`. **Implementation default:** a data value, not a class per reach, because the reaches differ only in path, scopes and verb list.

- **Dimension 5.1** — enrol saves the token once → Test `runners enroll saves the token and sends a key` (unit, `cli/test/runners.unit.test.ts`): a mocked `201` → the token lands in the `--token-out` file only, and the request carried a UUIDv7 `Idempotency-Key` against `/v1/tenants/me/runners`.
- **Dimension 5.2** — a missing scope says which → Test `runners enroll without runner:tenant names the scope` (unit): a mocked `403 UZ-AUTH-022` → exit 1 and a JSON error naming `runner:tenant`, with a suggestion.
- **Dimension 5.3** — a replayed enrol tells the user how to recover → Test `runners enroll replay suggests rotate-token` (unit): a mocked `200` without a token → exit 1, no token file written, and a suggestion naming `agentsfleet runners rotate-token` with the runner's id.
- **Dimension 5.4** — the operator group is unchanged → Test `platform runners still call the admin path` (unit, `cli/test/platform-runners.unit.test.ts`): every operator verb still requests `/v1/admin/runners`.
- **Dimension 5.5** — the tenant group offers only tenant verbs → Test `tenant runners group hides operator-only verbs` (unit, `cli/test/scoped-discovery.unit.test.ts`): with `runner:tenant` alone, `agentsfleet runners --help` lists ten verbs and neither `events` nor `leases`. Without the scope, the group is not listed.

### §6 — The trust model, written down and published

`docs/AUTH.md` rewrites the runner principal section: a runner's tenant, tenant enrolment, `runner:tenant` and the platform-key refusals. Its scope catalogue and `TENANT_OWNER_GRANT` row change. "An admin cannot enroll a runner" becomes "an owner enrols only runners that take its own tenant's work". `runner_fleet.md` changes §Registering a runner and eligibility; labels become placement only, never a security control. `postgres_schemas.md` gains the column, the check, the receipt table and the deletion-matrix row. `runner_protocol.md` notes that a tenant runner needs a stream URL, CA file and route like any external runner. `public/openapi.json` is regenerated. The docs repository gets a changelog entry and a "Run your own runner" page on its own branch. API reference pages land after this Pull Request merges.

- **Dimension 6.1** — Indy accepts the written trust model → Test `trust_model_approved` (manual): Indy reads the rewritten runner sections of `AUTH.md` and `runner_fleet.md`, and his reply is quoted in the Pull Request Session Notes.

## Interfaces

| Method and path | Scope | Answers |
|---|---|---|
| `POST /v1/tenants/me/runners` (`Idempotency-Key`, a UUIDv7, required) | `runner:tenant` | `201` `RegisterResponse` with `runner_token`; `200` replay with `runner_id` and `assigned_policy`, no token; `400 UZ-REQ-001`; `401 UZ-AUTH-*` for an `agt_r` bearer; `403 UZ-AUTH-022`; `409 UZ-RUN-025` |
| `GET /v1/tenants/me/runners?limit=&starting_after=` | `runner:tenant` | `200`, the admin list item shape, this tenant's runners only, `total` this tenant's count |
| `GET`, `PATCH`, `DELETE /v1/tenants/me/runners/{runner_id}` | `runner:tenant` | the admin detail, patch and delete answers; `404 UZ-RUN-014` for a runner not owned; `400 UZ-REQ-001` for a `decision_model_id` edit |

The enrolment body is the admin `RegisterRequest` without `decision_model_id`. PATCH takes the admin body: an `action` (`cordon`, `drain`, `revoke`, `rotate`, `selftest`) or a labels and policy edit. The admin list and detail gain `tenant_id` (UUID or null). On a tenant runner, the admin PATCH refuses `rotate` and edits with `409 UZ-RUN-026`.

New codes: `UZ-RUN-025` `RUNNER_ENROLMENT_KEY_REUSED`, "Enrolment key reused with a different request", 409. `UZ-RUN-026` `RUNNER_TENANT_MANAGED`, "Runner is managed by its tenant", 409. `UZ-RUN-027` `RUNNER_PLACEMENT_FOREIGN`, a log-only code for a refused foreign placement. The runner binary, its environment variables and the stream protocol do not change.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Foreign runner at the claim | security | a scan bug, or a direct claim | no row; slot, fence and admission untouched | `claim_statement_refuses_a_foreign_runner` |
| Hole in the claim filter | security | a claim predicate regression | issue refuses under the host lock, no lease row, an `error` line with `UZ-RUN-027` | `issue_refuses_a_foreign_runner_under_the_host_lock` |
| Setup draft to a foreign host | security | authoring scan or claim | never offered; `admit` answers `Skipped` | `authoring_placement_refuses_a_foreign_runner` |
| Platform model key on a tenant host | security | platform-model work placed on a tenant runner | never a candidate; `Leases::issue` refuses `platform` posture | `platform_key_never_reaches_a_tenant_runner`, `platform_task_revision_never_leases_on_a_tenant_runner` |
| Platform decision key on a tenant host | security | a decision model set on a tenant runner | the table check raises; `register` and `edit` answer `400 UZ-REQ-001` | `tenant_runner_refuses_a_decision_model`, `enrolment_retry_replays_without_the_token` |
| Operator takes over a tenant host | security | an admin rotate or edit on a tenant runner | `409 UZ-RUN-026`; row and token unchanged | `platform_reach_cannot_rotate_or_edit_a_tenant_runner` |
| Reading another tenant's host | auth failure | a guessed `runner_id` | `404 UZ-RUN-014`, the other row unchanged | `tenant_lists_and_manages_only_its_runners` |
| Enrolling without the scope | auth failure | a narrowed person or key | `403 UZ-AUTH-022`, no row | `enrolment_without_runner_tenant_is_refused` |
| Retried enrolment | replay | a lost `201` | `200` with the runner and no token; the CLI suggests `rotate-token` | `enrolment_retry_replays_without_the_token`, `runners enroll replay suggests rotate-token` |
| Concurrent duplicate enrolment | race | two posts with one key | one runner; the rest replay | `concurrent_enrolments_with_one_key_leave_one_runner` |
| Malformed enrolment | malformed input | no key, a non-UUIDv7 key, or a decision model | `400 UZ-REQ-001`, no row | `enrolment_retry_replays_without_the_token` |
| Rotation retried | replay | a lost rotate answer | PATCH `rotate` is not idempotent (HTTP-PATCH): each call mints a token, so a retry kills the token the first call returned; the caller rotates again | `tenant_lists_and_manages_only_its_runners` |
| A tenant runner flipped to platform | security | a statement writes `tenant_id` | the trigger raises; the row keeps its tenant | `runner_tenant_cannot_change` |
| Platform-model work with only tenant runners online | capacity | the tenant has no platform runner in reach | work waits for a platform runner | `platform_key_never_reaches_a_tenant_runner` |
| A held fleet moves to the platform model | capacity | fleet F, held on T's runner, switches to `platform-default` | T's runner may not claim it, and platform runners wait for the hold to lapse (`SANDBOX_HOLD_IDLE`, 10 minutes) | `platform_task_revision_never_leases_on_a_tenant_runner` |
| Datastore unavailable at enrolment | dependency unavailable | Postgres refuses or times out | `503`, no row, no receipt; the client retries with the same key | `enrolment_retry_replays_without_the_token` |

No per-tenant runner limit exists in this spec.

## Invariants

1. No fleet or setup lease lands on a runner owned by another tenant. The claim statement's `fleet` predicate and the issue and `admit` re-check under `lock_host.sql`'s `FOR UPDATE` enforce it, against Postgres only; `cross_tenant_placement_matrix` is the counterexample.
2. No lease on a tenant runner carries a platform key. The claim predicate, `Leases::issue`'s posture check and the tenant-or-decision-model table check enforce it; `platform_key_never_reaches_a_tenant_runner`.
3. A runner's tenant never changes after enrolment. The §1 trigger refuses it; `runner_tenant_cannot_change`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `runner_placement_foreign_refused` (log event, `error`, `error_code` `UZ-RUN-027`) | operator | issue or `admit` refuses a foreign runner | `lease_type` (`fleet`, `authoring`), `runner_id` | no tenant, fleet or draft text | `issue_refuses_a_foreign_runner_under_the_host_lock` |

No metric family changes.

## Acceptance Rubric (single scoring surface)

R rows read the log the S3 lane writes: `.tmp/rustd-integration.log` (`make/test-integration-rustd.mk`).

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A foreign runner is refused at scan, claim and issue (§3) | `grep -c "cross_tenant_placement_matrix ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R2 | The claim statement alone refuses a foreign runner (§3) | `grep -c "claim_statement_refuses_a_foreign_runner ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R3 | Setup work never reaches a foreign runner (§3) | `grep -c "authoring_placement_refuses_a_foreign_runner ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R4 | No platform model key reaches a tenant runner (§4) | `grep -c "platform_key_never_reaches_a_tenant_runner ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R5 | No platform decision key reaches a tenant runner (§1) | `grep -c "tenant_runner_refuses_a_decision_model ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R6 | A tenant enrols, and manages only its own runners (§2) | `grep -c -E "(tenant_enrols_a_tenant_runner\|tenant_lists_and_manages_only_its_runners) ... ok" .tmp/rustd-integration.log` | `2` | P0 | |
| R7 | A retried enrolment leaves one runner (§2) | `grep -c "concurrent_enrolments_with_one_key_leave_one_runner ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R8 | A real tenant host completes a chat (§4) | `grep -c "tenant_runner_completes_a_tenant_model_chat ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R9 | The tenant surface is published (§6) | `grep -c "/v1/tenants/me/runners" public/openapi.json` | `1` or more | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `KNOWN_BITS` in `afd_auth/src/scope.rs` | `git grep -n -w KNOWN_BITS -- rustd` | no output |
| `afd_api_runner::enrolment_handler` and `afd_api_runner/src/handler/runner/enrolment.rs` | `git grep -n -w enrolment_handler -- rustd` | no output |
| `.orly/skills/` (moved by orly 0.50.0 into each runtime's skill folder) | `git ls-files .orly/skills` | no output |

Schema this milestone leaves with no runtime reader or writer is dropped in a new slot under Indy's blanket approval (Discovery); none is expected.

## Out of Scope

- Moved to M251_001: the public stream route and its certificate authority; the executor signal audit (the Nov 06 draft's §0); rotation without a restart (its §5); the tenant placement setting; the daemon-side model proxy and its metering; Settings → Runners and the admin badge.
- One-time enrolment codes and self-registration: excluded in the reviewed EGM brief (`concepts_brief.md`, Nov 06, 2026); `runner_fleet.md` keeps mode C for later.
- Counting holds, auto-cordon and checkpoint respawn: M247.
- Tenant-visible runner events and lease history: the admin routes keep them.
- Sharing one runner across tenants, per-workspace runners, and a per-tenant runner limit: not planned.
- Data residency: history, checkpoints and keys pass through `agentsfleet` whatever the runner, and the docs page says so.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An owner runs `agentsfleet runners enroll --host-id build-box-1 --token-out ./runner.token`. They start the runner on a machine that reaches the stream listener through a route their operator provides, since the public route is M251_001. They send their fleet a message, and `agentsfleet runners show` reports that machine busy with it.
2. **Preserved user behaviour** — Fleets on platform runners run as today. A tenant with no runners sees no change. Operators enrol and manage platform runners as before, with a tenant column added.
3. **Optimal-way check** — One mint and one placement predicate deliver the moment. The gaps: existing accounts need `runner:tenant` granted, platform-model fleets run only on platform runners, and the stream route is the operator's until M251_001.
4. **Rebuild-vs-iterate** — Iterate: the existing enrolment, admin handlers, operator CLI and label placement are extended; no new service.
5. **What we build** — A tenant column and its filter, the `runner:tenant` routes, the platform-key refusals, a tenant CLI group and the one-list scope catalogue.
6. **What we do NOT build** — Enrolment codes (excluded); tenant lease history (admin only); cross-tenant sharing (breaks the boundary); a dashboard page, a placement setting and a model proxy (M251_001).
7. **Fit with existing features** — Compounds with M243's stream and with labels. The platform claim, the hottest statement, gains one predicate and must not slow.
8. **Surface order** — CLI first: an operator installing a host works in a terminal, and the token comes from the same call. The dashboard follows in M251_001.
9. **Dashboard restraint** — No dashboard change in this spec.
10. **Confused-user next step** — "My fleet doesn't start on my runner": `agentsfleet runners show` reports the runner's state. The docs page explains that a fleet on the platform default model runs only on platform runners.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** schema, then the tenant routes that create tenant runners, then the filter that confines them, then the key refusals, then the CLI and the docs. Each Section is testable on the one before it.
- **Alternatives considered:**
  - Labels as the control: rejected, because an untagged fleet matches any runner.
  - The tenant on the principal: rejected, because the host-lock read is the authority.
  - A tenant filter on the admin list: rejected, because a missing tenant would list every runner.
  - A model proxy: moved to M251_001, because it needs a new transport and daemon-side metering.
  - A retry key column on the runner row: rejected by SQL-RETRYKEY.
  - A `RunnerPlane` trait: rejected, because the reaches differ in one value, not in behaviour.
- **Patch-vs-refactor verdict:** a **patch**. It adds a column, a check, a receipt table, a predicate and one route family to paths that keep their shape. The scope catalogue's rewrite keeps every claim string.

## Discovery (consult log)

- **Consults** — Source review on `main` `14539a73e`, Nov 06, 2026, re-verified on `97224ed74`, Nov 17, 2026:
  - Placement checks only `required_tags <@ labels` (`select_ledger_candidates.sql`). The claim re-checks neither labels nor owner (`claim_admission_head.sql`). `lock_host.sql` already reads the runner row `FOR UPDATE`.
  - A durable task runs `COALESCE(t.accepted_revision_id, f.head_revision_id)` (`claim_admission_head.sql`, `installed` step), so the model filter reads that revision, not the head alone.
  - A runner's decision model reaches the lease (`afd_fleet/src/lease/issue.rs`), and a decision reservation hands it the platform decision key (`afd_fleet/src/task/decision/dispatch.rs`).
  - The operator route is `/v1/admin/runners` (`af_wire/src/paths.rs`). `UZ-RUN-024` is M243's stream disconnect, so the new codes start at `UZ-RUN-025`.
  - The admin list's `total` comes from `count_runners.sql`, which has no tenant filter (`afd_runner/src/view/store.rs`).
  - `Capacity` and `Held` derive `Copy`, while `Uuid7` does not (`af_core/src/id.rs`), so they carry a `foreign` boolean.
  - The operator CLI ships twelve runner verbs (`cli/src/program/tree/platform-runners.command.ts`), and discovery reads scopes only under `platform` (`cli/src/program/entry/discovery.ts`).
  - The stream listener is private to Fly (`deploy/fly/agentsfleetd-prod/fly.toml`); the runner trusts OS roots plus an optional CA file (`afr_supervisor/src/config/stream.rs`).
  - Rule interpretation, an agent default: HTTP-IDEMKEY asks a replay to return the first response, and AUTH-SECRET forbids keeping the token. The replay therefore returns the first response without its token. Indy may overrule it at the trust-model review (Dimension 6.1).
- **EGM review** — four reviewers on Claude Opus 5.5, Nov 17, 2026: security and design, references, readiness and standards, structure and style. Findings: 2 blockers (the decision key path, and the head-only model filter looping on task revisions), 15 major and about 25 minor. All are applied above except where the owner decides.
- **Owner decisions** —
  > Indy (Nov 06, 2026: 10:19 AM): "This will be on by default durable_tasks_v1. ... my plan also was to have folks spin a runner and point to the agentsfleetd (in cloud) and keep using it." — context: tenant-enrolled runners against the cloud daemon.
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this." — context: labels stay placement; the tenant setting is M251_001.
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: blanket approval, asked as "Should each milestone drop the tables and columns it leaves with no reader or writer?"
  > Indy (Nov 17, 2026): "so lets focus on the needed one that can help end users" — context: trimming M246 to the end-user path.
  > Indy (Nov 17, 2026): "Based on the above i want to prioritize the above - tenant runs a runner" — context: the scope of this rescope.
  > Indy (Nov 17, 2026): "I dont want to decide on this. as its not important and it depends on the CA authority the agentsfleet have." — context: the public stream route and its certificate authority, moved to M251_001 §1.
  > Indy (Nov 17, 2026): "To me in this spec, we need these, can you trim the spec and create this one in the current spec, the other review audit and move to a new spec after M250 (the last current sequence)" — context: the transfer of the executor signal audit and the remaining Sections to M251_001; this supersedes his Nov 10 "Fold into M246".
  > Indy (Nov 17, 2026): "Lets stick to standard named  Scope::RunnerOwn (runner:own) to  Scope::RunnerTenant (runner:teanant) TENANT_GRANT" — context: the scope is `runner:tenant`, and it joins `TENANT_OWNER_GRANT`.
  > Indy (Nov 17, 2026): "can we see if we can leverage via traits, so its an abstraction ... or would you go with composition" and, for the CLI, "try and figure out an abstraction ... so we are scalable and performant, no new code smell." — context: §2's `Reach` and §5's `RunnerReach`.
  > Indy (Nov 17, 2026): "Also in this PR i need the orly to be updated to 0.50, ensure the global orly is updated to 0.50 and the previous version removed." — context: the managed-rules row in Files Changed.
  > Indy (Nov 17, 2026): "i want the spec 249_* first part to be implemented here, 251_* can be implmented later. This is not just a doc spec." — context: read as M246_001, the first part; this Pull Request carries its implementation.
  > Indy (Nov 17, 2026): "2. scope-macro Fold it into the M246 Pull Request. The tenant urls must be v1/tenants/me/runners. \"approved\"" — context: the `scopes!` rewrite rides this Pull Request; the tenant routes stay under `/v1/tenants/me/runners`.
  > Indy (Nov 17, 2026): "yes apply the mechanical fixes and start" — context: the EGM findings are applied and implementation starts.
- **Open blockers** — none.
- **Deferrals and transfers** — The Nov 06 draft's §0, §5, placement setting, model proxy, Settings → Runners and admin badge transfer to M251_001, under Indy's Nov 17 quote above. No `MOVED` row is used. Each moved criterion is rewritten as M251_001's own P0 row: the executor signal outcome, overlap rotation, the token reload, the placement holder refusal, the proxied key and metering, the proxied chat and the Settings page.
