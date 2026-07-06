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

# M102_006: Grant-gated credential mint + lease — restore M102_001 Invariant 3

**Prototype:** v2.0.0
**Milestone:** M102
**Workstream:** 006
**Date:** Jul 06, 2026
**Status:** PENDING
**Priority:** P0 — live security gap on main: any connected fleet with a valid lease mints a provider token with **no** approved-integration-grant check; the grant lifecycle (request/approve/revoke) exists and is operable, but nothing reads it at mint or lease.
**Categories:** API
**Batch:** B1 — single-section-family security fix; carved out of M102_005 §2 so it ships ahead of the ingress/CLI/docs tail.
**Depends on:** none — `core.integration_grants` (schema 008), its request/approve/revoke lifecycle, and the mint broker are all shipped; this workstream only adds the missing reads.
**Provenance:** carve-out of M102_005 §2 per the approved incident-fleet office-hours design (approach C, `docs/v2/reviews/incident-fleet-office-hours-design.md`); gap independently confirmed by direct code read this session (zero grant references in the mint path).
**Canonical architecture:** `docs/AUTH.md` §credential boundary + the `agt_r` plane; `docs/architecture/capabilities.md` §3 (broker row). Introduces no new trust plane — two reads on an existing table at two existing enforcement points.

---

## Overview

**Goal (testable):** `POST /v1/runners/me/credentials/mint` refuses with a typed `UZ-GRANT-004 grant_required` (403, no token, no upstream call) when the lease's fleet has no `approved` row in `core.integration_grants` for the requested integration; the lease path emits a `mintable` entry ONLY when the grant is approved (an ungranted connector credential is omitted entirely — never leaked into `secrets_map`); a grant revoked mid-lease refuses the next mint; and `SUPPORTED_SERVICES` accepts the connector providers (`github`, `zoho`, `jira`, `linear`) so the gate is actually reachable for them.

**Problem:** schema 008's own comment states the invariant — "A fleet must have an approved grant for a service before agentsfleet will inject credentials for it" — but no code enforces it. On main today: `credentials_mint.zig` resolves the lease + vault handle and calls `broker.mint` with no grant read; `service.zig`'s lease classifier emits every connected mintable unconditionally; and `integration_grants/handler.zig`'s `SUPPORTED_SERVICES` list (`slack, gmail, agentmail, discord, grafana`) does not even include `github`, so a GitHub grant cannot be requested at all. M102_001 Invariant 3 is designed, documented, and inert. This is the single platform prerequisite for the approved incident-fleet v1.0 wedge (an unattended, Pull-Request-opening fleet must not mint ungated tokens).

