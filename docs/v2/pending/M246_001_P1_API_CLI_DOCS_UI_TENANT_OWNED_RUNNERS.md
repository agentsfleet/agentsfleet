<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M246_001: A tenant's own runner takes only its tenant's work, never holds the platform key, and rotates its token without a restart

**Prototype:** v2.0.0

**Milestone:** M246

**Workstream:** 001

**Date:** Nov 06, 2026

**Status:** PENDING

**Priority:** P1, customers will point their own hosts at the cloud `agentsfleetd`, and today any enrolled host receives every tenant's work and the platform model key

**Categories:** API, CLI, DOCS, UI

**Batch:** B1, milestone 6 of the push-scheduler programme; starts after M243 merges, and ships as its own Pull Request because it moves a security boundary (SPEC-BOUNDARY)

**Depends on:** M243_001, tenant runners connect over its runner stream, which authenticates once per connection, re-checks on an interval and closes on revoke; M241_001, which creates `push_scheduler.md` and the bench baselines

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the EGM brief and source research on main 14539a73e

**Canonical architecture:** `docs/architecture/push_scheduler.md` §M246 (created by M241_001); `docs/AUTH.md` §"Runner token (`agt_r`) — the machine principal" and `docs/architecture/runner_fleet.md` §"Registering a runner" own the trust model this changes

---

## Overview

**Goal (testable):** A runner a tenant enrols leases only that tenant's fleet and setup work, is refused another tenant's work at claim and at issue, receives a proxied model capability instead of the platform key, and keeps working through a token rotation with no restart.

**Problem:** Indy wants people to "spin a runner and point to the agentsfleetd (in cloud) and keep using it". Today that host would be unsafe to admit:
- Only platform operators can enrol: `POST /v1/runners` requires `runner:enroll` (`afd_http/src/route/runner_ops.rs`), and `TENANT_OWNER_GRANT` lacks it (`afd_auth/src/scope.rs`).
- Nothing keeps another tenant's work off a host. Placement checks only `required_tags <@ labels` (`select_ledger_candidates.sql`), so an untagged fleet matches any runner. The claim (`claim_admission_head.sql`) and issue (`afd_fleet/src/lease/issue.rs`) re-check neither labels nor owner, and authoring's `candidates.sql` has no runner filter at all.
- Every lease carries the resolved model key, the platform's included (`afd_gate/src/policy/build/mod.rs`, `billing_and_provider_keys.md:33`, `runner_fleet.md:192`). The runner also reports its own token counts (`billing_and_provider_keys.md:230`), so a tenant host would write its own platform token bill.
- Rotation overwrites `token_hash` and the old token dies on the next read (`rotate_runner_token.sql`). The runner reads `AGENTSFLEET_RUNNER_TOKEN` once at boot (`afr_supervisor/src/config/mod.rs`), so every rotation is a restart.

**Solution summary:** `runner.runners` gains an owner tenant (NULL is a platform host). Tenant owners enrol and manage their own runners under a new `runner:own` scope, from the dashboard and the command-line interface (CLI). A hard ownership filter sits in the claim statement and is re-checked at issue under the host lock, for fleet and setup (authoring) leases alike. A tenant setting picks one of three placements. A tenant runner on platform posture gets a proxied model-call capability, and the daemon calls the provider and meters the tokens. Rotation keeps the old digest for an overlap window, and the runner reloads its token from a file on SIGHUP or a 401.

**Design flow:**

```
owner ─ Settings → Runners, or `agentsfleet runner create` ─► POST /v1/tenants/me/runners
   (runner:own, Idempotency-Key) ─► runners row, owner_tenant_id = caller's tenant ─► agt_r shown once
host: token in AGENTSFLEET_RUNNER_TOKEN_FILE ─► agentsfleet-runner ─► M243 runner stream (auth once)
admission for fleet F of tenant T, offered to runner R:
   candidate scan ─ R.owner = T, or R.owner NULL and T's placement admits platform hosts
   claim statement ─ same predicate gates the slot upsert; refused → no row, slot untouched
   issue / authoring admit ─ lock_host returns R.owner FOR UPDATE; a foreign owner → refuse, count
   gate ─ R tenant-owned and posture platform → model_access proxied {token, path}; else direct {api_key}
proxied turn: runner ─► POST /v1/runners/me/leases/{lease_id}/model-calls
   ─► daemon opens the platform key, runs af_inference::stream ─► chunk frames back
   ─► usage line logged by the daemon under the lease row lock, then the end frame
rotate: current ← new digest, previous ← old digest until now + RUNNER_TOKEN_OVERLAP
   host: new token into the file, then SIGHUP or the next 401 ─► reload ─► new stream supersedes old
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): tenant-owned runners behind an ownership filter
- **Intent (one sentence):** let a customer run fleets on their own machine without that machine ever seeing another customer's work or the platform's model key.
- **Authoring handshake:** Indy wants tenants to enrol their own hosts against the cloud daemon, with respawn across ownership as a tenant setting. The ownership filter is a tested security control apart from labels, and widening goes one way only: tenant work may go to platform hosts, never to another tenant's runner. ASSUMPTIONS I'M MAKING:
  1. M243 has merged: runners hold one stream, authenticated once per connection and re-checked on an interval, and revoke closes it. This spec only changes what that re-check compares (§5).
  2. Existing accounts receive `runner:own` by an identity-provider edit (`docs/AUTH.md` §"Development provisioning"). Pre-launch, no backfill job.
  3. A tenant with no placement row reads `own_or_platform`, which is today's placement for every tenant.
  4. Runner and daemon ship together, so `ExecutionPolicy` changes shape with no compatibility field. No customer runner exists yet, and M243's handshake refuses an older runner.
  5. The two open blockers in Discovery keep this spec in `pending/` until Indy answers.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE). CHORE(open) re-verifies Files Changed against `origin/main`: PR #803 edits the same runner SQL, M242, M243 and M245 rewrite the claim, renew and report paths, and schema and error-code numbers are taken fresh (SPEC-NUM).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/claim_admission_head.sql` and `select_ledger_candidates.sql` — the claim and the scan the ownership predicate enters; the claim is the control, the scan only a filter.
