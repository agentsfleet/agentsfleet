<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M246_001: A tenant registers its own runner, which takes only that tenant's work and never a platform model key

**Prototype:** v2.0.0

**Milestone:** M246

**Workstream:** 001

**Date:** Nov 06, 2026 (rescoped Nov 17, 2026)

**Status:** DONE

**Priority:** P1, customers will point their own hosts at the cloud `agentsfleetd`. Today only an operator can register a host, and any registered host receives every tenant's work.

**Categories:** API, CLI, DOCS, OBS

**Batch:** B1, milestone 6 of the push-scheduler programme; ships as its own Pull Request because it moves a security boundary (SPEC-BOUNDARY)

**Depends on:** M243_001 (done): its runner stream authenticates once per connection, re-checks on every heartbeat and closes on revoke.

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 06, 2026) from the Elephant-Goldfish Model (EGM) brief and source research on main 14539a73e. Rescoped Nov 17, 2026 against main 97224ed74 at Indy's request, then revised after a four-reviewer EGM review.

**Canonical architecture:** `docs/AUTH.md` §"Runner token (`agt_r`): the machine principal" and `docs/architecture/runner_fleet.md` §"Registering a runner" own the trust model this changes. `docs/architecture/runner_protocol.md` owns the stream handshake this reuses unchanged.

---

## Overview

**Goal (testable):** A tenant-registered runner leases only its tenant's fleet and setup work, and never receives a platform model key.

**Problem:** Indy wants people to "spin a runner and point to the agentsfleetd (in cloud) and keep using it". Today a tenant cannot register that host, and the host would be unsafe if it were registered:
- Only platform operators can register. `POST /v1/admin/runners` requires `runner:register` (`afd_http/src/route/runner_ops.rs`), and `TENANT_OWNER_GRANT` lacks it (`afd_auth/src/scope.rs`).
- Nothing keeps another tenant's work off a host. Placement checks only `required_tags <@ labels` (`select_ledger_candidates.sql`), so an untagged fleet matches any runner. The claim and issue re-check neither labels nor owner. Authoring's `candidates.sql` takes no runner at all.
- Platform keys reach runners two ways. A fleet on the platform default model carries the key in its lease (`ExecutionPolicy.api_key`, `afd_gate/src/policy/build/mod.rs`). A runner assigned a platform decision model receives the platform decision key (`afd_fleet/src/task/decision/dispatch.rs`). A tenant holds root on its own host and can read both.

**Solution summary:** `runner.runners` gains `platform_owned`, stated at registration, and a `tenant_id` set exactly when it is false. Tenant owners register and manage their own runners under a new `runner:tenant` scope at `/v1/tenants/me/runners`, through the handlers the admin routes use. One fixed rule decides placement: a tenant runner takes only its tenant's work, and a platform runner takes any tenant's work. The claim statement applies the rule, and issue re-checks it under the host lock. A tenant runner never takes work that would carry a platform key, and a database check keeps a platform decision model off it.

**Design flow:**