**Solution summary:** one shared read module (`state/integration_grant_lookup.zig`: `isApproved` for the mint hot path, `approvedSet` for the lease batch), wired into BOTH enforcement points — `loadMintInputs` refuses before the vault load, and `resolveExecutionPolicy`'s classification loop omits ungranted mintables. Add the four connector providers to `SUPPORTED_SERVICES` via the existing `common.PROVIDER_*` constants. Register `UZ-GRANT-004` with a hint pointing at the grant-request flow. The broker (`credentials/`) is untouched.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m102): grant-gate credential mint + lease — restore Invariant 3
- **Intent (one sentence):** a fleet without a human-approved integration grant can no longer obtain a provider token — refused at lease-classification and re-checked at mint — closing the gap between the shipped grant lifecycle and the credential path that ignored it.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/AUTH.md` §credential boundary + §Runner token — **auth-flow surface; mandatory read before any mint/lease edit.**
2. `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` — `loadMintInputs` (lease resolve at `resolveLeaseWorkspace`, then vault load) is where the mint-time read slots in; `dispose()` is the pure result→wire mapper whose test style the refusal follows.
3. `src/agentsfleetd/fleet/service.zig` `resolveExecutionPolicy` — the classification loop (`secrets_resolve.mintableId` → `mintable` list vs `secrets_map`) is where the lease-time gate slots in; `issueLease` has `acq.fleet_id` in scope to fetch the approved set.
4. `src/agentsfleetd/http/handlers/integration_grants/handler.zig` — `GrantStatus` (status strings owned here), `SUPPORTED_SERVICES`, and the request/idempotency flow the refusal hint points at.
5. `schema/008_core_integration_grants.sql` — fleet-scoped (`UNIQUE (fleet_id, service)`, indexed on `fleet_id`); the reads bind `status = 'approved'` via `GrantStatus.approved.toSlice()`, never a fresh literal (RULE UFS).
6. `src/agentsfleetd/http/handlers/runner/memory.zig` — the sibling runner-plane handler whose active-lease predicate `resolveLeaseWorkspace` already mirrors; keep the extended (workspace, fleet) resolve consistent with it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/state/integration_grant_lookup.zig` | CREATE | the ONLY grant-read module: `isApproved(conn, fleet_id, service)` (mint hot path) + `approvedSet(alloc, conn, fleet_id)` (lease batch) |
| `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` | EDIT | lease resolve returns (workspace_id, fleet_id); grant check between lease resolve and vault load; `UZ-GRANT-004` refusal |
| `src/agentsfleetd/fleet/service.zig` | EDIT | fetch `approvedSet` at lease-issue; classification loop emits a mintable ONLY when approved; ungranted → omitted + warn |
| `src/agentsfleetd/http/handlers/integration_grants/handler.zig` | EDIT | `SUPPORTED_SERVICES` += `common.PROVIDER_{GITHUB,ZOHO,JIRA,LINEAR}`; refresh the rejection detail string |
| `src/agentsfleetd/errors/error_registry.zig` + `errors/error_entries.zig` | EDIT | `ERR_GRANT_REQUIRED = "UZ-GRANT-004"` (403) + `hint()` naming the grant-request flow |
| `docs/v2/pending/M102_005_P1_API_CLI_DOCS_AGENT_IDENTITY_PROXY_TAIL.md` | EDIT | §2 replaced with a moved-to-M102_006 pointer; grant rows dropped from its tables (same-commit NDC hygiene) |
| _colocated tests (Zig `test {}` + integration suite)_ | CREATE/EDIT | one test per Dimension below |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (status literal via `GrantStatus.approved.toSlice()`, provider ids via `common.PROVIDER_*`, error code named constant — all shared verbatim with tests), **ECL** (`grant_required` is a typed refusal class, distinct from `not_connected`/`reconnect_required`), **EMS** (standard error body via `hx.fail`), **VLT** (no grant payload, handle, or token in any log/frame — only fleet id, integration id, refusal reason), **NDC** (M102_005 §2 removed in the same commit, no contradicting duplicate scope), **NLR/ORP** standard floor.
- **`dispatch/write_zig.md`** — pg-drain discipline (`PgQuery` with `defer deinit()`; `conn.exec` for no-rows), file ≤350 / fn ≤50, cross-compile both linux targets.
- **`docs/AUTH.md`** — auth-flow surface: the gate rides the existing lease trust anchor (Invariant 2's runner-scoped lease resolve); no new plane, no new header, no Bearer semantics change.
- **`docs/LOGGING_STANDARD.md`** — new warn events carry `error_code` + scoped logger; no secret material.
- No schema Data Definition Language — `core.integration_grants` exists and is only READ; `docs/SCHEMA_CONVENTIONS.md` does not apply.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all edits are `*.zig` | PgQuery deinit; single-purpose fns ≤50; cross-compile x86_64 + aarch64 linux |
| PUB / Struct-Shape | yes — `integration_grant_lookup` pub surface | two narrow pub fns; FILE SHAPE verdict at PLAN |
| File & Function Length | yes | new module well under 350; `loadMintInputs` gains ~10 lines — extract if it crosses 50 |
| UFS | yes — status/provider/code literals | constants imported from their owning modules; tests import the same |
| ERROR REGISTRY | yes — `UZ-GRANT-004` | registered with status 403 + `hint()`; docs error-codes row at DOCUMENT |
| LOGGING | yes — two new warn events | `credential_mint_denied`, `lease_secret_grant_missing`; VLT-clean fields |
| LIFECYCLE | yes — allocations in `approvedSet` | arena-owned returns; `errdefer` on multi-step builds |
| UI / DESIGN TOKEN / SCHEMA | no | no UI surface; no schema change |

## Prior-Art / Reference Implementations

- **Lease-scoped resolve:** `credentials_mint.zig`'s own `resolveLeaseWorkspace` (runner-scoped, live-lease predicate) — extend its SELECT with `fleet_id::text`, keeping the predicate identical to the `memory.zig` sibling.
- **Status read shape:** `integration_grants/handler.zig` `fetchExistingGrant` — same table, same PgQuery idiom; the lookup module is its read-only, status-filtered sibling.
- **Refusal wiring:** `dispose()`'s typed Disposition table + its pure unit tests — the grant refusal is written in the same style (typed code, provider-neutral detail, hint).
- **Divergence from parent spec:** M102_005 §2 named `secrets_resolve.zig` as an edit site and "resolves static-only" as the ungranted behaviour. Both corrected here: the classification loop lives in `service.zig` (so `secrets_resolve` stays pure), and an ungranted mintable is **omitted entirely** — falling through to static would ship the raw vault handle (`installation_id`, refresh-token fields) into `secrets_map`, violating the very VLT invariant the classifier's comment pins ("the stored handle/App config NEVER reaches the child").

## Sections (implementation slices)

### §1 — Shared grant lookup + supported-services alignment

Delivers the single read path both enforcement points import, and makes connector grants requestable at all. **Implementation default:** `state/integration_grant_lookup.zig` (sibling of `vault.zig`); `isApproved` is one indexed `SELECT 1 … WHERE fleet_id = $1 AND service = $2 AND status = $3 LIMIT 1`; `approvedSet` is one `SELECT service …` returning an arena-owned set for the lease loop — never N per-credential queries.

- **Dimension 1.1** — `isApproved` returns true only for `status = approved` (pending → false, revoked → false, no row → false); `approvedSet` returns exactly the approved services for the fleet → Test `test_grant_lookup_status_predicate`
- **Dimension 1.2** — `SUPPORTED_SERVICES` includes `github`, `zoho`, `jira`, `linear` via `common.PROVIDER_*` constants; the rejection detail names the full list; a `POST …/integration-requests {service:"github"}` now creates a pending grant → Test `test_supported_services_include_connectors`

### §2 — Mint-time enforcement (defense-in-depth re-check)

Delivers the hot-path refusal. **Implementation default:** the check runs inside `loadMintInputs`, after the lease resolve (which now also returns `fleet_id`) and BEFORE the vault handle load, on the same held connection — no extra acquire, and an ungranted request never touches vault bytes.

- **Dimension 2.1** — mint with no approved grant → 403 `UZ-GRANT-004`, no token, no upstream call, `credential_mint_denied` warn logged → Test `test_mint_requires_approved_grant`
- **Dimension 2.2** — grant approved at lease-issue then revoked → the NEXT mint refuses (mint-time re-check, not just lease-time) → Test `test_mint_rechecks_revoked_grant`

### §3 — Lease-time gating (fail-fast classification)

Delivers the early gate so an ungranted fleet's run fails deterministically at substitution rather than mid-run. **Implementation default:** `issueLease` fetches `approvedSet(acq.fleet_id)` once alongside the secrets resolve; `resolveExecutionPolicy` receives it and the classification loop emits a `Mintable` only when its integration id is in the set; an ungranted mintable credential is dropped from BOTH `mintable` and `secrets_map`, with a `lease_secret_grant_missing` warn naming fleet, credential name, and integration.

- **Dimension 3.1** — connected but ungranted `github` → lease's `ExecutionPolicy.mintable` is empty AND `secrets_map` carries no handle fields → Test `test_lease_gates_mintable_on_grant`
- **Dimension 3.2** — static custom secrets (no `integration` field) are untouched by the gate: resolved into `secrets_map` exactly as today → Test `test_static_secrets_unaffected_by_grant_gate`

## Interfaces

```
state/integration_grant_lookup.zig (NEW — the only grant-read module):
  isApproved(conn, fleet_id, service) !bool          # one indexed read; status = GrantStatus.approved
  approvedSet(alloc, conn, fleet_id) !ApprovedSet    # one batch read per lease-issue; arena-owned

POST /v1/runners/me/credentials/mint (EXISTING route — one additive refusal outcome):
  -> 200 { token, expires_at_ms }                    # unchanged happy path
  | 403 UZ-GRANT-004 grant_required                  # NEW: no approved grant; no token, no upstream call
  | (existing outcomes unchanged: 404 lease, UZ-CRED-*, UZ-GH-*, UZ-CONN-006)

Lease ExecutionPolicy (EXISTING wire shape — emission rule tightened):
  .mintable[] entry emitted ONLY when the fleet's grant for that integration is approved;
  ungranted connector credential -> omitted from mintable AND secrets_map (never a fallback to static)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Mint without approved grant | connected fleet, valid lease, no/pending grant | 403 `UZ-GRANT-004` + hint naming `POST …/integration-requests`; no token; no upstream call |
| Grant revoked mid-lease | approved at lease-issue, revoked before next tool call | next mint refused by the mint-time re-check; child's tool call errors with the typed detail |
| Ungranted at lease-issue | fleet declares a connector credential, grant absent | credential omitted from `mintable` + `secrets_map`; `lease_secret_grant_missing` warn; substitution fails fast naming the missing credential |
| Grant read fails (DB error) at mint | transient DB failure on the grant SELECT | fail closed: 500 internal, no token — never fail-open to a mint |
| Grant read fails at lease-issue | transient DB failure on `approvedSet` | mirror the secrets-resolve transient path: release the claim, no-work reply, delivery stays leasable — never a lease with an ungated mintable |
| Grant approved but integration never connected | grant row exists, no `fleet:<provider>` vault handle | existing `UZ-CRED-*` not-connected path unchanged (grant check passes, vault load fails as today) |
| Pre-existing connected-but-ungranted fleets break on upgrade | intended fail-closed behaviour change | DOCUMENT stage ships a changelog **Upgrading** entry: request + approve the grant once per fleet; refusal hint self-describes the fix |

## Invariants

1. **No token without an approved grant** — enforced at BOTH `resolveExecutionPolicy` (emission) and `loadMintInputs` (re-check) via the same module; proven by Tests 2.1/2.2/3.1, including the no-upstream-call assertion.
2. **One grant-read implementation** — `integration_grant_lookup.zig` is the only module querying `core.integration_grants` for enforcement; grep-enforced: no `integration_grants` SQL outside `state/integration_grant_lookup.zig`, `handlers/integration_grants/`, and `webhooks/grant_approval.zig`.
3. **An ungranted mintable handle never reaches the child in any form** — omitted from `mintable` AND `secrets_map` (VLT); Test 3.1 asserts both absences.
4. **The broker stays grant-free** — no grant logic in `credentials/`; grep `grant` over `src/agentsfleetd/credentials/` returns zero enforcement references (separation: the broker mints, the boundary authorizes).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `credential_mint_denied` (operator warn) | ops | mint refused for missing/revoked grant | fleet id, integration id, reason | no token/handle/grant-reason bytes (VLT) | `test_mint_requires_approved_grant` |
| `lease_secret_grant_missing` (operator warn) | ops | lease classification drops an ungranted mintable | fleet id, credential name, integration id | no handle fields | `test_lease_gates_mintable_on_grant` |

No product analytics or funnel change — security-plane observability only; no analytics/funnel playbook update required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_grant_lookup_status_predicate` | approved → true; pending/revoked/absent → false; `approvedSet` returns exactly the approved services |
| 1.2 | unit | `test_supported_services_include_connectors` | `isSupportedService("github"/"zoho"/"jira"/"linear")` true; list literals come from `common.PROVIDER_*` |
| 2.1 | integration | `test_mint_requires_approved_grant` | lease + connected handle + NO grant → 403 body carries `UZ-GRANT-004`; fake exchange records zero upstream calls |
| 2.2 | integration | `test_mint_rechecks_revoked_grant` | approve → lease → revoke → mint → 403 `UZ-GRANT-004`; approve again → mint succeeds |
| 3.1 | integration | `test_lease_gates_mintable_on_grant` | ungranted github: lease response `mintable == []` and `secrets_map` has no github entry; approved: `mintable` carries it |
| 3.2 | integration | `test_static_secrets_unaffected_by_grant_gate` | a static custom secret (no `integration` field) resolves into `secrets_map` with no grant row present |
| — | regression | existing `dispose` unit tests + broker suites | unchanged — the refusal happens before the broker; no `MintResult` variant added |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Mint refused without approved grant, no upstream call (§2) | `make test-integration 2>&1 \| grep -E "mint_requires_approved_grant"` | pass line | P0 | |
| R2 | Revoked-mid-lease re-check (§2) | `make test-integration 2>&1 \| grep -E "mint_rechecks_revoked_grant"` | pass line | P0 | |
| R3 | Lease omits ungranted mintable from both surfaces (§3) | `make test-integration 2>&1 \| grep -E "lease_gates_mintable_on_grant\|static_secrets_unaffected"` | 2 pass lines | P0 | |
| R4 | Connector grants requestable (§1) | `make test 2>&1 \| grep -E "supported_services_include_connectors"` | pass line | P1 | |
| R5 | Single grant-read module (Invariant 2) | `grep -rln "integration_grants" src/agentsfleetd --include='*.zig' \| grep -v "state/integration_grant_lookup\|handlers/integration_grants\|webhooks/grant_approval\|_test"` | no output | P0 | |
| R6 | Broker stays grant-free (Invariant 4) | `grep -rn "grant" src/agentsfleetd/credentials/*.zig` | no enforcement references | P1 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | Cross-compile both linux targets | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S5 | No secrets in diff | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — none deleted; this workstream is additive plus one spec edit.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted | — |

**2. Orphaned references — zero remaining after the M102_005 slim.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| M102_005 §2 grant dimensions (2.1–2.3) and their rubric/test rows | `grep -n "test_mint_requires_approved_grant\|test_lease_gates_mintable\|gates_mintable_on_grant" docs/v2/pending/M102_005_*.md` | 0 matches (all moved here) |
| duplicated grant predicate outside the lookup module | rubric R5 grep | no output |

## Out of Scope

- **Gating static custom secrets** (`${secrets.*}` with no `integration` field) — grants gate on-demand-minted connector credentials only; gating statics would break every existing fleet using pasted vendor keys and belongs, if ever, to its own consulted workstream.
- **M102_005 §1 ingress, §3 connector CLI, §4 docs sweep** — remain in the slimmed M102_005.
- **Broker/`MintResult` changes** — the refusal is a handler-boundary concern; the broker never learns about grants.
- **Grant UI changes** — the dashboard approval inbox and request flow already exist; the hint points at them.
- **Atomic lease-recheck-after-mint** — the pre-existing residual race documented in `credentials_mint.zig` stays a separate follow-up; this gate neither widens nor closes it.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator approves a fleet's `github` grant once in the dashboard; from then on the incident fleet mints scoped tokens invisibly — and the day the operator revokes it, the very next mint is refused with an error that names the fix.
2. **Preserved user behaviour** — fleets with approved grants and all static-secret fleets behave exactly as today; the happy-path mint wire shape is unchanged.
3. **Optimal-way check** — yes: two reads on an existing indexed table at the two existing enforcement points; no new plane, table, or endpoint.
4. **Rebuild-vs-iterate** — iterate; the parent design (M102_001 Invariant 3) was right, only unenforced.
5. **What we build** — one lookup module, two wired reads, four registry entries, one error code.
6. **What we do NOT build** — static-secret gating, broker changes, new UI, the M102_005 tail.
7. **Fit with existing features** — completes the grant lifecycle (request/approve/revoke shipped in M102_001) and unblocks the incident-fleet wedge (office-hours design approach C); must not destabilize the mint hot path or lease issuance.
8. **Surface order** — backend only; the CLI signpost (`agentsfleet connector`) stays in M102_005 §3.
9. **Dashboard restraint** — N/A — no User Interface (UI) built; the existing approval inbox is the surface.
10. **Confused-user next step** — the 403 detail + `hint()` name the exact request-grant call; the changelog Upgrading entry covers the one-time migration for pre-existing connected fleets.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections — lookup+registry (§1), mint re-check (§2), lease gate (§3) — one PR, because the invariant is only true when both enforcement points land together.
- **Alternatives considered:** (a) mint-time check only — rejected: an ungranted fleet would discover the refusal mid-run instead of at substitution, and Invariant 3 explicitly names both sites; (b) lease-time only — rejected: cannot catch revoked-mid-lease; (c) gate inside the broker — rejected: the broker is a pure mint engine, authorization is a boundary concern (Invariant 4); (d) fall ungranted mintables through to `secrets_map` as static (the parent spec's wording) — rejected as a VLT violation, see Prior-Art Divergence.
- **Patch-vs-refactor verdict:** **patch** — additive enforcement on shipped surfaces. Same-commit hygiene: M102_005 §2 is replaced with a pointer here so no duplicate scope survives (NDC).

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: {empty at creation}
- **Metrics review** — {empty at creation — expected: "operator-plane warns only, no analytics/funnel playbook update"}
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs`: {empty at creation}
- **Deferrals** — {empty at creation}
- **Origin (Indy + Orly, Jul 06, 2026):** carved from M102_005 §2 on the approved incident-fleet office-hours design (approach C) — the grant-gate is the single platform prerequisite for shipping the v1.0 wedge fleet to customers; the ingress/CLI/docs tail stays sequenced behind it.