2. `rustd/crates/daemon/afd_runner/src/capacity/mod.rs` and `capacity/sql/lock_host.sql` — the host lock both `Leases::issue` and the authoring `admit` take; the owner rides this row.
3. `rustd/crates/daemon/afd_authoring/src/model/capability.rs`, `af_wire/src/authoring/lease.rs` (`ModelAccess`) and `afd_api_runner/src/handler/runner/authoring/model_call.rs` — the proxied capability §4 mirrors.
4. `docs/AUTH.md` §"Runner token (`agt_r`) — the machine principal" — the trust model, per-request runner auth and why the principal carries no tenant.
5. `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/api_key.rs` with `cli/src/commands/api_key.ts` and `ui/packages/app/app/(dashboard)/settings/api-keys/` — the tenant-minted, reveal-once credential surfaces §2 and §6 mirror.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M246_001_P1_API_CLI_DOCS_UI_TENANT_OWNED_RUNNERS.md` | CREATE, then MOVE to `active/` and `done/` | this spec |
| `schema/2850_runner_runners_owner_and_token_overlap.sql`, `2860_runner_tenant_placements.sql`, `2870_runner_leases_model_token.sql`, `2880_runner_enrolment_receipts.sql` | CREATE | §1; placeholder numbers, renumbered from `origin/main` at CHORE(open) |
| `rustd/crates/daemon/afd_db/src/migration.rs` and its position assertions | EDIT | §1: register the slots (MIG) |
| `rustd/crates/daemon/afd_state/src/sql/select_runner_token.sql`, `credentials/mod.rs`, `credentials/rows.rs`, `credentials/tests.rs` | EDIT | §1, §5: the previous digest authenticates inside its window |
| `rustd/crates/foundation/af_core/src/timing.rs` | EDIT | §3, §5: `PLATFORM_FALLBACK_AFTER`, `RUNNER_TOKEN_OVERLAP`, with their assertions |
| `rustd/crates/foundation/af_core/src/error_code/fleet.rs`, `error_code/mod.rs`, `problem/fleet.rs` | EDIT | §2: `UZ-RUN-024` const, `REGISTRY` row, problem entry |
| `rustd/crates/daemon/afd_auth/src/scope.rs` and its tests | EDIT | §2: `Scope::RunnerOwn`; `TENANT_OWNER_GRANT` grows to 11 |
| `rustd/crates/daemon/afd_runner/src/store.rs`, `rotate.rs`, `sql/runner/insert_runner_with_event.sql`, `sql/runner/rotate_runner_token.sql` | EDIT | §2, §5: owner and receipt at enrolment; overlap rotation |
| `rustd/crates/daemon/afd_runner/src/sql/runner/select_runner_owned.sql`, `sql/runner_view/list_owned_runners_first.sql`, `list_owned_runners_after.sql`, `list_runners_first.sql`, `list_runners_after.sql`, `runner_detail.sql`, `view/` | CREATE, EDIT | §2: ownership proof, owned list, operator views gain the owner |
| `rustd/crates/daemon/afd_runner/src/placement/mod.rs`, `placement/tests.rs`, `placement/sql/*.sql` | CREATE | §2, §3: setting names, rule table, settings read and write |
| `rustd/crates/daemon/afd_runner/src/capacity/mod.rs`, `capacity/sql/lock_host.sql` | EDIT | §3: the owner rides the host lock |
| `rustd/crates/daemon/afd_api_wire/src/admin/mod.rs`, `afd_api_wire/src/tenant/runners.rs` | EDIT, CREATE | §2: `owner_tenant_id`; placement wire |
| `rustd/crates/protocol/af_wire/src/paths.rs`, `af_wire/src/policy/mod.rs`, `af_wire/src/redact.rs`, `af_wire/src/runner/model_call.rs` | EDIT, CREATE | §2, §4: paths, `ModelAccess`, model-call frames |
| `rustd/crates/daemon/afd_http/src/route/tenant.rs`, `route/runner.rs`, `services/leasing/mod.rs` | EDIT | §2, §4: four route templates; the model-call service |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/runner/` (`mod.rs`, `enrol.rs`, `manage.rs`, `placement.rs`), `handler/tenant/mod.rs`, `lib.rs`, `openapi.rs` | CREATE, EDIT | §2: tenant handlers and their mount |
| `rustd/crates/daemon/afd_api_operator/src/handler/operator/runners.rs` | EDIT | §2: owner in the operator view |
| `rustd/crates/daemon/afd_api/tests/route_inventory.rs`, `route_meta_total.rs`, and a new `integration_tenant_runners.rs` | EDIT, CREATE | §2: `INVENTORY`, counts, route tests |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql`, `claim_admission_head.sql`, `lease/assign/`, `lease/issue.rs`, `lease/deliver/mod.rs` | EDIT | §3, §4: the predicate, its binds, the issue re-check, owner into the gate |
| `rustd/crates/daemon/afd_authoring/src/lease/claim/sql/candidates.sql`, `claim/mod.rs`, `claim/selection.rs` | EDIT | §3: the same filter for setup work |
| `rustd/crates/daemon/afd_gate/src/policy/build/mod.rs` | EDIT | §4: direct or proxied access by owner and posture |
| `rustd/crates/daemon/afd_fleet/src/lease/model_call/` (`mod.rs`, `meter.rs`, `tests.rs`), `lease/usage/mod.rs`, `lease/renew/`, `lease/report/`, `Cargo.toml` | CREATE, EDIT | §4: daemon-side turn, daemon metering, runner lines skipped |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/model_call.rs`, `lib.rs`, `openapi.rs` | CREATE, EDIT | §4: the runner-plane route |
| `rustd/crates/runner/afr_supervisor/src/model_proxy/` (`mod.rs`, `tests.rs`), `lib.rs`, `client/`, `config/mod.rs`, `config/tests.rs`, `halt/mod.rs` | CREATE, EDIT | §4, §5: proxied provider; token file and reload |
| `rustd/crates/harness/af_providers/src/connect/mod.rs`, `turn.rs`; `afr_sandbox/examples/kernel_lane/supervisor_plane.rs` | EDIT | §4: read `ModelAccess::Direct` only |
| `rustd/crates/runner/main/src/main.rs`, `af_core/src/signal.rs`, `agentsfleet_runner/tests/fake_daemon.rs`, `tests/fixtures/runner/help.txt` | EDIT | §5: SIGHUP reload and its tests; help lists the file variable |
| The M243 gateway's stream re-check (path fixed when M243 merges) | EDIT | §5: re-check by the digest that opened the connection |
| `rustd/crates/daemon/afd_fleet/tests/integration_runner_owner.rs`, `integration_runner_owner/`; `afd_authoring/tests/` placement module | CREATE | §3: matrix, claim, issue, hold, authoring |
| `rustd/crates/daemon/main/tests/integration_rust_runner_tenant.rs`, `daemon_suite.rs`, `tests/support/fake_model.rs` | CREATE, EDIT | §4: end-to-end run; the fake records the auth header it received |
| `rustd/crates/protocol/af_observability/src/metrics/declared/fleet.rs`, `producers/fleet/mod.rs`, `docs/metrics.census.tsv` | EDIT | §3, §4: two operator families |
| `cli/src/commands/runner.ts`, `cli/src/program/tree/runner.command.ts`, `root.command.ts`, `flags.ts`, `cli/test/runner.integration.test.ts` | CREATE, EDIT | §6: the `runner` command group |
| `ui/packages/app/app/(dashboard)/settings/runners/` (`page.tsx`, `actions.ts`, `loading.tsx`, `components/`), `settings/page.tsx` | CREATE, EDIT | §6: Settings → Runners |
| `ui/packages/app/lib/api/tenant_runners.ts`, `lib/auth/scopes.ts`, `lib/analytics/events.ts` | CREATE, EDIT | §6: client, scope, product events |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerTile.tsx`, `AddRunnerDialog.tsx` | EDIT | §6: owner badge; the dialog takes its server action |
| `ui/packages/app/tests/runners-settings-*.test.tsx`, `tests/e2e/dashboard-smoke.spec.ts`, `tests/e2e/acceptance/settings-runners.spec.ts` | CREATE, EDIT | §6 |
| `docs/AUTH.md`, `docs/architecture/runner_fleet.md`, `billing_and_provider_keys.md`, `runner_execution.md`, `postgres_schemas.md`, `observability.md`, `push_scheduler.md` | EDIT | §7 |
| `public/openapi.json` | EDIT (regenerated with `--no-banner`) | §7 |
| `~/Projects/docs`, branch `chore/m246-tenant-runners-changelog` | EDIT | §7: changelog and a runner setup page; API pages after merge |

## Applicable Rules

- **`domains/auth.md`** — AUTH-DOC, AUTH-GUARD, AWO, AUTH-SECRET, AUTH-NOLOG, AUTH-RANDOM, CTM, CTC
- **`domains/sql.md`** — SCH, MIG, SQL-SLOT, STS, SQL-READER, SQL-ERASE, SQL-ID, SQL-FKNAME, SQL-RETRYKEY, SQL-DEADLINE, SQL-LOCKCLOCK, SQL-FILE, SQL-PREPARE, SQL-TIMEOUT, SGR
- **`domains/http.md`** — HTTP-NOUN, HTTP-VERB, HTTP-PATCH, HTTP-IDEMKEY, HTTP-GARDE, HTTP-VALID, HTTP-ROUTE
- **`domains/http-responses.md`** — HTTP-STATEEXT, HTTP-NEWCODE, HTTP-LEAK, HTTP-PAGE; **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-SURFACE, HTTP-ENUM
- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SENTINEL, RS-NEWTYPE; **`languages/typescript.md`** — TS-SCHEMA, TS-ERRSTYLE, TS-BUNTEST
- **`code.md`** — CODE-SEAM, FLL, NLG, CODE-ERRREG; **`domains/logging.md`** — LOG-SECRET, LOG-EVENT, OBS
- **`tests.md`** — TEST-NEG, TEST-REAL, TEST-ABSENT, TEST-RACE, TEST-LANE; **`docs.md`** — DOC-02, DOC-03, DOC-11; **`domains/architecture.md`** — ARCH-LAND, ARCH-PUBLISHED

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `gitleaks` at commit and `orly gate pr` | yes, the key-sentinel test | the sentinel is drawn from `af_crypto::entropy` at test time; no key-shaped literal is committed |
| `make lint-all` (clippy `-D warnings`, `cargo shear`, TypeScript lint) | yes, eleven crates, the CLI and the app change | `lint-rustd` and the app lint per Section |
| Route inventory and `route_meta_total` | yes, four templates, eight operations | `INVENTORY` and counts move in §2's and §4's commits |
| Error registry and problem parity | yes, `UZ-RUN-024` | const, row and problem entry in one commit |
| Metrics census and producer tests | yes, two families | census rows, declarations and producers in one commit |
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, `AUTH.md` and five architecture pages | sentences under 25 words, paragraphs of at most three |
| Vitest coverage lock | yes, new app files | tests land with each component |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `afd_fleet` lease, the claim SQL binds and the tenant handlers | `model_call/` split into call and meter; tenant handlers split by verb |

## Prior-Art / Reference Implementations

- **Reference:** GitLab's runner scopes (https://docs.gitlab.com/ci/runners/runners_scope/) — project runners serve only their project while instance runners serve everyone, which is the one-way widening here; `runner_fleet.md:227` already follows GitLab's "create runner, then token" enrolment. Divergence: the tenant chooses whether platform hosts may take its work.
- **Reference:** `afd_runner::Runners::register` and `afd_api_runner/.../enrolment.rs` — the mint, the digest-only row and the reveal-once response; divergence: an owner, and an `Idempotency-Key` receipt.
- **Reference:** `afd_authoring/src/model/capability.rs` and the authoring model-call route — a lease-bound capability checked in constant time; divergence: the runtime turn streams chunk frames instead of one bounded result, and meters runtime usage.
- **Reference:** `schema/20261108121600_model_tenant_selections.sql` — a per-tenant row keyed by `tenant_id`, where a missing row means the default.
- **Reference:** `cli/src/commands/api_key.ts` — the CLI follows all six practices: command, handler and error layers apart; handlers that neither print nor exit; one rendering layer; structured JSON errors with suggestion and retry fields; handler, in-process and subprocess tests; JSON by default when stdout is not a terminal.

## Sections (implementation slices)

### §1 — The runner row carries its owner and a second token

Four slots in the assigned range 2850–2890, renumbered from `origin/main` at CHORE(open) because PR #803 (open) takes `schema/2190-2690`:
- `2850` alters `runner.runners`: `owner_tenant_id` referencing `identity.tenants(id) ON DELETE CASCADE` (NULL is a platform host), with a partial index; `previous_token_hash` and `previous_token_expires_at` (a NULL marker), both or neither, with a partial index; a trigger refusing any change to `owner_tenant_id`.
- `2860` creates `runner.tenant_placements`: one row per tenant, keyed by `tenant_id` (the SQL-ID exception `model.tenant_selections` already states), and `placement_type` validated in the application.
- `2870` adds `runner.leases.model_token_hash`, set only on a proxied lease (§4).
- `2880` creates `runner.enrolment_receipts`: `UNIQUE (tenant_id, idempotency_key)`, the request digest and the enrolled `runner_id`, cascading with the runner.

`select_runner_token.sql` matches the current digest, or the previous one while `previous_token_expires_at > statement_timestamp()`. The deletion matrix gains the runners, placement row and receipts under Tenant. **Implementation default:** the owner lives on the row and rides the host lock, never on `Principal::Runner`, because the lock read is the authority at placement and a principal copy would be a staler second one. **Implementation default:** a trigger keeps the owner fixed, because flipping a tenant host to NULL would make it a platform host that receives every tenant's secrets.

- **Dimension 1.1** — the owner cannot change → Test `runner_owner_cannot_change` (integration): an `UPDATE` setting a tenant runner's `owner_tenant_id` to NULL raises, and the row still names its tenant.
- **Dimension 1.2** — a previous digest needs its expiry → Test `previous_token_needs_its_expiry` (integration): writing `previous_token_hash` with a NULL expiry violates the check.
- **Dimension 1.3** — the lookup honours the window → Test `token_lookup_honours_the_overlap` (integration): a previous digest expiring 1 s ahead authenticates; 1 s behind, it returns `Ok(None)`, never `Unavailable`.
- **Dimension 1.4** — erasure takes only the tenant's hosts → Test `tenant_erasure_takes_its_runners` (integration): deleting tenant A removes A's two runners, their leases, A's placement row and receipts; a platform runner and tenant B's runner remain.

### §2 — Tenants enrol and manage their own runners

`Scope::RunnerOwn` (`runner:own`, a discrete verb) joins the catalogue and `TENANT_OWNER_GRANT`. The routes in Interfaces are `Guard::Bearer` with `Scopes::Always(RUNNER_OWN)`. Enrolment reuses `RegisterRequest`, `RegisterResponse` and `Runners::register` with the caller's tenant as owner. It requires a UUIDv7 `Idempotency-Key`, and the receipt commits in the enrolment statement. A repeated key answers `409 UZ-RUN-024` with `current_state: "enrolled"` whatever the body, because the one-time token cannot be shown again (AUTH-SECRET); the client recovers by listing. The owned list binds the owner as a required parameter. Get, patch and delete first prove ownership with `select_runner_owned.sql`, then call the operator's store function (`transition`, `edit`, `rotate_token`, `delete_revoked`, `request_selftest`). A runner the caller does not own answers `404 UZ-RUN-014` (AWO). PATCH takes the operator body (`action`, or a labels and policy edit), so one set of dashboard dialogs serves both planes. The operator list and detail gain `owner_tenant_id`.

**Implementation default:** separate tenant routes, not a tenant filter on `/v1/fleets/runners`, because that route serves `runner:read` operators across tenants and a nullable filter turns a missing tenant into a full listing. **Implementation default:** prove, then act, because §1's trigger makes ownership immutable, so the proof cannot go stale between the two statements.

- **Dimension 2.1** — an owner enrols a host → Test `tenant_enrols_its_own_runner` (integration): a session holding `runner:own` posts a valid body → `201`, an `agt_r` once, `Cache-Control: no-store`, and the row's owner is the caller's tenant.
- **Dimension 2.2** — enrolment needs the scope → Test `enrolment_without_runner_own_is_refused` (integration): an owner grant without `runner:own` → `403 UZ-AUTH-022` and no row; an `agt_r` bearer is refused `401` by the plane, before any datastore read.
- **Dimension 2.3** — a retry enrols nothing twice → Test `enrolment_retry_answers_conflict` (integration): the same key twice → the second answers `409 UZ-RUN-024` with `current_state: "enrolled"`, one row; the same key with other labels → `409`, still one row; no key → `400 UZ-REQ-001`.
- **Dimension 2.4** — a tenant sees and touches only its own → Test `tenant_lists_only_its_runners` (integration): A owns two, B one, the platform one. A's list returns exactly A's two ids, and `?limit=1` pages them. A's GET, cordon, rotate and DELETE on B's runner each answer `404 UZ-RUN-014`, and B's row is unchanged. An empty list fails the test, so it cannot pass by hiding everything.
- **Dimension 2.5** — a tenant revoke ends the host's stream → Test `tenant_revoke_closes_its_runner_stream` (integration): A revokes its runner → the M243 stream closes, and a reconnect answers `401 UZ-RUN-009`.
- **Dimension 2.6** — the setting reads its default and refuses nonsense → Test `placement_setting_refuses_unknown_value` (integration): GET with no row → `own_or_platform`; PUT `own_only` → `200` and GET reads it; PUT `anywhere` or `OWN_ONLY` → `400 UZ-REQ-001`, row unchanged.
- **Dimension 2.7** — operators see who owns a host → Test `operator_list_names_each_owner` (integration): the platform row reads `owner_tenant_id: null`; the tenant row reads its tenant.

### §3 — The ownership filter in the claim, at issue and for setup work

`afd_runner::placement` names three settings and one rule. A tenant-owned runner takes only its own tenant's work. A platform host takes a tenant's work unless the tenant chose `own_only`, and under `own_then_platform` only once the work has waited `PLATFORM_FALLBACK_AFTER`. Another tenant's runner is never eligible. Its rule table is the single statement of the rule, and the SQL binds the setting names from its constants (STS).

The rule's authority is `runner.runners.owner_tenant_id` with `runner.tenant_placements`, read only from Postgres, so any replica decides alike. Three places apply it:
- The candidate scan filters on the runner's owner and the fleet tenant's placement, a performance filter only.
- The claim statement gains a `permitted` step joining the fleet, the runner and the placement, and the slot upsert runs only when it holds. Called with a foreign fleet, it returns no row and leaves the slot and its fencing token untouched. This is the control.
- `lock_host.sql` returns `owner_tenant_id` under `FOR UPDATE`, and `Capacity` carries it. `Leases::issue` and the authoring `admit` refuse a tenant-owned host whose owner is not the work's tenant before any write, and count `agentsfleet_placement_foreign_refusals_total`. A non-zero count means the claim has a hole.

`candidates.sql` gets the same predicate over `authoring.drafts.tenant_id`. "Waited" is the later of the head admission's creation and its slot's last lease expiry, compared with `statement_timestamp()` and a bound interval (SQL-DEADLINE). **Implementation default:** the issue re-check covers ownership only, because the setting is a preference the claim settled under the slot lock, while ownership is the boundary. **Implementation default:** no preference for own runners under `own_or_platform`, because labels and stickiness already order the scan.

- **Dimension 3.1** — the rule table is complete → Test `placement_rule_covers_every_cell` (unit): three settings by own, platform and foreign, plus `own_then_platform` before and after the threshold → 12 verdicts; foreign is false in every setting, platform is false under `own_only`.
- **Dimension 3.2** — the live matrix matches the table → Test `cross_tenant_placement_matrix` (integration): each of the nine cells is seeded live (tenant T's fleet with one admission; an own, a platform and a foreign runner) and driven through the scan, the claim and issue. Verdicts equal 3.1's table; every foreign cell yields no candidate, no slot write and no lease row. Own cells must lease, so a filter that refuses everything fails.
- **Dimension 3.3** — the claim alone refuses → Test `claim_statement_refuses_a_foreign_runner` (integration): `claim_admission_head.sql` bound directly with T's fleet and tenant U's runner → no row; the slot's fencing token and expiry and the admission's `lease_fence` are unchanged. The same bind with T's runner returns a row.
- **Dimension 3.4** — issue refuses under the host lock → Test `issue_refuses_a_foreign_runner_under_the_host_lock` (integration): an `Acquired` for T's fleet handed straight to `Leases::issue` with U's runner → `None`, no `runner.leases` row, and the counter rises by one with `lease_type="fleet"`.
- **Dimension 3.5** — setup work is filtered too → Test `authoring_placement_refuses_a_foreign_runner` (integration): `candidates.sql` never offers T's queued draft to U's runner; `claim_queued` with U's runner returns `None` and counts `lease_type="authoring"`; T's runner then claims it.
- **Dimension 3.6** — fallback waits for its threshold → Test `own_then_platform_waits_for_the_threshold` (integration): with no own runner online, a platform host claiming at `PLATFORM_FALLBACK_AFTER` less 1 s gets nothing, and at 1 s past it leases.
- **Dimension 3.7** — a new setting refuses a platform holder → Test `placement_change_refuses_the_platform_holder` (integration): T's fleet is held on a platform host; T sets `own_only` → the holder's next claim gets nothing, and T's runner claims once the hold lapses.

### §4 — The platform key never leaves the daemon

`ExecutionPolicy` replaces `api_key`, `inference_host` and `base_url` with `model_access`: `direct` with those three fields, or `proxied` with a token, its expiry and an invocation path, mirroring authoring's `ModelAccess`. `proxied` has no key field, so no code path can fill one. The gate picks `proxied` when the host-locked owner is a tenant and the posture is `platform`; every other pair stays `direct`. Issue draws the capability from `af_crypto::entropy`, stores its SHA-256 as `model_token_hash`, and the lease returns it once.

`POST /v1/runners/me/leases/{lease_id}/model-calls` (`Guard::RunnerBearer`, `runner:self`, `RouteClass::Stream`) checks the lease and the capability before any provider call. The lease must be this runner's, active, at its current fence and proxied, and the capability matches in constant time, as `afd_authoring::model::capability::check` does. `afd_fleet::lease::model_call` re-resolves the platform key through `Providers`, pins the lease's model, and opens one `af_inference::stream::StreamTurn`. Its chunks return as newline-delimited JSON (NDJSON) frames. At the turn's end the daemon logs the call's usage line under the lease row lock, numbered by the daemon in the runner range, and commits it before the `end` frame. Renew and report on a lease with `model_token_hash` skip runner request lines, answer `recorded_through` so the runner forgets them, and log runtime only.

On the runner, `afr_supervisor::model_proxy` implements `af_providers::Connect` over the supervisor's daemon client, and `direct` leases reach today's `Connector` unchanged. **Implementation default:** the runner sends its provider-neutral turn request and the daemon runs the attempt through `af_inference`, rather than forwarding provider bytes. The daemon then pins the model, reads usage from the attempt it ran, and needs no per-format parser. This default stands until open blocker 2 is answered.

- **Dimension 4.1** — the platform key never reaches a tenant host → Test `platform_key_never_reaches_a_tenant_runner` (integration): the platform default key is a sentinel drawn per run. T's runner leases T's platform-posture fleet, makes one proxied call and reports. The sentinel bytes appear in no lease body, model-call frame, renew or report answer, `runner.leases` row or captured daemon log line. The fake provider received exactly the sentinel, so a lease with no model access at all fails the test.
- **Dimension 4.2** — ownership, not posture alone, picks the access → Test `platform_host_keeps_direct_access` (integration): the same fleet on a platform host carries `model_access.type = "direct"`; T's self-managed fleet on T's runner carries `direct` with T's own key.
- **Dimension 4.3** — a wrong or stale capability spends nothing → Test `model_call_refuses_a_wrong_capability` (integration): a wrong token, the previous lease's token after a reclaim, and a stale fence each answer `409 UZ-RUN-005`, and the fake provider counts zero calls.
- **Dimension 4.4** — direct and foreign leases have no proxy → Test `model_call_refuses_a_direct_or_foreign_lease` (integration): a `direct` lease, or another runner's proxied lease → `404 UZ-RUN-006`, zero provider calls.
- **Dimension 4.5** — the daemon writes the token bill → Test `proxied_usage_is_metered_by_the_daemon` (integration): the fake provider reports 1,000 input and 200 output tokens while the runner's renew claims 1 and 1 → `billing.usage_lines` holds one request line of 1,000 and 200 for the lease, none from the runner, and `agentsfleet_proxied_model_calls_total{outcome="ok"}` rose by one.
- **Dimension 4.6** — a provider failure ends the turn cleanly → Test `proxied_call_reports_a_provider_failure` (integration): the fake answers `503` after some text → the stream ends with an `error` frame naming the provider failure, usage known before it is logged once, and the turn fails as a direct provider failure does.
- **Dimension 4.7** — the runner rebuilds the stream faithfully → Test `proxied_provider_streams_daemon_chunks` (unit): scripted text, call, usage and end frames → the same `Chunk` sequence; a stream cut before `end` → an error ending the turn; an unknown frame type → an error naming it.
- **Dimension 4.8** — a real tenant host completes a chat → Test `tenant_runner_completes_a_platform_posture_chat` (e2e, `integration_rust_runner::`): a tenant enrols through §2's route, the runner binary boots from a token file, leases T's platform-posture chat, completes it through the proxy against the fake model, and its report lands; the lease carried `proxied`.

### §5 — Rotation without a restart

Operator and tenant rotation share `Runners::rotate_token`, which locks the runner row, moves the current digest to `previous` with `previous_token_expires_at = clock_timestamp() + RUNNER_TOKEN_OVERLAP`, stores the new digest and commits before the response reveals the token. Inside an open overlap, a second rotation replaces only the current digest and keeps the previous one and its expiry. A retried rotation after a lost response therefore never cuts off the token the host still holds, and the undelivered token dies. Revoke ends both at once through the status gate.

`AGENTSFLEET_RUNNER_TOKEN_FILE` names a file holding the token, and exactly one of it and `AGENTSFLEET_RUNNER_TOKEN` may be set. The supervisor reads the file at boot, on SIGHUP and once on a `401`. A changed token replaces the client's bearer and opens a new M243 stream, which supersedes the old. An unchanged token on a `401` halts as `halt/mod.rs` does today. An unreadable file on SIGHUP keeps the current token and logs `runner_token_reload_failed`. M243's re-check re-runs the token lookup with the digest that opened the connection, so the overlap's end closes a stream on the old token within one re-check interval.

**Implementation default:** `RUNNER_TOKEN_OVERLAP` is one hour, long enough for a hand install and short enough that an old token dies the same day; a suspected leak is a revoke, which skips the overlap. **Implementation default:** keep `AGENTSFLEET_RUNNER_TOKEN`, because `deploy/baremetal/` and the playbooks set it and deployment configuration edits need Indy's approval; a host using it rotates with a restart.

- **Dimension 5.1** — both tokens work, then only the new → Test `rotation_keeps_the_old_token_for_the_overlap` (integration): after a rotation, old and new each answer `200` on `GET /v1/runners/me`; with the expiry moved 1 s into the past, old → `401 UZ-RUN-001` and new → `200`.
- **Dimension 5.2** — a repeated rotation keeps the host's token → Test `rotation_inside_the_overlap_keeps_the_previous_token` (integration): two rotations → the original still `200` until its first expiry, the first new token `401`, the second `200`.
- **Dimension 5.3** — revoke skips the overlap → Test `revoke_ends_both_tokens` (integration): a revoke inside the overlap → both tokens `401 UZ-RUN-009`.
- **Dimension 5.4** — a stream on the old token closes → Test `overlap_end_closes_a_stream_on_the_old_token` (integration): a stream opened with the old token closes within one re-check interval of the expiry, and its reconnect with the old token answers `401`.
- **Dimension 5.5** — a 401 reloads before it halts → Test `runner_reloads_its_token_file_on_401` (unit, `agentsfleet_runner/tests/fake_daemon.rs`): the fake refuses token A while the file holds B → the next call carries B from the same process with no halt; with the file still A, the runner halts with `runner_token_refused`.
- **Dimension 5.6** — SIGHUP swaps the token → Test `sighup_swaps_the_token_without_a_restart` (unit, fake daemon): SIGHUP after writing B → the next connection carries B; SIGHUP with the file removed → still A, `runner_token_reload_failed` logged, no halt.
- **Dimension 5.7** — boot refuses an ambiguous or bad source → Test `config_refuses_token_and_token_file` (unit): both variables set → a boot error naming both; an empty file, a missing path or a non-`agt_r` value → a boot error naming `AGENTSFLEET_RUNNER_TOKEN_FILE`.

### §6 — The CLI and the dashboard

`agentsfleet runner` gains `create` (`--host-id`, repeatable `--label`, `--sandbox-tier`, `--network-policy`, `--worker-count`, a fresh UUIDv7 `Idempotency-Key`), `list`, `rotate`, `cordon`, `drain`, `revoke`, `delete` and `placement`. `placement` prints the setting and how many of the tenant's runners are online; `--set` changes it. `create` and `rotate` print the token once, with the line to put it in `AGENTSFLEET_RUNNER_TOKEN_FILE`.

Settings → Runners (`app/(dashboard)/settings/runners/`) shows only with `runner:own`: the tenant's runners, Add runner (`AddRunnerDialog` and `PolicyFields` with a tenant server action), the reveal-once dialog, row actions and the placement choice. Its three choices read "Only my runners; work waits", "My runners, then AgentsFleet hosts" and "Any matching runner, mine or AgentsFleet's". Under `own_only` with no runner online, the page says "Work waits until one of your runners is online". The admin `RunnerTile` shows a Tenant badge on an owned runner. `runner_token_minted` gains `owner` (`platform` or `tenant`), and `runner_placement_changed` carries `placement`.

- **Dimension 6.1** — create reveals once → Test `runner create prints the token once` (unit, in-process, `cli/test/runner.integration.test.ts`): a mocked `201` → the token printed once on a terminal; piped stdout → JSON carrying `runner_token`; the request carried a UUIDv7 `Idempotency-Key`.
- **Dimension 6.2** — a missing scope says which → Test `runner create without runner:own names the scope` (unit): a mocked `403 UZ-AUTH-022` → exit 1 and a JSON error naming `runner:own`, with a suggestion.
- **Dimension 6.3** — placement refuses a typo → Test `runner placement refuses an unknown value` (unit): `--set anywhere` → exit 2 naming the three values, and no request sent.
- **Dimension 6.4** — piped output is JSON from the built binary → Test `runner list prints JSON when piped` (subprocess, built CLI against the mock API): exit 0 and a JSON array of the two mocked runners.
- **Dimension 6.5** — the page hides without the scope → Test `runners settings page redirects without runner:own` (unit, Vitest): no scope → redirect to `/settings`, and the list is never fetched.
- **Dimension 6.6** — the wait warning is earned → Test `own-only placement warns only with no runner online` (unit, Vitest): `own_only` with two offline runners → the warning shows; with one online → it does not.
- **Dimension 6.7** — the signed-in path works → Test `settings-runners.spec.ts` (e2e, Playwright acceptance, run locally before the Pull Request): an owner adds a runner, sees the token once and never after closing, finds the row listed, and sets `own_only`, which survives a reload.
- **Dimension 6.8** — product events carry fixed words only → Test `runner settings actions capture owner and placement` (unit, Vitest): a tenant enrolment captures `runner_token_minted` with `owner: "tenant"`; a placement save captures `runner_placement_changed` with `placement` alone; a refused save captures nothing.

### §7 — The trust model, written down and published

`docs/AUTH.md` rewrites the runner principal section: owner, tenant enrolment, `runner:own`, overlap rotation, the token file and proxied model access. Its scope catalogue and `TENANT_OWNER_GRANT` row change, and "an admin cannot enroll a runner" becomes "an owner enrols only runners that take its own tenant's work". `runner_fleet.md` changes §Registering a runner, the enrolment sentence at :209, rotation at :251 and eligibility at :337; labels become placement only, never a security control. `billing_and_provider_keys.md` changes the :33 row and §8.2. `runner_execution.md` notes that a proxied lease's supervisor holds no key. `postgres_schemas.md`, `observability.md` and `push_scheduler.md` §M246 follow. `public/openapi.json` is regenerated. The docs repository gets a changelog entry and a "Run your own runner" page on its own branch; API reference pages land after this Pull Request merges.

- **Dimension 7.1** — the pages read clean → Test `orly gate verify` (integration): zero `docs.language` findings on the changed pages.
- **Dimension 7.2** — Indy accepts the written trust model → Test `trust_model_approved` (manual): Indy reads the rewritten runner sections of `AUTH.md` and `runner_fleet.md`; his reply is quoted in the Pull Request Session Notes.

## Interfaces

| Method and path | Scope | Answers |
|---|---|---|
| `POST /v1/tenants/me/runners` (`Idempotency-Key` required) | `runner:own` | `201` `RegisterResponse`, `runner_token` marked `x-secret-handling: one-time-read`; `409 UZ-RUN-024`; `400`; `403` |
| `GET /v1/tenants/me/runners?limit=&starting_after=` | `runner:own` | `200`, the operator list item shape, owned runners only |
| `GET`, `PATCH`, `DELETE /v1/tenants/me/runners/{runner_id}` | `runner:own` | the operator detail, patch and delete answers; `404 UZ-RUN-014` for a runner not owned |
| `GET`, `PUT /v1/tenants/me/runner-placement` | `runner:own` | `{"placement": "own_only"}`, one of `own_only`, `own_then_platform` and `own_or_platform` |
| `POST /v1/runners/me/leases/{lease_id}/model-calls` | `runner:self` | `200` `application/x-ndjson` frames `text`, `call`, `usage`, `end` or `error`; `404 UZ-RUN-006`; `409 UZ-RUN-005` |

A proxied lease's policy carries no key:

```json
"model_access": {"type": "proxied", "token": "<capability>", "expires_at": "2026-11-06T10:00:30.000Z",
                 "invocation_path": "/v1/runners/me/leases/<lease_id>/model-calls"}
```

A direct lease carries `{"type": "direct", "api_key", "inference_host", "base_url"}`. The operator list and detail gain `owner_tenant_id` (UUID or null). `UZ-RUN-024` is a `409` carrying `current_state: "enrolled"`. The runner reads `AGENTSFLEET_RUNNER_TOKEN_FILE` and reloads on SIGHUP. `af_core::timing` adds `RUNNER_TOKEN_OVERLAP` and `PLATFORM_FALLBACK_AFTER`; the second's value is open blocker 1. PATCH with `{"action": "rotate"}` is not idempotent: each call mints a token, and §5's overlap rule makes a retry safe.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Foreign runner at the claim | security | a scan bug, or a direct claim | no row; slot, fence and admission untouched | `claim_statement_refuses_a_foreign_runner` |
| Hole in the claim filter | security | a claim predicate regression | issue refuses under the host lock, no lease row, the counter rises | `issue_refuses_a_foreign_runner_under_the_host_lock` |
| Setup draft to a foreign host | security | authoring scan or claim | never offered, `admit` refuses | `authoring_placement_refuses_a_foreign_runner` |
| Platform key on a tenant host | security | the gate builds `direct` | `proxied` access; the key exists in no lease or answer | `platform_key_never_reaches_a_tenant_runner` |
| Tenant host under-reports tokens | security | a modified runner sends small lines | the daemon meters from the provider; runner lines skipped | `proxied_usage_is_metered_by_the_daemon` |
| Wrong or stale capability | auth failure, replay | a guessed token, a reclaimed lease | `409 UZ-RUN-005`, no provider call | `model_call_refuses_a_wrong_capability` |
| Model call on a direct or foreign lease | auth failure | a host probing other leases | `404 UZ-RUN-006`, no provider call | `model_call_refuses_a_direct_or_foreign_lease` |
| Provider down mid-turn | dependency unavailable | `503` or a timeout | `error` frame; known usage logged once; the turn fails | `proxied_call_reports_a_provider_failure` |
| Proxy stream cut | network blip | the connection drops before `end` | the runner's turn fails as a direct provider loss does | `proxied_provider_streams_daemon_chunks` |
| Reading another tenant's host | auth failure | a guessed `runner_id` | `404 UZ-RUN-014`, B's row unchanged | `tenant_lists_only_its_runners` |
| Enrolling without the scope | auth failure | a narrowed person or key | `403 UZ-AUTH-022`, no row | `enrolment_without_runner_own_is_refused` |
| Retried enrolment | replay | a lost `201` | `409 UZ-RUN-024`, one runner | `enrolment_retry_answers_conflict` |
| Unknown placement | malformed input | a typo or a stale client | `400 UZ-REQ-001`, row unchanged | `placement_setting_refuses_unknown_value` |
| Host never got the new token | auth failure | the overlap passed | `401 UZ-RUN-001`; one reload, then a halt | `rotation_keeps_the_old_token_for_the_overlap`, `runner_reloads_its_token_file_on_401` |
| Rotation retried | replay | a lost rotate answer | PATCH `rotate` is not idempotent (HTTP-PATCH), since each call mints a token; the host's token stays previous and the undelivered one dies | `rotation_inside_the_overlap_keeps_the_previous_token` |
| Live stream outlives the overlap | auth failure | a connection opened on the old token | closed within one re-check | `overlap_end_closes_a_stream_on_the_old_token` |
| Token file unreadable on SIGHUP | failure | a removed file or bad permissions | current token kept, failure logged | `sighup_swaps_the_token_without_a_restart` |
| Placement changed under a hold | race | `own_only` while a platform host holds | the holder is refused; an own runner claims after the hold | `placement_change_refuses_the_platform_holder` |
| No own runner online under `own_only` | capacity | every host offline | work waits; the page and `runner placement` say so | `cross_tenant_placement_matrix`, `own-only placement warns only with no runner online` |
| An owner flipped to platform | security | a statement writes the owner | the trigger raises; the row keeps its tenant | `runner_owner_cannot_change` |

## Invariants

1. No fleet or setup lease lands on a runner owned by another tenant. The claim statement's `permitted` step and the issue and `admit` re-check under `lock_host.sql`'s `FOR UPDATE` enforce it, against Postgres only; `cross_tenant_placement_matrix` is the counterexample.
2. A lease placed on a tenant-owned runner under platform posture carries no platform key. `ModelAccess::Proxied` has no key field, and the gate chooses it from the host-locked owner; `platform_key_never_reaches_a_tenant_runner`.
3. A runner's owner never changes after enrolment. The slot-2850 trigger refuses it; `runner_owner_cannot_change`.
4. At most two digests authenticate a runner, the older only until its expiry. The both-or-neither check and the lookup's `statement_timestamp()` predicate enforce it; `token_lookup_honours_the_overlap`.
5. Token charges on a proxied lease come only from attempts the daemon ran. Renew and report skip runner lines when `model_token_hash` is set; `proxied_usage_is_metered_by_the_daemon`.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `agentsfleet_placement_foreign_refusals_total` | operator | issue or `admit` refuses a foreign host | `lease_type` (`fleet`, `authoring`) | no tenant, runner or fleet identifier | `issue_refuses_a_foreign_runner_under_the_host_lock` |
| `agentsfleet_proxied_model_calls_total` | operator | a proxied turn ends | `outcome` (`ok`, `provider_error`, `refused`) | no identifier, no model text | `proxied_usage_is_metered_by_the_daemon` |
| `runner_token_reloaded`, `runner_token_reload_failed` (log events) | operator | the supervisor reads its token file | `source` (`sighup`, `unauthorized`) | never the token or its digest | `sighup_swaps_the_token_without_a_restart` |
| `runner_token_minted` (gains `owner`) | product | an enrolment succeeds | `runner_id`, `sandbox_tier`, `owner` | opaque identifier only | `runner settings actions capture owner and placement` |
| `runner_placement_changed` | product | a save changes the setting | `placement` | a fixed word only | `runner settings actions capture owner and placement` |

## Acceptance Rubric (single scoring surface)

R rows read the logs the S lanes write: `.tmp/rustd-integration.log` from S3 and `.tmp/rustd-unit-runner.log` from S2.

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A foreign runner is refused at scan, claim and issue, in every setting (§3) | `grep -c "cross_tenant_placement_matrix ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R2 | The claim statement alone refuses a foreign runner (§3) | `grep -c "claim_statement_refuses_a_foreign_runner ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R3 | Setup work never reaches a foreign runner (§3) | `grep -c "authoring_placement_refuses_a_foreign_runner ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R4 | The platform key never reaches a tenant runner (§4) | `grep -c "platform_key_never_reaches_a_tenant_runner ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R5 | The daemon, not the host, writes the proxied token bill (§4) | `grep -c "proxied_usage_is_metered_by_the_daemon ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R6 | A real tenant host completes a platform-posture chat (§4) | `grep -c "tenant_runner_completes_a_platform_posture_chat ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R7 | Rotation overlaps, then the old token gets 401 (§5) | `grep -c "rotation_keeps_the_old_token_for_the_overlap ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R8 | The runner reloads its token on a 401 without a restart (§5) | `grep -c "runner_reloads_its_token_file_on_401 ... ok" .tmp/rustd-unit-runner.log` | `1` | P0 | |
| R9 | A tenant lists and touches only its own runners (§2) | `grep -c "tenant_lists_only_its_runners ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
| R10 | The tenant surface is published (§7) | `grep -c "/v1/tenants/me/runner-placement" public/openapi.json` | `1` or more | P1 | |
| R11 | The new page renders and stays first-party (§6) | `make dry-app` | exit 0 | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `ExecutionPolicy`'s top-level `api_key`, `inference_host` and `base_url` | `git grep -n -e "policy\.api_key" -e "policy\.inference_host" -e "policy\.base_url" -- rustd` | no output |

Rotation's "the old token fails the next read" sentences in `AUTH.md:662` and `runner_fleet.md:251` are rewritten by §7, not swept.

Schema this milestone leaves with no runtime reader or writer is dropped in a new slot in this Pull Request, under Indy's blanket approval (Discovery). Each drop gets a row above, with a `git grep -n -w` over `rustd`, `ui` and `cli` proving nothing still reads or writes it.

## Out of Scope

- One-time enrolment codes and self-registration: excluded by decision; `runner_fleet.md:227` keeps mode C for later.
- The runner stream, pushed revocation and the version handshake: M243. Counting holds, auto-cordon and checkpoint respawn: M247.
- Tenant-visible runner events and lease history: the operator plane keeps them.
- Sharing one runner across tenants, and per-workspace runners: not planned.
- An `Idempotency-Key` on operator enrolment and API-key creation: those routes are unchanged here.
- Data residency: history, checkpoints and keys pass through `agentsfleet` whatever the setting (brief M12), and the docs page says so.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An owner runs `agentsfleet runner create --host-id build-box-1`, puts the token on their own machine, sends their fleet a message, and Settings → Runners shows that machine busy with it.
2. **Preserved user behaviour** — Fleets on platform hosts run as today; a tenant with no runners reads `own_or_platform` and sees no change; operators enrol and manage platform hosts as before, with an owner column added.
3. **Optimal-way check** — One mint and one placement predicate deliver the moment. The gap: existing accounts need `runner:own` provisioned, and platform-posture turns on a tenant host take one extra hop through the daemon.
4. **Rebuild-vs-iterate** — Iterate: the existing enrolment, runner plane, label placement and authoring capability are extended; no new service.
5. **What we build** — An owner column and its filter, the `runner:own` routes, the placement setting, the proxied model call, overlap rotation with file reload, the CLI `runner` group and a Settings → Runners page.
6. **What we do NOT build** — Enrolment codes (decided out); tenant lease history (operators only); cross-tenant sharing (breaks the boundary); per-runner rules beyond labels (labels suffice); capacity reporting (M247).
7. **Fit with existing features** — Compounds with M243's stream, labels and authoring's proxy. It must not destabilise the platform claim, the hottest statement, which gains one predicate.
8. **Surface order** — Both together: the token is minted by the same call either way, an operator installing a host works in a terminal, and the placement choice is a settings decision.
9. **Dashboard restraint** — No per-runner graphs and no tenant events tab. The one computed signal is the wait warning, shown only under `own_only` with zero of the tenant's runners online.
10. **Confused-user next step** — "My fleet doesn't start": `agentsfleet runner placement` prints the setting and "0 of 2 runners online; work waits until one connects". A refused host logs `runner_token_refused`, and the docs page "Run your own runner" covers the token file and SIGHUP.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** schema first, then the tenant routes that create owned hosts, then the filter that confines them, then the key boundary, then rotation, then the surfaces and the docs. Each Section is testable on the one before it.
- **Alternatives considered:** labels as the control, rejected because an untagged fleet matches any runner; the owner on the principal, rejected because the host-lock read is the authority; a byte-forwarding proxy, rejected because it needs a usage parser per format and forwards any body on the platform key; tenant keys only, which is open blocker 2; a tenant filter on the operator list, rejected because a missing tenant would list every host; rotating twice to kill a leaked token, rejected because a retried rotation would cut off a healthy host, so a leak is a revoke.
- **Patch-vs-refactor verdict:** a **patch** because it adds a column, a predicate and one route family to paths that keep their shape; the only reshaped type is `ExecutionPolicy`'s model access.

## Discovery (consult log)

- **Consults** — Source review on `main` `14539a73e`, Nov 06, 2026:
  - Placement checks only `required_tags <@ labels` (`select_ledger_candidates.sql`, the `eligible` step); the claim re-checks neither labels nor owner (`claim_admission_head.sql`); issue checks capacity and the ticket after `capacity::lock` (`afd_fleet/src/lease/issue.rs`); `lock_host.sql` already reads the runner row `FOR UPDATE`.
  - `runner.runners` has no tenant column (`schema/20261108123100_runner_runners.sql`); the token lookup returns id, status and degraded (`select_runner_token.sql`); `Principal::Runner` holds no tenant (`afd_auth/src/principal.rs`).
  - Rotation overwrites `token_hash` (`rotate_runner_token.sql`, `afd_runner/src/rotate.rs`); the runner sets its bearer once into default headers (`afr_supervisor/src/client/http.rs`).
  - The lease carries the resolved key (`afd_gate/src/policy/build/mod.rs`, `ExecutionPolicy.api_key` in `af_wire/src/policy/mod.rs`), and the runner dials with it (`af_providers/src/connect/mod.rs`, `turn.rs`). Authoring already keeps its key in the daemon behind a capability (`afd_authoring/src/model/capability.rs`, `AUTH.md:762`).
  - Rule interpretation: HTTP-IDEMKEY asks a replay to return the first response, and AUTH-SECRET forbids keeping the token that response revealed. The receipt answers a replay with `409`, which keeps the retry from enrolling twice; the operator enrolment and API-key create take no key today and stay as they are.
- **Owner decisions** —
  > Indy (Nov 06, 2026: 08:50 AM): "And we are not in production yet, so i want to rethink the approach on admissions, lease, and so on as i am focussed on scale. If something fails in one host bare metal, it should automatically cordon and respawn in another runner" — context: respawn across hosts, which the placement setting governs across ownership.
  > Indy (Nov 06, 2026: 10:19 AM): "This will be on by default durable_tasks_v1. ... my plan also was to have folks spin a runner and point to the agentsfleetd (in cloud) and keep using it." — context: tenant-enrolled runners against the cloud daemon.
  > Indy (Nov 06, 2026: 10:43 AM): "1. Yes placement rules are via the tags and labels 2. Respawn across ownership - this is a tenant setting ... Based on the scope i would like to build this ... 4. Yes, but donot overengineer now, we can improve this." — context: labels stay placement; the three-option setting; the smallest design.
  - Programme decisions recorded in the reviewed EGM brief (`concepts_brief.md`, Nov 06, 2026), not quoted verbatim: runners carry an owner tenant, NULL a platform host; the ownership filter sits in the claim and is re-checked at issue under the host lock; widening is one way, and setup placement gets the same filter; the setting has three options; platform keys never leave the daemon; rotation overlaps old and new digests and the runner reloads from a file on a 401 or SIGHUP; enrolment is the dashboard and CLI mint with a new tenant scope, and one-time codes are out of scope.
  > Indy (Nov 06, 2026: 04:18 PM): "yes" — context: blanket approval, asked as "Should each milestone drop the tables and columns it leaves with no reader or writer?": a milestone drops, in a new slot in its own Pull Request, any table, column or index it leaves with no runtime reader or writer (`.orly/standards/domains/sql.md` destructive-change approval).
- **Model reviews** — none yet. The EGM (Elephant-Goldfish Model) review runs before this spec leaves `pending/`.
- **Open blockers** —
  1. **The platform-fallback threshold for `own_then_platform`.** Question for Indy: how long does a tenant's work wait for its own runners before a platform host may take it, and is the trigger time or liveness? Options: a platform-wide wait in `af_core::timing` (☠️ my call: 30 s, one `LEASE_TTL`, so a dead own host is replaced as fast as a lapsed lease is today); fall back only when none of the tenant's runners is online, which needs a runner scan in the claim; or a per-tenant value, a column on `runner.tenant_placements`. Dimension 3.6 binds whichever he picks.
  2. **Proxied platform inference on tenant runners, or the tenant's own key only.** Question for Indy: is it acceptable that every platform-posture turn on a tenant runner goes through the daemon? The costs are a provider stream held per turn, one hop per model call, and a daemon crash mid-call losing that call's usage line (the platform pays the provider; the tenant is not charged). The alternative refuses platform posture on tenant runners: such fleets run only on platform hosts, `own_only` with platform posture never runs, and §4 shrinks to Dimensions 4.1 to 4.2 plus a placement refusal. ☠️ my call: proxy, because `own_only` then works for every fleet and the daemon already runs this path for authoring.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
