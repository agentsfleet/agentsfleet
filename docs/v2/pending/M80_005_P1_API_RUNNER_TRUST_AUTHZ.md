# M80_005: Operator-assigned runner trust — trust_class, workspace allowlist, trust-gated placement

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 005
**Date:** May 27, 2026
**Status:** PENDING
**Priority:** P1 — multi-tenant secret confinement: today any authenticated runner can be handed any tenant's `secrets_map`; this gates who a runner may serve.
**Categories:** API
**Batch:** B1
**Branch:** {feat/mNN-name — added when work begins}
**Depends on:** M80_001 (register handler + `runnerBearer` + `fleet.runners`), M80_002 (assignment/fencing — placement extends `assign.select`)
**Provenance:** agent-generated (Opus 4.7, May 27, 2026 — from `runner_fleet.md` S4 + the M80_001 "M80_005 narrows to authz fields + operator-trust placement" decision)

> **Provenance is load-bearing.** LLM-drafted, security-boundary spec — the implementing agent reads `docs/AUTH.md` first and cross-checks the `fleet.runners` schema + `assign.select` before touching code. TLS and the register/bearer plane already shipped (M80_001/002); this is the authz layer on top.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (S4 identity row + "placement keys off operator-assigned trust, not self-reported `sandbox_tier`").

---

## Implementing agent — read these first