```
owner ─ `agentsfleet runners register` ─► POST /v1/tenants/me/runners
   (runner:tenant, Idempotency-Key) ─► receipt + runners row (tenant_id = caller's) ─► agt_r once
host: AGENTSFLEET_RUNNER_TOKEN + stream URL + CA file ─► agentsfleet-runner ─► M243 stream (unchanged)
admission for fleet F of tenant T, offered to runner R:
   scan + claim ─ R.tenant NULL, or R.tenant = T and the revision that runs is not platform-default
   issue / authoring admit ─ lock_host(R, T) returns `serves` under FOR UPDATE ─► refuse, log
   issue ─ R tenant-owned and Billed.posture = platform ─► refuse; the slot's fence releases it
revoke: PATCH {"action":"revoke"} ─► status gate ─► M243 stream closes
```

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): tenant-owned runners behind an ownership filter
- **Intent (one sentence):** let a customer run fleets on their own machine without that machine ever seeing another customer's work or a platform model key.
- **Authoring handshake:** Indy asked to prioritise "tenant runs a runner" and to move the audit and the rest to a spec after M250. He chose to fold the scope-catalogue rewrite into this Pull Request. ASSUMPTIONS I'M MAKING:
  1. The tenant routes (§2) and the ownership filter (§3) stay. Without them no tenant can create a runner, and a tenant runner would receive every tenant's work.
  2. A tenant runner reaches the stream through a route its operator provides. The public listener route and its certificate authority (CA) choice are M251_001 §1.
  3. Only accounts a platform admin chooses receive `runner:tenant`, by an identity-provider edit (`docs/AUTH.md` §"Provisioning grants"); signup never seeds it.
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
| `docs/v2/active/M246_001_P1_API_CLI_DOCS_OBS_TENANT_OWNED_RUNNERS.md` | EDIT, then MOVE to `done/` | this spec |
| `docs/v2/pending/M251_001_P1_API_CLI_DOCS_INFRA_OBS_UI_TENANT_RUNNERS_CONNECT_ROTATE_PROXY.md` | CREATE | the Sections this rescope moved out |
| `schema/20261108123100_runner_runners.sql` | EDIT | §1: `tenant_id`, its index, the trigger that fixes it, and the tenant-or-decision-model check, in the table's own file. Dev's database is reset when this Pull Request deploys (Release order), so editing an applied file is safe this once |
| Every `schema/*.sql` slot | EDIT | comments normalized and reduced, as Indy chose; no statement changes, proved by comparing each file with its comments stripped. Unused columns and index findings are applied only where Discovery records them |
| `schema/<UTC>_runner_registration_receipts.sql` | CREATE | §1: the registration retry receipt (SQL-RETRYKEY) |
| `rustd/crates/daemon/afd_db/src/migration.rs` | EDIT | §1: register the receipt slot (MIG); `afd_db/tests/migrations.rs` proves every file is listed in order |
| `rustd/crates/daemon/afd_fleet/tests/integration_runner_row.rs`, `fleet_suite.rs`, `support/fleet_tenant_runners.rs` (new) | CREATE, EDIT | §1: the tenant, check and erasure tests, and the tenant-runner row helpers §3 and §4 reuse |
| `rustd/crates/daemon/afd_auth/src/scope.rs`, `afd_auth/src/credential.rs` (its `KNOWN_BITS` mention), `afd_auth/tests/scope_catalogue.rs` | EDIT | §2: the `scopes!` catalogue, `Scope::RunnerTenant`, outside `TENANT_OWNER_GRANT`, which stays at 10 |
| `rustd/crates/foundation/af_core/src/error_code/fleet.rs`, `error_code/mod.rs`, `problem/fleet.rs` | EDIT | §2, §3: `UZ-RUN-025`, `UZ-RUN-026`, `UZ-RUN-027` |
| `rustd/crates/daemon/afd_runner/src/scope.rs`, `lib.rs`, `error/mod.rs`, `error/raise.rs` | CREATE, EDIT | §2: `RunnerScope`, `Owner` and their binds, and the refusal kinds behind `UZ-RUN-025`, `UZ-RUN-026` and the decision-model and durable-task refusals |
| `rustd/crates/daemon/afd_runner/src/store.rs`, `admin.rs`, `edit.rs`, `rotate.rs`, `view/store.rs`, `view/decode.rs` | EDIT | §2: each management call is a `RunnerScope` method; views carry `tenant_id` |
| `rustd/crates/daemon/afd_runner/src/register.rs` (new), `sql/runner/insert_runner_with_event.sql`, `insert_registration_receipt.sql` (new), `select_registration_receipt.sql` (new), `select_runner_tenant.sql` (new), `rotate_runner_token.sql`, `sql/runner/mod.rs` | CREATE, EDIT | §2: the shared registration, the receipt and its replay read, rotation by reach |
| `rustd/crates/daemon/afd_runner/src/sql/runner_view/list_runners_first.sql`, `list_runners_after.sql`, `count_runners.sql`, `runner_detail.sql`, `view/store.rs`, `view/decode.rs`, `view/mod.rs`, `sql/runner_admin/transition_runner_status.sql`, `delete_runner_if_in_state.sql`, `patch_runner_selftest_request.sql`, `select_runner_patch_state.sql` | EDIT | §2: each statement binds the reach's tenant; views return `tenant_id` |
| `rustd/crates/daemon/afd_runner/src/capacity/mod.rs`, `capacity/choice.rs`, `capacity/sql/lock_host.sql` | EDIT | §3: the host lock binds the work's tenant and returns `serves` |
| `rustd/crates/daemon/afd_http/src/route/runner_ops.rs`, `afd_http/src/handler/reach.rs` (new), `afd_http/src/handler/mod.rs` | CREATE, EDIT | §2: tenant route identities and the tenant lookup that builds a `Reach` |
| `rustd/crates/daemon/afd_api_operator/src/handler/operator/runners.rs`, `runner_patch.rs`, `runner_delete.rs`, `runner_register.rs` (moved from `afd_api_runner`), `mod.rs`, `afd_api_operator/src/lib.rs`, `openapi.rs` | CREATE, EDIT | §2: one handler set for both reaches |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/enrolment.rs`, `afd_api_runner/src/handler/runner/mod.rs`, `lib.rs`, `openapi.rs` | DELETE, EDIT | §2: registration moves to its sibling handlers |
| `rustd/crates/daemon/afd_api/src/router/mount.rs` | EDIT | §2: drop the registration special case |
| `rustd/crates/daemon/afd_api_wire/src/operator.rs`, `rustd/crates/protocol/af_wire/src/runner/mod.rs` | EDIT | §2: `tenant_id` on views; the token-free replay answer |
| `rustd/crates/daemon/afd_api/tests/admin_operator_route_inventory.rs`, `route_meta_total.rs`, `route_meta_scopes.rs`, `operator_plane_suite.rs` and its new `tenant_runners` module | CREATE, EDIT | §2: inventory, counts and route tests; `autotests = false`, so the suite file registers the module |
| Every caller of `Runners::register`, `list_runners`, `runner_detail`, `transition`, `edit`, `rotate_token`, `request_selftest` and `delete_revoked`: the `afd_fleet`, `afd_runner`, `afd_gateway` and `main` test fixtures and `afd_bench/src/lane/lease/seed/mod.rs` | EDIT | §2: pass `&Reach::Platform` (NLG) |
| `rustd/crates/daemon/afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql`, `claim_admission_head.sql`, `lease/assign/ledger.rs`, `lease/issue.rs`, `lease/store.rs`, `lease/renew/extend.rs` | EDIT | §3, §4: the predicate, its binds, the issue re-checks |
| Every test and bench that binds the claim or scan statement positionally: `afd_fleet/tests/integration_lease_plan/`, `integration_ledger_queue/deadlines.rs`, `integration_claim_delivery.rs`, `integration_tasks/decisions/configured/support.rs`, `afd_bench/src/statements/tests.rs`, `afd_bench/src/lane/cardinality/probe.rs` | EDIT | §3: the new binds |
| `rustd/crates/daemon/afd_authoring/src/lease/claim/sql/candidates.sql`, `claim/mod.rs`, `claim/selection.rs`, `afd_authoring/src/draft/mod.rs`, `afd_authoring/tests/draft/admission/discovery.rs` | EDIT | §3: the same filter for setup work |
| `rustd/crates/daemon/afd_fleet/tests/integration_runner_tenant.rs`, `integration_runner_tenant/`, `fleet_suite.rs` | CREATE, EDIT | §3, §4: matrix, claim, issue, key, registered in the suite |
| `rustd/crates/daemon/afd_db/src/test_util/runner/mod.rs`, `afd_db/src/test_util/fleet/mod.rs` | EDIT | §3, §4: `RunnerSeed` takes a tenant; a fleet seed on a tenant's own model |
| `rustd/crates/daemon/afd_authoring/tests/draft/admission/tenant.rs`, `draft/admission/mod.rs`, `authoring_suite.rs` | CREATE, EDIT | §3: setup work placement, registered in the suite |
| `rustd/crates/daemon/main/tests/integration_rust_runner_tenant.rs`, `main/tests/daemon_suite.rs` | CREATE, EDIT | §4: the end-to-end run |
| `cli/src/commands/platform/runner-kind.ts` | CREATE | §5: `RunnerKind` and its two values |
| `cli/src/commands/platform/runners.ts`, `common.ts`, `cli/src/program/tree/platform-runners.command.ts`, `platform.command.ts`, `index.ts`, `platform-flags.ts`, `cli/src/program/entry/discovery.ts` | EDIT | §5: one command factory; the tenant group and its scope discovery |
| `cli/src/commands/platform/runner-schema.ts`, `cli/test/helpers-platform.ts` | EDIT | §5: the token-free replay answer and the tenant path |
| `cli/test/runners.unit.test.ts` (new), `cli/test/platform-runners.unit.test.ts`, `cli/test/scoped-discovery.unit.test.ts`, `cli/test/runners.subprocess.test.ts` (new) | CREATE, EDIT | §5 |
| `docs/AUTH.md`, `docs/architecture/runner_fleet.md`, `postgres_schemas.md`, `runner_protocol.md`, `push_scheduler.md` | EDIT | §6; `push_scheduler.md` drops its promise of the placement setting under M246 |
| `docs/v2/pending/M247_001_P1_API_CLI_DOCS_OBS_UI_FAILED_HOSTS_RESPAWN_FROM_CHECKPOINT.md` | EDIT | §6: its dependency line names M251_001 for the placement setting |
| `public/openapi.json` | EDIT (regenerated with `--no-banner`) | §6 |
| `~/Projects/docs`, branch `chore/m246-tenant-runners-changelog` (docs#253) | EDIT | §6: the changelog entry; API pages after merge. The "Run your own runner" page waits for M251_001's public route |
| The `enroll` → `register` rename: `afd_auth/src/scope.rs`, `afd_auth/tests/scope_catalogue.rs`, `afd_http/src/route/runner_ops.rs`, `afd_api/tests/operator_plane_runner_edit.rs`, `afd_api/tests/runner_plane/authorization.rs`, `afd_api/tests/runner_plane/mod.rs`, `afd_api_runner/src/handler/runner/mod.rs`, `cli/src/program/tree/platform-flags.ts`, `platform-runners.command.ts`, `cli/src/commands/platform/runners.ts`, `cli/test/acceptance/platform-operator-live.spec.ts`, `cli/test/scoped-discovery.unit.test.ts`, `ui/packages/app/lib/auth/platform.ts`, `platform.test.ts`, `scopes.ts`, `deploy/local/bootstrap.ts`, `docs/AUTH.md`, `docs/architecture/runner_fleet.md`, `playbooks/README.md`, `playbooks/founding/02_preflight/001_playbook.md`, `playbooks/operations/admin_bootstrap/001_playbook.md`, `playbooks/operations/runner_onboarding/001_playbook.md`, `public/openapi.json` | EDIT | §2, §5: one word for creating a runner, as the API has said since May 2026: CLI `register`, scope `runner:register`. Production files first, tests after (SPEC-BLAST); finished specs keep their wording as records |
| `.orly/**`, `.claude/skills/orly-*/`, `.claude/agents/orly-standards-reviewer.md`, `.agents/skills/orly-*/`, `.opencode/skills/orly-*/` | EDIT, DELETE | the managed rules re-materialised by `orly update --no-hooks` at orly 0.50.0, at Indy's request; no `orly_version` pin |
| `.orly/orly.json`, `.orly/AGENTS.md`, `.claude/skills/orly-burst/SKILL.md`, `.agents/skills/orly-burst/SKILL.md`, `.opencode/skills/orly-burst/SKILL.md` | EDIT | the integration lane on burst machines: `burst.integration`, placement in `burst.e2e` and `burst.aws`, the test-binary warm-up, and orly 0.60.0, which reads that placement |
| `rustd/crates/daemon/afd_fleet/src/lease/waiting.rs` (new), `afd_fleet/src/lease/sql/lease/select_waiting_work.sql` (moved from `afd_gateway/src/sql/select_waiting_admission.sql`), `afd_fleet/src/lease/sql/queue.rs`, `afd_fleet/src/lease/mod.rs` | CREATE, MOVE, EDIT | §3.5: one statement answers whether work waits and names the connected tenant runners whose tenant has none |
| `rustd/crates/daemon/afd_gateway/src/scheduler/mod.rs`, `scheduler/recovery/mod.rs`, `registry/mod.rs`, `scheduler/tests.rs`, `scheduler/recovery/tests.rs`, `registry/tests.rs` | EDIT | §3.5: the offer pass skips the runners the probe names idle; a recovery pass skips no one |
| `rustd/crates/daemon/afd_fleet/tests/integration_tenant_placement/waiting.rs` (new), `integration_tenant_placement/mod.rs` | CREATE, EDIT | §3.5 on live Postgres |
| `schema/20261108121900_fleet_fleets.sql`, `schema/20261108123600_authoring_drafts.sql` | EDIT | `(tenant_id)` indexes the §3.5 probe reads: 13.65 ms → 1.59 ms over 20,915 fleets and 50,002 drafts |
| `schema/20261108123900_authoring_attempts.sql`, `schema/20261108131200_fleet_task_objects.sql` | EDIT | drop `idx_attempts_user_id` and `idx_task_objects_upload_owner`, which no statement reads |
| `schema/20261108130800_fleet_task_checkpoints.sql`, `schema/20261108120800_identity_cli_credentials.sql` | EDIT | index the foreign keys a fleet, tenant or user delete checks: `accepted_revision_id`, `user_id`, `tenant_id` |
| `docs/architecture/scaling.md` | EDIT | the probe's new home and the idle skip |
| `.github/workflows/build-candidate.yml`, `scripts/toolbox/build.sh` | EDIT | build the toolbox on Ubuntu 24.04, and refuse an image `fsck.erofs` rejects: erofs-utils 1.9 on Ubuntu 26.04 writes a corrupt one and exits 0 |
| `cli/src/commands/platform/runner-schema.ts`, `cli/src/commands/platform/runners.ts`, `cli/test/platform-runners.unit.test.ts`, `cli/test/platform-output.unit.test.ts`, `cli/test/runners.unit.test.ts` | EDIT | an operator's list and detail keep each runner's `tenant_id` and name its owner |
| `rustd/crates/daemon/afd_api_wire/src/admin/mod.rs`, `afd_api_operator/src/handler/operator/runner_patch.rs`, `tenant_runners.rs`, `public/openapi.json` | EDIT | both runner PATCH routes publish their two `200` shapes as `RunnerPatchReply` (`oneOf`); the document is regenerated |
| `rustd/crates/daemon/afd_runner/tests/integration_runner_plane/operator.rs` | EDIT | the runner-count race test parks the page between its total and its rows, so it fails without the snapshot |
| `rustd/crates/daemon/afd_api/tests/tenant_runner_refusals.rs` (new), `afd_api/tests/tenant_plane_suite.rs` | CREATE, EDIT | every tenant runner route refuses when the caller's tenant cannot be read |
| `rustd/crates/daemon/afd_runner/src/store.rs`, `afd_runner/src/register.rs`, `afd_runner/src/sql/runner/mod.rs`, `afd_runner/src/sql/runner/insert_runner_with_event.sql`, `afd_api_operator/src/handler/operator/runner_register.rs`, `tenant_runners.rs`, `afd_bench/src/lane/lease/seed/mod.rs`, and every registration call in the `afd_runner`, `afd_fleet`, `afd_api`, `afd_gateway` and `agentsfleetd` tests | EDIT | §2.10: both registration paths take the caller's subject and record it as the event's `actor_id` |
| `docker-compose.yml` | EDIT | the local test Postgres takes 300 connections: the integration lane refused clients at the default 100 (`sorry, too many clients already`) |
| `rustd/crates/runner/afr_executor/tests/terminal.rs` | EDIT | the terminal test prints the terminal's name through `printf`: uutils' `tty` on Ubuntu 26.04 prints no newline onto a terminal |
| `deploy/baremetal/service_test.sh` | EDIT | the unit check drops lines naming the host's own unit files; Ubuntu 26.04's `xfs_scrub_all.service` warns about itself, and every other line still fails |
| `scripts/local-dev-state.sh` | EDIT | the dev launcher's lock is a noclobber file: uutils' `mkdir` on Ubuntu 26.04 lets two racers both take a directory lock |
| `scripts/local-dev-test.sh` | EDIT | the held-lock test plants the lock as a file, as the launcher now takes it |
| `.github/actions/build-toolbox/action.yml` | EDIT | the scan step names each variable it passes through `sudo`: `sudo-rs` on Ubuntu 26.04 ignores `-E` |
| `deploy/baremetal/agentsfleet-runner.service` | EDIT | drops `CPUAccounting=yes`, which current systemd ignores and `systemd-analyze verify` now warns about, failing `safety-gates` on the CI image; `Delegate=cpu` already gives the runner the controller |
| `rustd/crates/daemon/afd_runner/src/view/store.rs`, `src/sql/runner_view/mod.rs`, `src/sql/runner_view/read_snapshot.sql` (new), `tests/integration_runner_plane/operator.rs` | EDIT, CREATE | a runner page and an event page read their total and their rows in one read-only snapshot; the race is on `main`, and this branch's concurrent tenant registrations surface it |

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
| `make lint-all` (clippy `-D warnings`, `cargo shear`, TypeScript lint) | yes, the daemon crates in Files Changed and the CLI | `lint-rustd` and the CLI lint per Section |
| Route inventory, `route_meta_total` and `route_meta_scopes` | yes, two templates, five operations | inventory, counts and scopes move in §2's commit |
| Error registry and problem parity | yes, three codes | const, registry row and problem entry in one commit |
| `docs.language` at `orly gate verify` and `orly gate pr` | yes, `AUTH.md` and four architecture pages | sentences of at most 25 words, paragraphs of at most three |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, the operator handlers and the claim binds | registration gets its own file; `scope.rs` stays separate |

## Prior-Art / Reference Implementations

- **Reference:** GitLab's runner scopes (https://docs.gitlab.com/ci/runners/runners_scope/). Project runners serve only their project, and instance runners serve everyone: the one-way widening here.
- **Reference:** `afd_api_runner/src/handler/runner/enrolment.rs` and `afd_runner::Runners::register`: the mint, the digest-only row and the reveal-once response. Divergence: a tenant, and an `Idempotency-Key` receipt.
- **Reference:** `schema/20261108124300_authoring_replay_receipts.sql`: a receipt keyed by an operation identifier, holding a request digest, with no foreign keys and an append-only trigger. The registration receipt mirrors it.
- **Reference:** `drafts.tenant_model_id` (`docs/architecture/postgres_schemas.md` §"Model pointers"): a nullable tenant pointer where NULL means the platform.
- **Reference:** `cli/src/commands/platform/runners.ts`. The tenant group follows the six CLI practices this code already keeps: command, handler and error layers apart; handlers that neither print nor exit; one rendering layer; structured JSON errors with suggestion and retry fields; handler, in-process and subprocess tests; JSON by default when stdout is not a terminal.

## Sections (implementation slices)

### §1 — The runner row names its tenant

`runner.runners` gains `platform_owned BOOLEAN NOT NULL` and `tenant_id`, referencing `identity.tenants(id) ON DELETE CASCADE`, with a partial index where it is set. `ck_runners_owner` requires `platform_owned = (tenant_id IS NULL)`, so every row is a platform runner or one tenant's, and an insert must state which. A missing tenant never stands for the platform: the platform runner takes every tenant's work, so a bug that dropped a tenant would otherwise fail open. Every reader tests the flag (`platform_owned OR tenant_id = …`), so a row that is neither matches nothing. `tenant_id` names the tenant everywhere else in the schema (SQL-FKNAME). A trigger refuses any change to either column after insert. A check refuses a tenant runner with a decision model: `platform_owned OR decision_platform_model_id IS NULL`.

`runner.registration_receipts` mirrors `authoring.replay_receipts`: a UUIDv7 `id`, `tenant_id`, `operation_id`, `request_digest`, `runner_id` and `created_at`. It is unique on `(tenant_id, operation_id)`, has no foreign keys and refuses updates. `api_runtime` gets `SELECT` and `INSERT` (SGR). `operation_id` is the request's `Idempotency-Key`. `request_digest` is the SHA-256 of the request re-serialised from its parsed form, so whitespace never changes it. The receipt outlives its runner, so a retried key never registers twice.

The deletion matrix gains the tenant's runners under Tenant, with their leases, history and counters. Receipts stay, holding opaque identifiers only, as other receipts do. **Implementation default:** the tenant lives on the row and rides the host lock, never on `Principal::Runner`. The lock read is the authority at placement, and a principal copy would be a staler second one. **Implementation default:** a trigger keeps the tenant fixed. A tenant runner flipped to NULL would become a platform runner and receive every tenant's work.

- **Dimension 1.1 — DONE** — the tenant cannot change → Test `runner_tenant_cannot_change` (integration): setting a tenant runner's `tenant_id` to NULL, a tenant runner's to another tenant, and a platform runner's to a tenant each raise the trigger's refusal, and the rows keep their tenants. Renaming a platform runner's host succeeds, so a trigger that refuses every update fails.
- **Dimension 1.2 — DONE** — a tenant runner holds no decision model → Test `tenant_runner_refuses_a_decision_model` (integration): setting `decision_platform_model_id` on a tenant runner violates the check; on a platform runner it succeeds.
- **Dimension 1.3 — DONE** — erasure takes only the tenant's runners → Test `tenant_erasure_takes_its_runners` (integration): deleting tenant A removes A's two runners and their leases. A platform runner, tenant B's runner and A's receipts remain.

### §2 — Tenants register and manage their own runners

**The scope catalogue is one list.** A `scopes!` `macro_rules!` takes each variant, its doc comment and its claim string once. It generates the enum, `Scope::ALL` and the `const fn wire()`, so adding a scope is one line. `ScopeSet` stays a `u32`, and a compile-time assertion bounds `Scope::ALL.len()` by 32; the catalogue holds 30. `KNOWN_BITS` goes, because a list generated from one input cannot miss a variant. `HIERARCHY` stays hand-written. Every claim string stays byte-identical, because the identity provider matches them verbatim. **Implementation default:** `macro_rules!`, not `strum`. `strum`'s conversions are not `const`, and a procedural-macro crate is a build unit every consumer waits on (`af_core/src/error_shell.rs`).

`Scope::RunnerTenant` (`runner:tenant`) joins the catalogue but not `TENANT_OWNER_GRANT`, so signup never grants it and a regular user does not hold it. A platform admin grants it per account through the identity provider, as Indy ruled. It covers registration, reading and changes together. Platform registration is its own scope because that host receives every tenant's work (`runner_ops.rs`); a tenant runner receives only its own tenant's.

**One handler set, two scopes.** `Runners::platform()` and `Runners::tenant(&tenant_id)` are the only ways to get a `RunnerScope`, the runner store narrowed to one caller's runners. `list_runners`, `runner_detail`, `transition`, `edit`, `rotate_token`, `request_selftest` and `delete_revoked` are its methods. The ownership it binds is a crate-private `Owner`, so no caller names the platform by leaving a tenant out.
- Each runner statement binds `($platform OR tenant_id = $tenant)`: true and no tenant for the platform, false and its id for a tenant, so a missing tenant matches no row. Ownership is checked inside the statement that acts, so there is no window between a proof and the change, and a runner outside the scope answers `404 UZ-RUN-014` (AWO).
- `list_runners` and its count bind the same tenant, so `total` counts only that tenant's runners.
- On a tenant runner, `Platform` may do everything except rotate its token, which answers `409 UZ-RUN-026`: a new token would hand an operator a working credential for the tenant's host. Rotation binds `(($platform AND platform_owned) OR tenant_id = $tenant)`, and a refused platform rotation reads `platform_owned` once to tell `409` from `404`.
- `register` stays the platform entry with its signature; `register_tenant` adds the tenant and the receipt. Both insert one prepared registration (`afd_runner/src/register.rs`).
- `register` and `edit` refuse `assigned_policy.decision_model_id` for any tenant runner, from either reach, with `400 UZ-REQ-001`. Durable tasks stay off on tenant runners in M246: a tenant registration stores `durable_tasks_enabled` false whatever the body says, and the tenant reach refuses turning it on with `400 UZ-REQ-001`. Only the platform reach may change that one field on a tenant runner, as Indy ruled; no M246 path turns it on, because a task's decisions need a platform decision model a tenant runner may not hold.
- A tenant route whose caller resolves to no tenant answers `403`, as the api-key routes do (`afd_http::handler::refusal::Refusal::forbidden`).

Registration moves from `afd_api_runner` into `afd_api_operator`, beside the list, detail, patch and delete handlers. A thin handler per route builds its `RunnerScope` and passes it to one shared core function per verb. Each admin handler passes `runners().platform()`. Each tenant handler first calls `afd_http::handler::reach::tenant`, which resolves the caller's tenant through the shared `tenant_of` lookup, then calls the same core function with `runners().tenant(&tenant)`. The tenant route identities join `RunnerOpsRoute`, so one route family maps to one handler crate. Events and leases stay admin-only.

**Registration and its retry.** A tenant registration requires a UUIDv7 `Idempotency-Key`, read by `afd_http::handler::setup_input::operation`. In one transaction, the receipt insert runs first with `ON CONFLICT DO NOTHING`, then the runner and its event. When no receipt was inserted, a second statement reads the stored receipt. A matching digest answers `200` with the original `runner_id` and `assigned_policy` and no token. A different digest answers `409 UZ-RUN-025`. A concurrent duplicate waits on the unique index, inserts nothing, and then reads the committed receipt.

**Implementation default:** composition by value, not a trait with platform and tenant implementations. The two reaches differ in one value and one statement choice, not in behaviour. **Owner ruling (Indy, Nov 17, 2026):** the replay answers `200` without the token. AUTH-SECRET forbids keeping the token, so HTTP-IDEMKEY's replay returns everything else, and the CLI names the runner to rotate. **Implementation default:** separate tenant routes, not a tenant filter on `/v1/admin/runners`, because a nullable filter turns a missing tenant into a full listing.

- **Dimension 2.1 — DONE** — an owner registers a host → Test `tenant_registers_a_tenant_runner` (integration): a session holding `runner:tenant` posts a valid body → `201`, an `agt_r` once, `Cache-Control: no-store`, and the row's `tenant_id` is the caller's tenant. A platform registration's row has a NULL `tenant_id`, so a test that never sets it fails.
- **Dimension 2.2 — DONE** — registration needs the scope → Test `registration_without_runner_tenant_is_refused` (integration): the default owner grant, which lacks `runner:tenant`, → `403 UZ-AUTH-022` and no row; an `agt_r` bearer → `401`, and no row.
- **Dimension 2.3 — DONE** — a retry registers nothing twice → Test `registration_retry_replays_without_the_token` (integration): the same key and body twice → the second answers `200` with the same `runner_id` and no `runner_token`, one row. The same key with other labels → `409 UZ-RUN-025`, still one row. No key, or a key that is not a UUIDv7 → `400 UZ-REQ-001`. A body naming `decision_model_id` → `400 UZ-REQ-001`, no row.
- **Dimension 2.4 — DONE** — concurrent retries register once → Test `concurrent_registrations_with_one_key_leave_one_runner` (integration): 100 tasks post the same key and body behind a barrier (TEST-CONTEND) → exactly one `201`, 99 `200` with the same `runner_id`, and one row.
- **Dimension 2.5 — DONE** — a tenant manages only its own → Tests `tenant_lists_only_its_runners`, `tenant_manages_its_own_runner` and `tenant_never_reaches_another_tenants_runner` (integration): A owns two, B one, the platform one. A's list returns exactly A's two ids with `total: 2`, and `?limit=1` pages them. A's GET, cordon, labels edit, policy edit, self-test, rotate and DELETE on its own runner succeed. A second rotate kills the token the first returned (`401` on `GET /v1/runners/me`). The same calls on B's runner each answer `404 UZ-RUN-014`, and B's row and token digest are unchanged.
- **Dimension 2.6 — DONE** — an operator cannot take over a tenant's host → Test `platform_reach_cannot_rotate_or_edit_a_tenant_runner` (integration): an operator's rotate → `409 UZ-RUN-026`, and an operator's decision-model edit → `400 UZ-REQ-001`. The operator's labels edit and durable-tasks switch succeed. A's own PATCH setting a decision model, or `durable_tasks_enabled: true` → `400 UZ-REQ-001`. A's registration asking for durable tasks stores them off. The operator's PATCH setting `durable_tasks_enabled` on A's runner succeeds. The row and token digest are unchanged, and the operator's cordon on the same runner succeeds.
- **Dimension 2.7 — DONE** — a tenant revoke ends the host's stream → Test `tenant_revoke_closes_its_runner_stream` (integration): A revokes its runner → the M243 stream closes, and a reconnect answers `401 UZ-RUN-009`. Erasing tenant C while C's runner is connected closes C's stream too, and its reconnect answers `401 UZ-RUN-001`.
- **Dimension 2.8 — DONE** — operators see who owns a host → Test `operator_list_names_each_tenant` (integration): the platform row carries no `tenant_id` key (HTTP-NULL); the tenant row reads its tenant.
- **Dimension 2.9 — DONE** — every claim string survives the catalogue rewrite → Test `scope_claim_strings_are_unchanged` (unit, `afd_auth/tests/scope_catalogue.rs`): the 30 `wire()` values equal a pinned list byte for byte, in catalogue order, with `runner:register` replacing `runner:enroll` and `runner:tenant` added; `from_wire` round-trips every entry, and `runner:enroll` grants nothing. `runner_tenant_is_not_part_of_the_signup_grant` proves neither `TENANT_OWNER_GRANT` nor `signup_owner_claim()` carries `runner:tenant`.
- **Dimension 2.10 — DONE** — the registration event names who registered the runner → Test `a_registration_event_names_who_registered_the_runner` (integration, `afd_runner`): a platform and a tenant registration each record the caller's subject as `actor_id` on the runner's first event, as admin transitions and token rotations already do.

### §3 — The ownership filter in the claim, at issue and for setup work

One fixed rule: a tenant runner takes only its tenant's work, and a platform runner takes any tenant's work. Work is fleet work and setup work; setup work is authoring, the model-assisted drafting of a fleet's definition. The authority is `runner.runners.tenant_id`, read only from Postgres, so every replica decides alike. Three places apply it:
- The candidate scan filters on the runner's tenant and the fleet's tenant. It is a performance filter only.
- The claim statement's `fleet` step joins the runner, and keeps the fleet only when the runner is `platform_owned` or its tenant matches. The slot upsert already requires that row. Called with a foreign fleet, the claim returns no row and leaves the slot and its fencing token untouched. This is the control.
- `lock_host.sql` binds the work's tenant and returns two booleans under `FOR UPDATE`: `serves` (`platform_owned OR tenant_id IS NOT DISTINCT FROM $4`) and `tenant_owned`. `Held` carries both and `Capacity` carries `serves`, and both stay `Copy`; no tenant identifier is compared in Rust. `Leases::issue_for_revision`, the production issue path, refuses a host that does not serve the work before any write. So does `Drafts::admit`, right after `lock_host_capacity` and before `selection::authorize`, so no attempt row or message failure is written. Each logs `runner_placement_refused` at `error`, with `error_code` `UZ-RUN-027` and `reason` `foreign_tenant` (LOG-CODE). Any occurrence means the claim has a hole. Renewal and every other lock that places no work bind no tenant; a tenant host then reads `serves` false, and no such caller reads it. A stream call's credential proof is a bound flag, so a stream call without its proof is refused rather than trusted.

`candidates.sql` gains a runner bind and the same predicate over `authoring.drafts.tenant_id`. A refused `Drafts::admit` answers `Placement::Skipped`, so the walk moves to the next draft. The gateway's offer walk stays tenant-blind: it may wake a tenant runner for work that runner cannot claim, which costs one bounded claim pass and never a lease.

**Release order.** Dev's database is torn down before this Pull Request's dev deploy (`playbooks/operations/teardown/`, `ENV=dev`, Indy authorizing), because the edited schema files no longer match the checksums dev recorded; dev data is lost, as Indy accepted. The schema and the daemon then ship in one rolling deploy, and the migration runs first (`deploy/fly/agentsfleetd-prod/fly.toml`). Until every machine runs this build, an old machine's claim has no tenant predicate. So no tenant registers a runner before the rollout completes, and a rollback below this build first revokes every tenant runner. Pre-launch, no tenant runner exists when this ships. **Implementation default:** no preference for a tenant's own runners over platform runners, because labels and stickiness already order the scan. A tenant's choice of placement is M251_001.

- **Dimension 3.1 — DONE** — the live matrix matches the rule → Test `cross_tenant_placement_matrix` (integration): tenant T has three fleets on T's own model, each with one admission, and every runner carries the fleets' tag. T's runner, a platform runner and tenant U's runner each drive one fleet through the scan, the claim and issue. T's and the platform runner each lease their fleet. U's yields no candidate, no slot write and no lease row. A filter that refuses everything fails the test.
- **Dimension 3.2 — DONE** — the claim alone refuses → Test `claim_statement_refuses_a_foreign_runner` (integration): `claim_admission_head.sql` bound directly with T's fleet and U's runner → no row. The slot's fencing token and expiry and the admission's `lease_fence` are unchanged. The same bind with T's runner returns a row.
- **Dimension 3.3 — DONE** — issue refuses under the host lock → Test `issue_refuses_a_foreign_runner_under_the_host_lock` (integration): an `Acquired` for T's fleet handed straight to `Leases::issue_for_revision` with U's runner → `None`, no `runner.leases` row, and one `runner_placement_refused` line carrying `UZ-RUN-027`, `reason="foreign_tenant"` and `lease_type="fleet"`.
- **Dimension 3.4 — DONE** — setup work is filtered too → Test `authoring_placement_refuses_a_foreign_runner` (integration): `candidates.sql` never offers T's queued draft to U's runner. `claim_queued` with U's runner returns `None` and logs `lease_type="authoring"`; T's message stays queued, and no `authoring.attempts` row names U's runner. T's runner then claims the draft.
- **Dimension 3.5 — DONE** — a wake asks no tenant runner that has nothing to take → Tests `a_pass_skips_the_runner_the_probe_names_idle`, `a_wake_asks_the_one_runner_with_work_and_a_recovery_pass_asks_all_fifty` and `candidates_leave_out_the_idle_runners` (unit, `afd_gateway`): one probe per wake names every connected runner; of fifty sessions, the forty-nine the probe names idle are asked nothing, and a recovery pass asks all fifty without probing. Tests `the_probe_names_only_the_tenant_runners_with_nothing_waiting` and `the_probe_without_runners_names_none_idle` (integration, `afd_fleet`): over live Postgres the statement names the quiet tenant's runner and neither the busy tenant's nor a platform runner.

### §4 — No platform key reaches a tenant runner

A tenant runner never takes work whose revision runs on the platform default model. The revision that runs is the task's accepted revision, or else the fleet's head (`claim_admission_head.sql`'s `installed` step). The scan's head step and the claim's `head` step both read that revision. For a tenant runner they require its `runtime_model_ref` to be set and not `afd_credential::provider::PLATFORM_MODEL_REF`, bound by position per NSQ. The test applies to the admission at the head of the fleet's queue, after `ORDER BY a.sequence LIMIT 1`. A tenant runner that may not take the head takes nothing from that fleet, so it never runs a later admission ahead of it. Such work stays for platform runners.

`Leases::issue_for_revision` re-checks under the host lock: a `tenant_owned` host with `Billed.posture` of `platform` returns `None` and logs `runner_placement_refused` with `reason` `platform_model`. The claim's fence then releases the slot, as a lost claim does. The decision key path is closed by §1's check: a tenant runner never has a decision model, so `lock_host` reports `decision_selected` false. `ExecutionPolicy` keeps its shape.

Billing is unchanged: runtime is charged in every posture (`afd_billing/src/price/rate.rs`), so a tenant pays the run fee on its own host as on a platform runner, as Indy ruled. **Implementation default:** refuse, not proxy. A daemon-side model proxy would let platform-model fleets run on tenant runners, but it needs a new transport and daemon-side metering; that is M251_001. Token charges stay safe without it: self-managed posture charges no tokens (`afd_billing/src/price/rate.rs`).

- **Dimension 4.1 — DONE** — the platform key never reaches a tenant runner → Test `platform_key_never_reaches_a_tenant_runner` (integration, `Fixtures::create_isolated`): the platform default key is a sentinel drawn per run. T's fleet on the platform default has an admission, and only T's runner is online. Two consecutive polls give T's runner no candidate, no slot write and no lease. A platform runner then leases the fleet, and its issued `ExecutionPolicy.api_key` equals the sentinel, so a test that never builds a key fails.
- **Dimension 4.2 — DONE** — a tenant runner carries its tenant's key → Test `tenant_runner_carries_the_tenant_key` (integration): T's fleet on T's own model leases on T's runner. The issued `ExecutionPolicy.api_key` equals T's vault key, and the posture is `self_managed`.
- **Dimension 4.3 — DONE** — a platform task revision is filtered and re-checked → Test `platform_task_revision_never_leases_on_a_tenant_runner` (integration): T's head revision names T's model while a task's accepted revision names `platform-default`. Two polls give T's runner no candidate. `Leases::issue_for_revision` handed that task's `Acquired` with T's runner returns `None` and writes no lease.
- **Dimension 4.5 — DONE** — a refused head blocks the whole queue → Test `tenant_runner_never_jumps_a_platform_model_head` (integration): fleet F's head admission is a task on a `platform-default` revision, and its next is a chat on T's model. T's runner claims neither. A platform runner claims the head, and only after it settles does T's runner claim the chat.
- **Dimension 4.6 — DONE** — a held fleet that moves to the platform model is released by its hold's expiry → Test `held_fleet_on_the_platform_model_waits_for_its_hold` (integration): F is held on T's runner, then its head revision moves to `platform-default`. A platform runner gets nothing while the hold is live; with the hold aged past `SANDBOX_HOLD_IDLE`, it leases F.
- **Dimension 4.4 — DONE** — a real tenant host completes a chat → Test `tenant_runner_completes_a_tenant_model_chat` (e2e, `integration_rust_runner::`): a tenant registers through `POST /v1/tenants/me/runners`, the runner binary boots with that token, leases T's chat on T's own model, completes it against the fake model, and its report lands.

### §5 — The tenant `runners` command group

`agentsfleet runners` serves a tenant what `agentsfleet platform runners` serves an operator. Both groups come from one definition. A `RunnerKind` value (`cli/src/commands/platform/runner-kind.ts`) names a kind's API path, the scope each verb needs and the verbs it offers. `PLATFORM_RUNNERS` keeps today's path, scopes and twelve verbs (`platform-runners.command.ts`). `TENANT_RUNNERS` uses `/v1/tenants/me/runners` and `runner:tenant`, and offers all verbs except `events` and `leases`: ten in total. Each handler in `cli/src/commands/platform/runners.ts` takes a `RunnerKind`. One factory in `platform-runners.command.ts` builds a group from it and hides a verb the session's scopes do not grant. `discovery.ts` learns scopes for the `runners` path as it does for `platform`.

`register` on the tenant reach mints one UUIDv7 `Idempotency-Key` per invocation and keeps it across the CLI's own retries, so a lost response inside one run never registers twice. The operator reach keeps `maxAttempts: 1`, because its route takes no key. It saves the token through the existing `--token-out` private file (mode 0600) and prints the token file's path. A replayed registration prints no token and exits 3, the server-answer code, suggesting `agentsfleet runners rotate-token <runner_id> --token-out <path>`. **Implementation default:** a data value, not a class per reach, because the reaches differ only in path, scopes and verb list.

- **Dimension 5.1 — DONE** — register saves the token once → Test `runners register saves the token and sends a key` (unit, `cli/test/runners.unit.test.ts`): a mocked `201` → the token lands in the `--token-out` file only, and the request carried a UUIDv7 `Idempotency-Key` against `/v1/tenants/me/runners`.
- **Dimension 5.2 — DONE** — a missing scope says which → Test `runners register without runner:tenant names the scope` (unit): a mocked `403 UZ-AUTH-022` → exit 3, the server-answer code every scope refusal uses (`platform-catalogue.unit.test.ts`), and a JSON error naming `runner:tenant`, with a suggestion.
- **Dimension 5.3 — DONE** — a replayed register tells the user how to recover → Test `runners register replay suggests rotate-token` (unit): a mocked `200` without a token → exit 3, no token file written, and a suggestion naming `agentsfleet runners rotate-token` with the runner's id. A mocked network failure, then a `201`, sends one key twice and writes the token once.
- **Dimension 5.4 — DONE** — the operator group is unchanged → Test `platform runners still call the admin path` (unit, `cli/test/platform-runners.unit.test.ts`): every operator verb still requests `/v1/admin/runners`.
- **Dimension 5.6 — DONE** — the built CLI registers against the mock API → Test `runners register from the built binary` (subprocess, `cli/test/runners.subprocess.test.ts`): the built `agentsfleet runners register` against the mock API → exit 0, the token file at mode 0600, and JSON on a piped stdout.
- **Dimension 5.5 — DONE** — the tenant group offers only tenant verbs → Test `tenant runners group hides operator-only verbs` (unit, `cli/test/scoped-discovery.unit.test.ts`): with `runner:tenant` alone, `agentsfleet runners --help` lists ten verbs and neither `events` nor `leases`. Without the scope, the group is not listed.

### §6 — The trust model, written down and published

`docs/AUTH.md` rewrites the runner principal section: a runner's tenant, tenant registration, `runner:tenant` and the platform-key refusals. Its scope catalogue gains `runner:tenant`, granted per account by a platform admin and never at signup. "An admin cannot register a runner" becomes "an account a platform admin grants `runner:tenant` registers only runners that take its own tenant's work". `runner_fleet.md` changes §Registering a runner and eligibility; labels become placement only, never a security control. `postgres_schemas.md` gains the column, the check, the receipt table and the deletion-matrix row. `runner_protocol.md` notes that a tenant runner needs a stream URL, CA file and route like any external runner. `push_scheduler.md` and M247_001 stop promising the placement setting under M246. `public/openapi.json` is regenerated. The docs repository gets a changelog entry on docs#253, and API reference pages after this Pull Request merges. The "Run your own runner" page waits for M251_001's public route, so the docs never invite a customer to a door that is closed.

- **Dimension 6.1 — DONE** — Indy accepts the written trust model → Test `trust_model_approved` (manual): Indy reads the rewritten runner sections of `AUTH.md` and `runner_fleet.md`, and his reply is quoted in the Pull Request Session Notes.

## Interfaces

| Method and path | Scope | Answers |
|---|---|---|
| `POST /v1/tenants/me/runners` (`Idempotency-Key`, a UUIDv7, required) | `runner:tenant` | `201` `RegisterResponse` with `runner_token`; `200` replay with `runner_id` and `assigned_policy`, no token; `400 UZ-REQ-001`; `401 UZ-AUTH-*` for an `agt_r` bearer; `403 UZ-AUTH-022`; `409 UZ-RUN-025` |
| `GET /v1/tenants/me/runners?limit=&starting_after=` | `runner:tenant` | `200`, the admin list item shape, this tenant's runners only, `total` this tenant's count |
| `GET`, `PATCH`, `DELETE /v1/tenants/me/runners/{runner_id}` | `runner:tenant` | the admin detail, patch and delete answers; `404 UZ-RUN-014` for a runner not owned; `400 UZ-REQ-001` for a `decision_model_id` edit |

The registration body is the admin `RegisterRequest` without `decision_model_id`. PATCH takes the admin body: an `action` (`cordon`, `drain`, `revoke`, `rotate`, `self_test`) or a labels and policy edit. The admin list and detail gain `tenant_id`, omitted for a platform runner (HTTP-NULL). On a tenant runner, the admin PATCH refuses `rotate` with `409 UZ-RUN-026`.

OpenAPI operation IDs name the two kinds of runner. Platform runners, under `/v1/admin/runners`: `register_platform_runner`, `list_platform_runners`, `get_platform_runner`, `update_platform_runner`, `delete_platform_runner`, `list_platform_runner_events` and `list_platform_runner_leases`, replacing `register_runner` and the stale `*_fleet_runner*` names. Tenant runners, under `/v1/tenants/me/runners`: `register_tenant_runner`, `list_tenant_runners`, `get_tenant_runner`, `update_tenant_runner` and `delete_tenant_runner`.

New codes: `UZ-RUN-025` `RUNNER_REGISTRATION_KEY_REUSED`, "Registration key reused with a different request", 409. `UZ-RUN-026` `RUNNER_TENANT_MANAGED`, "Runner is managed by its tenant", 409. `UZ-RUN-027` `RUNNER_PLACEMENT_REFUSED`, a log-only code for a placement the claim should have filtered. The runner binary, its environment variables and the stream protocol do not change.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Foreign runner at the claim | security | a scan bug, or a direct claim | no row; slot, fence and admission untouched | `claim_statement_refuses_a_foreign_runner` |
| Hole in the claim filter | security | a claim predicate regression | issue refuses under the host lock, no lease row, an `error` line with `UZ-RUN-027` | `issue_refuses_a_foreign_runner_under_the_host_lock` |
| Setup draft to a foreign host | security | authoring scan or claim | never offered; `admit` answers `Skipped` | `authoring_placement_refuses_a_foreign_runner` |
| Platform model key on a tenant host | security | platform-model work placed on a tenant runner | never a candidate; `Leases::issue` refuses `platform` posture | `platform_key_never_reaches_a_tenant_runner`, `platform_task_revision_never_leases_on_a_tenant_runner` |
| Platform decision key on a tenant host | security | a decision model set on a tenant runner | the table check raises; `register` and `edit` answer `400 UZ-REQ-001` | `tenant_runner_refuses_a_decision_model`, `platform_reach_cannot_rotate_or_edit_a_tenant_runner` |
| Operator takes over a tenant host | security | an admin rotate on a tenant runner | `409 UZ-RUN-026`; row and token unchanged | `platform_reach_cannot_rotate_or_edit_a_tenant_runner` |
| Reading another tenant's host | auth failure | a guessed `runner_id` | `404 UZ-RUN-014`, the other row unchanged | `tenant_lists_and_manages_only_its_runners` |
| Enrolling without the scope | auth failure | a narrowed person or key | `403 UZ-AUTH-022`, no row | `registration_without_runner_tenant_is_refused` |
| Retried registration | replay | a lost `201` | `200` with the runner and no token; the CLI suggests `rotate-token` | `registration_retry_replays_without_the_token`, `runners register replay suggests rotate-token` |
| Concurrent duplicate registration | race | two posts with one key | one runner; the rest replay | `concurrent_registrations_with_one_key_leave_one_runner` |
| Malformed registration | malformed input | no key, a non-UUIDv7 key, or a decision model | `400 UZ-REQ-001`, no row | `registration_retry_replays_without_the_token` |
| Rotation retried | replay | a lost rotate answer | PATCH `rotate` is not idempotent (HTTP-PATCH): each call mints a token, so a retry kills the token the first call returned; the caller rotates again | `tenant_lists_and_manages_only_its_runners` (second rotate) |
| A live runner's token rotated | auth failure | a tenant rotates a connected runner | the old token dies at once: the next heartbeat answers `401 UZ-RUN-001`, and the runner halts until its token is replaced (M251_001 adds overlap rotation) | `tenant_lists_and_manages_only_its_runners` |
| Tenant erased while its runner is connected | auth failure | account erasure | the cascade removes the row and its leases; the stream closes, and a reconnect answers `401 UZ-RUN-001` | `tenant_revoke_closes_its_runner_stream` |
| Operator and tenant edit one runner | race | both reaches PATCH a tenant runner | last write wins, an HTTP-ETAG opt-out: a platform admin edits a tenant runner only by exception, so concurrent edits are not a realistic case, and the token stays the tenant's alone | `platform_reach_cannot_rotate_or_edit_a_tenant_runner` |
| A tenant runner flipped to platform | security | a statement writes `tenant_id` | the trigger raises; the row keeps its tenant | `runner_tenant_cannot_change` |
| Platform-model work with only tenant runners online | capacity | the tenant has no platform runner in reach | work waits for a platform runner | `platform_key_never_reaches_a_tenant_runner` |
| A held fleet moves to the platform model | capacity | fleet F, held on T's runner, switches to `platform-default` | T's runner may not claim it, and platform runners wait for the hold to lapse (`SANDBOX_HOLD_IDLE`, 10 minutes), an accepted limit | `held_fleet_on_the_platform_model_waits_for_its_hold` |
| A fleet's queue mixes models | ordering | a task on a platform revision heads a queue with tenant-model chats behind it | the tenant runner takes nothing from that fleet; the head waits for a platform runner | `tenant_runner_never_jumps_a_platform_model_head` |

No per-tenant runner limit exists in this spec, an accepted limit.

## Invariants

1. No fleet or setup lease lands on a runner owned by another tenant. The claim statement's `fleet` predicate and the issue and `admit` re-check under `lock_host.sql`'s `FOR UPDATE` enforce it, against Postgres only; `cross_tenant_placement_matrix` is the counterexample. Failure assumption: every replica runs this build (Release order).
2. No lease on a tenant runner carries a platform key. The claim predicate, `Leases::issue_for_revision`'s posture check and the tenant-or-decision-model table check enforce it; `platform_key_never_reaches_a_tenant_runner`. Failure assumption: every replica runs this build (Release order).
3. A runner's tenant never changes after registration. The §1 trigger refuses it; `runner_tenant_cannot_change`. Failure assumption: none; the database enforces it.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `runner_placement_refused` (log event, `error`, `error_code` `UZ-RUN-027`) | operator | issue or `Drafts::admit` refuses a placement the claim should have filtered | `reason` (`foreign_tenant`, `platform_model`), `lease_type` (`fleet`, `authoring`), `runner_id` | no tenant, fleet or draft text | `issue_refuses_a_foreign_runner_under_the_host_lock`, `platform_task_revision_never_leases_on_a_tenant_runner` |

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
| R6 | A tenant registers, and manages only its own runners (§2) | `grep -c -E "(tenant_registers_a_tenant_runner\|tenant_lists_and_manages_only_its_runners) ... ok" .tmp/rustd-integration.log` | `2` | P0 | |
| R7 | A retried registration leaves one runner (§2) | `grep -c "concurrent_registrations_with_one_key_leave_one_runner ... ok" .tmp/rustd-integration.log` | `1` | P0 | |
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
| The `runner:enroll` scope and the `enroll` CLI verb | `git grep -n -w -e 'runner:enroll' -e RunnerEnroll -e runnerEnroll -- . ':!docs/v2/done' ':!docs/v2/evidence'` | no output outside this spec's Discovery |

Schema this milestone leaves with no runtime reader or writer is dropped in a new slot under Indy's blanket approval (Discovery); none is expected.

## Out of Scope

- Moved to M251_001: the public stream route and its certificate authority; the executor signal audit (the Nov 06 draft's §0); rotation without a restart (its §5); the tenant placement setting; the daemon-side model proxy and its metering; Settings → Runners and the admin badge.
- One-time registration codes and self-registration: excluded in the reviewed EGM brief (`concepts_brief.md`, Nov 06, 2026); `runner_fleet.md` keeps mode C for later.
- Counting holds, auto-cordon and checkpoint respawn: M247.
- Tenant-visible runner events and lease history: the admin routes keep them.
- Sharing one runner across tenants, per-workspace runners, and a per-tenant runner limit: not planned.
- Data residency: history, checkpoints and keys pass through `agentsfleet` whatever the runner, and the docs page says so.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An owner runs `agentsfleet runners register --host-id build-box-1 --token-out ./runner.token`. They start the runner on a machine that reaches the stream listener through a route their operator provides, since the public route is M251_001. They send their fleet a message, and `agentsfleet runners show` reports that machine busy with it.
2. **Preserved user behaviour** — Fleets on platform runners run as today. A tenant with no runners sees no change. Operators register and manage platform runners as before, with a tenant column added.
3. **Optimal-way check** — One mint and one placement predicate deliver the moment. The gaps: existing accounts need `runner:tenant` granted, platform-model fleets run only on platform runners, and the stream route is the operator's until M251_001.
4. **Rebuild-vs-iterate** — Iterate: the existing registration, admin handlers, operator CLI and label placement are extended; no new service.
5. **What we build** — A tenant column and its filter, the `runner:tenant` routes, the platform-key refusals, a tenant CLI group and the one-list scope catalogue.
6. **What we do NOT build** — Registration codes (excluded); tenant lease history (admin only); cross-tenant sharing (breaks the boundary); a dashboard page, a placement setting and a model proxy (M251_001).
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
  - `Capacity` and `Held` derive `Copy`, while `Uuid7` does not (`af_core/src/id.rs`), so they carry `serves` as a boolean the statement computed.
  - The operator CLI ships twelve runner verbs (`cli/src/program/tree/platform-runners.command.ts`), and discovery reads scopes only under `platform` (`cli/src/program/entry/discovery.ts`).
  - The stream listener is private to Fly (`deploy/fly/agentsfleetd-prod/fly.toml`); the runner trusts OS roots plus an optional CA file (`afr_supervisor/src/config/stream.rs`).
  - Rule interpretation, an agent default: HTTP-IDEMKEY asks a replay to return the first response, and AUTH-SECRET forbids keeping the token. The replay therefore returns the first response without its token. Indy may overrule it at the trust-model review (Dimension 6.1).
- **EGM review** — four reviewers on Claude Opus 5.5, Nov 17, 2026: security and design, references, readiness and standards, structure and style. Findings: 2 blockers (the decision key path, and the head-only model filter looping on task revisions), 15 major and about 25 minor. All are applied above except where the owner decides.
- **Owner decisions** —
  > Indy (Nov 06, 2026: 10:19 AM): "This will be on by default durable_tasks_v1. ... my plan also was to have folks spin a runner and point to the agentsfleetd (in cloud) and keep using it." — context: tenant-registered runners against the cloud daemon.
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
  > Indy (Nov 17, 2026): "But you dont know if the NULL means a platform or a tenant? or caused by a bug?" — context: §1's ownership became the explicit `platform_owned` flag, and every reader fails closed; §2's `Reach` became the `RunnerScope` handle.
  > Indy (Nov 17, 2026): "to me this is politically stupid crap code you wrote: self.tenant.is_none_or(...)" — context: the host lock computes `serves` in SQL and `Held` carries booleans, as §3 always said.
  > Indy (Nov 17, 2026), choosing from the Fable 5.2 SQL review: "Q5 and Q1 (Recommended), Q4 and Q6 indexes, Also Q2" — context: gate unparking touches only parked rows; a report ends its lease at the report instant; the slot index drops `lease_expires_at`; three unused admissions indexes go; a notification pass waits for the pending-work probe. Q3, R1, R2 and R3 are not in this Pull Request.
  > External review (Tarzy, Nov 17, 2026): the `$2 IS NULL` branch in `lock_host.sql` reads a missing proof as authorized — context: present on `main`; the stream proof is now a bound flag, so a stream call without its proof is refused.
  > Indy (Nov 18, 2026): "There are several gates that fail test-unit-rustd, lint/safety-gates, test-integration-rustd just fix that" and "this is failing too" (the `build-daemon-musl / toolbox` job) — context: CI's `ubuntu-latest` moved to Ubuntu 26.04 on Nov 18, 2026.
  > Indy (Nov 18, 2026): "Yes raise the cap to 300" and "and push in this PR" — context: the local integration lane's Postgres hit its 100-connection default.
  > Indy (Nov 18, 2026): chose "Delete it in #850 (Recommended)" — context: `CPUAccounting=yes` leaves the runner unit, because the CI image's systemd warns that it is removed and `safety-gates` fails on any warning.
  > Indy (Nov 18, 2026): "Yes fold it in" — context: the runner-count race on `main` rides this Pull Request; `/v1/admin/runners` could answer a `total` below its own rows when a runner registered between its count and its page.
- **Second EGM review** — eleven reviewers under one coordinator, Nov 17, 2026. The coordinator and reviewers were launched on Claude Fable 5.2; Indy asked to stop Fable for cost, and the coordinator relaunched its reviewers after the first ten were stopped. Findings: 0 blockers against the revised text, 23 major and 19 minor. Applied: every reference, file, Dimension, wording and standards fix, the head-only ordering rule, the `tenant_owned` posture input, the release order, the HTTP-NULL omission and the HTTP-ETAG opt-out. Owner decisions: the four blockers below.
- **Open blockers** — none. The four raised by the second review were ruled on Nov 17, 2026:
  > Indy (Nov 17, 2026): chose "200, no token (Recommended)" — context: a retried tenant registration answers `200` with the runner and no token, after seeing the three-command recovery (`register`, `rotate-token`, start).
  > Indy (Nov 17, 2026): "Ideally on, but this is not getting turned on now. Ideally the api's will refuse to work for any one other than the platform admin? A signed in user will need to have that access given?" — context: durable tasks stay off on tenant runners in M246, and only the platform reach may change the flag.
  > Indy (Nov 17, 2026): chose "Unchanged (Recommended)" — context: the run fee applies on a tenant's own runner as on a platform runner.
  > Indy (Nov 17, 2026): chose "Accept both (Recommended)" — context: no per-tenant runner cap, and a held fleet that moves to the platform model waits up to `SANDBOX_HOLD_IDLE`; both revisited in M251_001.
  > Indy (Nov 17, 2026): "Okay keep going, i am looking a tthe standard way so should we say CLI command - register, Scope runner:register and why is the API register_runner" and "you have enroll say register?" — context: the API has said `register_runner` since May 2026 (`bfeae0a4f`); `runner:enroll` (June, `e1dd7bdb9`) and the CLI verb (Nov 10, `909193145`) are renamed to `register`. The identity provider's session-token template and every operator grant holding `runner:enroll` need the new string, a manual step for Indy at deploy; no alias is kept.
  > Indy (Nov 17, 2026): "can you show me the url" "of register_runner" "since its incorrect" — context: the URL `POST /v1/admin/runners` is the standard create shape; the sibling operation IDs carried a stale `fleet_` prefix and are renamed with this Pull Request.
  > Indy (Nov 17, 2026): "i think name it platform runners? and tenant runners platform runners start with url /v1/admin/runners/... tenant runners start with url /v1/tenant/me/runners/.... (this scope will not be activated for regular user, platform admin indy will have)" — context: operation IDs say `platform_runner` and `tenant_runner`; the tenant path keeps the existing `/v1/tenants/me/` plural, as his earlier "The tenant urls must be v1/tenants/me/runners" and every other tenant route spell it; `runner:tenant` stays out of `TENANT_OWNER_GRANT`.
  > Indy (Nov 17, 2026): "Also i expect you to read through all the @schema/*.sql and normalize the code comments reduce it so there is no dependency to just change code comment for the sake of it, and the PR barfs saying the dev cant be deployed." — context: the in-place edit of `20261108123100_runner_runners.sql` would have failed dev's migrate; §1 moved into a forward slot. How existing slots' comments are reduced is his call, since every applied file is checksummed whole.
  > Indy (Nov 17, 2026): chose "Reset dev, normalize now" — context: every schema slot's comments are normalized in this Pull Request, dev's database is reset once before deploy, and §1 returns to editing the runners table file; the forward slot is withdrawn.
  > Indy (Nov 17, 2026): "also while doing it check the unused columns, and any thing we could make the database sqls performant" — context: an audit of columns with no reader or writer, and of indexes against the statements that read them, rides the same pass.
  > Indy (Nov 18, 2026): chose "Accept fixed key (Recommended)" — context: Dimension 4.1's platform key is the fixed fixture key, since a per-run key would overwrite the shared platform default other suites lease against.
  > Indy (Nov 18, 2026): chose "Approved as written" — context: Dimension 6.1, the rewritten runner sections of `docs/AUTH.md` and `docs/architecture/runner_fleet.md`.
  > Indy (Nov 18, 2026): chose "Fix in #850 now" — context: idle tenant runners were offered every wake first and each cost a claim scan; now §3.5.
  > Indy (Nov 18, 2026): chose "Keep the removals (Recommended)" — context: tests removed with the dropped columns and indexes they alone asserted.
  > Indy (Nov 18, 2026): "I know we did it earlier, have you reviewed the queries to see if there are stale index, repetitve FK, cyclick FK, index that doesnt make sense on scale, and simple tables and their code associalton" and "you are free to do an inline table edit in sql files since we will teardown the db in dev and we are not in production yet" — context: the schema audit's index changes above.
  > Indy (Nov 18, 2026): "Also ensure the greptile is resolved and pushed." — context: the CLI owner field, the PATCH reply shapes and the deterministic race test.
  > Indy (Nov 18, 2026): "2. B" — context: record the registering person on the registration event in this PR (Dimension 2.10); every other runner event already records its actor.
  > Indy (Nov 18, 2026): "3. A" — context: `connector.person_links` stays; its upsert enforces current membership and no silent re-linking, and the reader that resolves a Slack approver to a member becomes its own milestone.
- **Deferrals and transfers** — The Nov 06 draft's §0, §5, placement setting, model proxy, Settings → Runners and admin badge transfer to M251_001, under Indy's Nov 17 quote above. No `MOVED` row is used. Each moved criterion is rewritten as M251_001's own P0 row: the executor signal outcome, overlap rotation, the token reload, the placement holder refusal, the proxied key and metering, the proxied chat and the Settings page.