1. `docs/AUTH.md` — the runner is a credential-typed principal; `runner_token` → `AuthPrincipal{mode=runner, runner_id}`. Trust/authz attaches to that principal; read the "What ships when" section.
2. `src/zombied/fleet/assign.zig` — `assign.select` is where placement happens; trust + workspace eligibility must filter **before** the sticky-routing hint (a hint must never route work a runner isn't authorized for).
3. `src/zombied/fleet/` schema for `core.runners`/`fleet.runners` + `docs/SCHEMA_CONVENTIONS.md` — the additive columns land here (append-only, single-concern, app-enforced enums).
4. `docs/architecture/runner_fleet.md` — the trust model: self-reported `sandbox_tier` is telemetry only; placement trusts operator-assigned `trust_class`.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** gate runner placement on operator-assigned trust + workspace allowlist
- **Intent (one sentence):** an operator decides which tenants a given runner may serve and at what trust level, and the control plane refuses to lease a tenant's work (and its inline secrets) to a runner outside that authorization.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent and list `ASSUMPTIONS I'M MAKING: …`. Mismatch with Intent → STOP and reconcile. Pay special attention to whether an empty `allowed_workspace_ids` means "all" or "none" — fail-closed default is "none" unless the operator marks the runner unrestricted.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (trust_class values single-sourced as named constants, shared with any client), NLG (pre-2.0: no compat shim for the pre-authz placement — replace in place).
- **`docs/ZIG_RULES.md`** — placement + middleware are `*.zig` (pg-drain on the eligibility query, tagged-union results, cross-compile).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the register/heartbeat request shapes gain authz fields additively; error envelopes for an unauthorized-workspace reject.
- **`docs/SCHEMA_CONVENTIONS.md`** — additive `fleet.runners` columns: append-only migration, single-concern, **no SQL `DEFAULT 'value'`/`CHECK IN` — enum values enforced in app via named constants**.
- **`docs/AUTH.md`** — credential-typed principal rules (auth-flow surface).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — placement + middleware `*.zig` | cross-compile; pg-drain audit on the eligibility query |
| SCHEMA GUARD | yes — new `fleet.runners` columns + migration | append-only, single-concern (≤100 lines), update `schema/embed.zig` + migration array; no static SQL enums |
| ERROR REGISTRY | yes — an unauthorized-workspace `UZ-RUN-*` | declare in registry before use; mirror in `client_errors.zig` if the runner observes it |
| UFS | yes — trust_class values | named constants single-sourced; no re-spelled literals |
| PUB / Struct-Shape | yes — authz fields on the principal/lease-request structs | shape verdict per changed struct |
| LOGGING | yes — placement-deny emit | logfmt with `error_code`, `runner_id`, `workspace_id`; never the secrets |

---

## Overview

**Goal (testable):** `assign.select` will not issue a lease for a workspace to a runner whose `allowed_workspace_ids` excludes it, nor route trusted-tier work to an untrusted runner — and the decision uses the operator-assigned `trust_class`, never the runner's self-reported `sandbox_tier` — asserted by `test_placement_denies_unauthorized_workspace` + `test_placement_ignores_self_reported_tier`.

**Problem:** post-M80_002 a runner authenticates (`runnerBearer`) and then any runner can be assigned any active zombie's event — which hands that tenant's inline `secrets_map` to a host the operator never authorized for that tenant. In a mixed-trust fleet (shared + dedicated + local-dev runners) that is a cross-tenant secret-exposure path.

**Solution summary:** add operator-assigned `trust_class` and `allowed_workspace_ids` to `fleet.runners` (set by the operator, not self-reported), and make `assign.select` filter eligibility on both **before** the sticky-routing hint. Self-reported `sandbox_tier` stays telemetry. A runner that requests/receives work outside its authorization is denied with a registry error; the event waits for an eligible runner.

---

## Prior-Art / Reference Implementations

- **API** → `src/zombied/fleet/assign.zig` (the existing sticky + any-eligible selection) — trust/workspace filtering wraps it; `src/zombied/auth/middleware/runner_bearer.zig` for the principal shape.
- **Schema** → the M80_001 `fleet.runners` migration + `docs/SCHEMA_CONVENTIONS.md`; columns added the same append-only way.
- **Reference concept** → `project_local_runner_affinity_trust_scope` (memory): sticky affinity for local/untrusted runners must filter on trust+scope before the hint, not route prod/other-tenant work blindly.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/*.sql` (a new migration) | CREATE | additive `trust_class`, `allowed_workspace_ids` on `fleet.runners`; append-only |
| `schema/embed.zig` | EDIT | register the new migration in the array |
| `src/zombied/fleet/assign.zig` | EDIT | eligibility filter on trust_class + allowed_workspace_ids, applied before the sticky hint |
| `src/zombied/fleet/` register/heartbeat handler | EDIT | accept operator-set authz fields; never let a runner self-assign trust |
| `src/lib/contract/protocol.zig` | EDIT | additive authz fields on the register request (additive only — frozen contract) |
| `src/zombied/errors/error_entries.zig`, `error_registry.zig` | EDIT | unauthorized-workspace `UZ-RUN-*` |
| `src/zombied/state/*trust*.zig` | CREATE | `TrustClass` enum + named constants (app-enforced, not SQL) |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** schema + placement, two slices, both extending M80_002's assignment rather than rewriting it.
- **Alternatives considered:** (a) zero-trust scoped/proxied secrets now — rejected (out of scope; the cutover target is trusted-fleet inline, scoped/proxy is later per M80_002 alternatives); (b) trust derived from self-reported `sandbox_tier` — rejected, a runner can lie about its tier, so trust MUST be operator-assigned.
- **Patch-vs-refactor verdict:** **additive refactor of placement** — `assign.select` gains an eligibility predicate; the selection algorithm is otherwise unchanged. Schema is purely additive.

---

## Sections (implementation slices)

### §1 — Trust + workspace authz fields (operator-assigned)

Delivers the data model: `trust_class` and `allowed_workspace_ids` on `fleet.runners`, set by an operator (admin-authenticated), never self-asserted by the runner. **Implementation default:** `allowed_workspace_ids` empty ⇒ the runner is **unrestricted only if** its row is explicitly marked unrestricted; otherwise empty means "no workspaces" (fail-closed) — because silently treating empty as "all" is the exact cross-tenant hole this closes.

- **Dimension 1.1** — `trust_class` + `allowed_workspace_ids` columns exist, set only via an admin-authenticated path; a runner's own register/heartbeat cannot raise its trust → Test `test_runner_cannot_self_assign_trust`
- **Dimension 1.2** — `TrustClass` is an app-level enum of named constants (no SQL `CHECK IN`/`DEFAULT`) → Test `test_trust_class_values_app_enforced`

### §2 — Trust-gated placement

Delivers the enforcement: `assign.select` filters candidate runners by `allowed_workspace_ids` ∋ the event's workspace AND a `trust_class` adequate for the work, **before** applying the sticky-routing hint. Self-reported `sandbox_tier` is never consulted for placement.

- **Dimension 2.1** — a lease is never issued for a workspace outside the runner's `allowed_workspace_ids` → Test `test_placement_denies_unauthorized_workspace`
- **Dimension 2.2** — the eligibility filter runs before the sticky hint; a sticky-preferred but unauthorized runner is skipped, work goes to an eligible one → Test `test_trust_filter_precedes_sticky_hint`
- **Dimension 2.3** — placement decisions ignore the runner's self-reported `sandbox_tier` (telemetry only) → Test `test_placement_ignores_self_reported_tier`

---

## Interfaces

```
fleet.runners (additive columns):
  trust_class           text     -- operator-assigned; app-enforced enum (TrustClass)
  allowed_workspace_ids uuid[]   -- operator-assigned; empty = none unless `unrestricted` is set
  unrestricted          boolean  -- operator opt-in for fleet-wide runners

POST /v1/runners (register) — request gains NOTHING runner-self-assignable for trust;
  trust_class / allowed_workspace_ids are set via an admin-authenticated operator path only.

assign.select(...) eligibility predicate (internal):
  candidate eligible ⇔ (unrestricted OR workspace_id ∈ allowed_workspace_ids)
                       AND trust_class adequate for the event's required trust
  applied BEFORE the last_runner_id sticky hint.

errors (new): UZ-RUN-008 runner_not_authorized_for_workspace (placement/report reject)
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Runner requests work outside its allowlist | mixed-trust fleet, workspace not allowed | not selected; if it somehow reports such a lease, report rejects (`UZ-RUN-008`); event waits for an eligible runner |
| Runner self-asserts higher trust | malicious/buggy runner sets trust in register | ignored — trust is operator-assigned only; register cannot raise it → `test_runner_cannot_self_assign_trust` |
| Empty allowlist | misconfigured runner | fail-closed: no workspaces eligible unless `unrestricted`; no silent "all" |
| No eligible runner for a workspace | all runners unauthorized | event stays unleased (no lease issued); operator-visible via fleet inventory (M80_006); no cross-tenant fallback |
| Self-reported tier spoofed | runner claims `landlock_full` while `dev_none` | placement ignores it (telemetry only); trust comes from operator assignment → `test_placement_ignores_self_reported_tier` |

---

## Invariants

1. Placement never consults self-reported `sandbox_tier` — enforced by `assign.select` reading only operator-assigned columns + `test_placement_ignores_self_reported_tier` (the test seeds a spoofed tier and asserts no effect).
2. A runner can never raise its own `trust_class`/`allowed_workspace_ids` — enforced by the register/heartbeat handler not reading those fields from the runner-authenticated request (only an admin path writes them) + `test_runner_cannot_self_assign_trust`.
3. Empty `allowed_workspace_ids` ⇒ no eligibility (unless `unrestricted`) — enforced in the eligibility predicate (fail-closed branch) + `test_placement_denies_unauthorized_workspace`.
4. Trust enum values are app-enforced named constants, not SQL strings — enforced by the SCHEMA gate (no `CHECK IN`/`DEFAULT 'value'`) + `test_trust_class_values_app_enforced`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_runner_cannot_self_assign_trust` | register/heartbeat from a runner with trust fields set → stored row unchanged; only the admin path mutates them |
| 1.2 | unit | `test_trust_class_values_app_enforced` | every persisted `trust_class` maps to a `TrustClass` constant; schema has no CHECK/DEFAULT enum |
| 2.1 | integration | `test_placement_denies_unauthorized_workspace` | event for workspace W, runner allowlist ∌ W → no lease issued to it |
| 2.2 | integration | `test_trust_filter_precedes_sticky_hint` | sticky-preferred runner unauthorized for W → skipped; an eligible runner gets the lease |
| 2.3 | integration | `test_placement_ignores_self_reported_tier` | runner reports `landlock_full` but is operator-untrusted → not selected for trusted work |

Regression: M80_002's `test_sticky_routing_is_hint_not_ownership` + `test_lease_assigns_across_active_zombies` stay green (the filter narrows candidates; it must not break selection when all are eligible). Replay: N/A.

---

## Acceptance Criteria

- [ ] Unauthorized-workspace lease impossible — verify: `test_placement_denies_unauthorized_workspace`
- [ ] Trust is operator-only — verify: `test_runner_cannot_self_assign_trust`
- [ ] Self-reported tier ignored for placement — verify: `test_placement_ignores_self_reported_tier`
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes
- [ ] `make check-pg-drain` clean (eligibility query drains) · cross-compile both linux targets
- [ ] `gitleaks detect` clean · no file over 350 lines added · schema migration append-only

---

## Eval Commands (post-implementation)

```bash
# E1: placement authz — make test-integration 2>&1 | grep -E "unauthorized_workspace|self_assign_trust|ignores_self_reported_tier|PASS|FAIL"
# E2: Build  — zig build
# E3: Tests  — make test && make test-integration
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: pg-drain — make check-pg-drain 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted. The pre-authz `assign.select` body is replaced in place by the trust-gated version (RULE NLR), not left beside a `_v2`.

---

## Discovery (consult log)

> **Empty at creation.** Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- **Provenance (May 27, 2026):** authored with M80_003/004/006 to formalize the remaining runner-rollout roadmap before M80_002's CHORE(close). The trusted-fleet cutover (M80_002) ships inline secrets without this authz; M80_005 is the hardening for mixed-trust fleets.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits coverage vs this Test Specification (esp. the fail-closed empty-allowlist arm) | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | adversarial review vs AUTH.md, the cross-tenant-secret threat, REST guide | clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | review-comments the open PR | comments addressed before merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Integration (authz) | `make test-integration` | {paste at VERIFY} | |
| pg-drain | `make check-pg-drain` | {paste at VERIFY} | |
| Lint | `make lint` | {paste at VERIFY} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste at VERIFY} | |

---

## Out of Scope

- Zero-trust scoped/proxied secret delivery (vs inline) — future, beyond the trusted-fleet model.
- Fleet inventory / revoke / heartbeat reassignment — M80_006.
- Capacity/label placement + autoscale — M80_007.
