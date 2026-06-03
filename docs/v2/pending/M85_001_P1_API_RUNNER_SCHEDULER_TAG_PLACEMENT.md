# M85_001: Runner scheduler — place a zombie only on a runner whose labels satisfy its required tags

**Prototype:** v2.0.0
**Milestone:** M85
**Workstream:** 001
**Date:** Jun 04, 2026
**Status:** PENDING
**Priority:** P1 — correctness + capability: today any runner claims any zombie, so GPU/region/trust-bound work can land on an unfit host.
**Categories:** API
**Batch:** B1 — single workstream; the eligibility filter is one coherent change to the lease claim.
**Branch:** {feat/m85-runner-scheduler — added when work begins}
**Depends on:** none hard — the lease-claim race (`fleet.runner_affinity`) and runner `labels` already exist; composes with M84_002 (`admin_state`/liveness as additional eligibility inputs) but does not require it.
**Provenance:** agent-generated (Indy CTO consult, Jun 04 2026 — authored as a design artifact in PR `feat/m84-dashboard-runner-enrollment`; **not implemented there**).

**Canonical architecture:** `docs/architecture/runner_fleet.md` (the "not a general scheduler" non-goal this spec reconciles to *"placement is M85_001"*; the lease/affinity model) + `docs/architecture/roadmap.md` (the eligibility-filter / scheduler entry). This spec is what flips label/capacity/sandbox-tier placement from a documented non-goal to a built capability — it supersedes the **stale `M80_007` placement reservation** (`M80_007` shipped as the runner-observability spec).

---

## Implementing agent — read these first

1. `docs/architecture/runner_fleet.md` — the **non-goals fence** ("not a general scheduler; placement is capped at sticky + any-eligible") this spec deliberately moves; the lease/fence/affinity model the eligibility filter slots into; the *Local-runner affinity trust scope* note (trust + scope must filter **before** the sticky hint).
2. `schema/023_fleet_runner_affinity.sql` + `src/zombied/fleet/{assign,affinity}.zig` — the per-zombie lease **claim** (the conditional UPSERT that wins iff the slot is free). The eligibility filter wraps the candidate set this claim races over; the sticky `last_runner_id` hint stays a preference *within* the eligible set.
3. `schema/021_fleet_runners.sql` + `src/zombied/http/handlers/runner/register.zig` — runner `labels` (free-form JSONB, advertised at enrollment, already stored) + `sandbox_tier` + `tenant_id` scope — the runner-side inputs to the match.
4. `src/zombied/http/handlers/zombies/create.zig` + the `core.zombies` schema — where a zombie's `required_tags` is set; the data plane the scheduler reads required tags from.
5. `docs/REST_API_DESIGN_GUIDELINES.md` — the zombie create/config surface gains a `required_tags` field; no new runner-facing route (the runner advertises labels at enrollment, never re-sends them per `/leases` poll).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Runner scheduler — match a zombie's required tags against runner labels before claim
- **Intent (one sentence):** Make a zombie run only on a runner whose advertised labels satisfy the zombie's required tags (GitLab-tags / GitHub-labels model), so GPU/region/trust-bound work lands on a capable host instead of the first free runner.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Key assumption to confirm: the match is **server-side at claim time** (`required_tags ⊆ runner.labels`), composed with trust/scope/sandbox-tier **before** the sticky hint; the runner does **not** re-advertise tags per poll. A mismatch with the Intent → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (the match operator + label keys are named consts shared verbatim where the runner and zombie sides both reference a well-known label), NDC, ORP (the affinity claim query is touched — sweep stale call sites).
- **`docs/ZIG_RULES.md`** — the claim is `*.zig` + SQL: pg-drain on any new `conn.query`, tagged-union result, cross-compile; the eligibility predicate must stay inside the atomic claim (no read-then-claim race).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the zombie create/config surface gains `required_tags`; request validation + error envelope.
- **`docs/SCHEMA_CONVENTIONS.md`** — `core.zombies.required_tags` column (JSONB array, never NULL, app-enforced — RULE STS, no static CHECK); single-concern migration.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets; the eligibility predicate is part of the atomic claim. |
| SCHEMA | yes | one migration adds `core.zombies.required_tags` (JSONB, app-enforced); update `schema/embed.zig` + migration array. |
| LIFECYCLE | yes | any new `conn.query` in the claim path drains before release. |
| UFS | yes | label-match operator + any well-known label keys single-sourced; no re-spelled `required_tags` literal. |
| ERROR REGISTRY | yes | a "no eligible runner" hold is **not** an error code (work waits, not fails); a malformed `required_tags` is `UZ-REQ-001`. |
| File & Function Length | yes | factor the eligibility predicate out of the claim fn (≤50 lines). |

---

## Overview

**Goal (testable):** A zombie carrying `required_tags = [gpu, us-east]` is leased only by a runner whose `labels` ⊇ `{gpu, us-east}`; a runner missing either label never claims it; a zombie with empty `required_tags` is claimable by any eligible runner (today's behaviour, preserved); the eligibility match is enforced **inside** the atomic lease claim, not by a pre-read.

**Problem:** Runner `labels` are stored at enrollment but **no assignment code reads them** — the lease claim is a global race where *any eligible runner may claim any zombie*. So an operator cannot pin GPU work to GPU hosts, region work to a region, or trusted-tier work to a trusted runner — the exact thing GitLab (tags) and GitHub (labels) exist to do. `runner_fleet.md` names this gap as the reserved scheduler milestone.

**Solution summary:** Add `required_tags` to the zombie/job model (data plane), and an **eligibility filter** to the lease claim (control plane): a runner may claim a zombie only if `required_tags ⊆ runner.labels` **and** the runner satisfies trust class, registration scope, and sandbox tier — composed **before** the sticky `last_runner_id` hint so the hint never overrides eligibility. The runner advertises its labels once at enrollment (unchanged); `zombied` does the match server-side at claim time. When no runner is eligible, the work **holds** (it is not failed or thrashed) until a capable runner appears.

---

## Prior-Art / Reference Implementations

- **API/claim** → `src/zombied/fleet/{assign,affinity}.zig` + `schema/023_fleet_runner_affinity.sql` — the existing conditional-UPSERT claim; the eligibility predicate composes into its `WHERE`, the sticky hint stays a tiebreak within the eligible set.
- **Data plane** → `src/zombied/http/handlers/zombies/{create,patch}.zig` + `core.zombies` — where `required_tags` is written and validated.
- **Model reference** → GitLab-16 runner **tags** (job runs only on a runner with all tags; untagged-job pickup is an opt-in) and GitHub Actions **labels** (`runs-on` matches all labels). usezombie adopts the subset relation `required_tags ⊆ labels`; capacity/fairness scheduling is explicitly **not** adopted (see Out of Scope).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/0NN_zombie_required_tags.sql` | CREATE | `core.zombies.required_tags` JSONB array (never NULL, default empty in app code). |
| `schema/embed.zig` + migration array | EDIT | Register the new migration. |
| `src/zombied/http/handlers/zombies/{create,patch}.zig` | EDIT | Accept + validate `required_tags` on zombie create/config. |
| `src/zombied/fleet/assign.zig` (+ `affinity.zig`) | EDIT | Add the eligibility predicate (`required_tags ⊆ labels` ∧ trust ∧ scope ∧ tier) to the atomic claim, before the sticky hint. |
| `src/lib/contract/protocol.zig` | EDIT | `required_tags` on the zombie/lease shape where the contract carries it. |
| `docs/architecture/runner_fleet.md` | EDIT | Move the "no scheduler" line to *"placement (label/capacity/sandbox-tier) is M85_001"*; describe the eligibility-before-hint ordering. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, four Sections — the data-plane field (§1), the control-plane eligibility filter (§2), the no-eligible-runner hold (§3), and doc reconciliation (§4). The filter is a surgical addition to an existing atomic claim, not a new scheduler subsystem.
- **Alternatives considered:** (a) a separate scheduler service that pushes work to runners — **rejected**: inverts the pull/lease model the fleet is built on and reintroduces a control-plane the non-goals fence forbids. (b) runner re-advertises tags on every `/leases` poll — **rejected**: the server already holds the runner's labels from enrollment; re-sending is redundant and spoofable.
- **Patch-vs-refactor verdict:** **patch** — extend the existing claim predicate + add one data-plane field. The larger "capacity/fairness scheduler" is deliberately a *different, later* milestone, not this one.

---

## Sections (implementation slices)

### §1 — Zombie `required_tags` (data plane)

A zombie carries `required_tags` (a set, possibly empty). Empty = any eligible runner (preserves today's behaviour); non-empty = only runners whose labels are a superset. **Implementation default:** validate as a deduped string set with the same character class as runner labels, so a typo can't silently never-match a real label.

- **Dimension 1.1** — a zombie persists `required_tags` from create/config; empty default when omitted → Test `zombie persists required tags with empty default`.
- **Dimension 1.2** — malformed `required_tags` (non-array / oversize / bad chars) is rejected `UZ-REQ-001` → Test `zombie rejects malformed required tags`.

### §2 — Eligibility filter in the lease claim (control plane)

The claim races only runners where `required_tags ⊆ runner.labels` **and** trust class, registration scope (`tenant_id`), and sandbox tier fit — composed **before** the sticky `last_runner_id` hint. The match is inside the atomic claim (no read-then-claim TOCTOU). **Implementation default:** evaluate the subset relation in the claim SQL against the runner's stored `labels` JSONB.

- **Dimension 2.1** — a zombie with `required_tags=[gpu]` is claimed only by a runner whose labels include `gpu`; a non-`gpu` runner never wins it → Test `claim respects required tag subset`.
- **Dimension 2.2** — eligibility composes before the hint: a sticky `last_runner_id` that no longer satisfies the tags does **not** win the claim → Test `sticky hint never overrides eligibility`.
- **Dimension 2.3** — trust/scope/sandbox-tier still gate (a weak-tier or other-tenant runner is ineligible regardless of labels) → Test `eligibility composes trust scope and tier`.

### §3 — No-eligible-runner hold

If no runner satisfies a zombie's tags, the work **holds** — it is neither failed nor thrashed across ineligible hosts — and becomes claimable the moment a capable runner enrolls/returns. **Implementation default:** the zombie simply remains unclaimed (its lease slot stays free); no error event, no dead-letter.

- **Dimension 3.1** — a zombie whose tags no current runner satisfies stays unclaimed (no error, no failed state), then is claimed once a matching runner appears → Test `unsatisfiable tags hold then schedule`.

### §4 — Reconcile the scheduler non-goal in the architecture docs

`runner_fleet.md`/`roadmap.md` describe label/capacity/sandbox-tier placement as **built here (M85_001)**, not a non-goal, and not the stale `M80_007`. **Invariant:** the non-goals fence still forbids capacity/fairness/autoscale (those stay out).

- **Dimension 4.1** — no live doc reserves placement under `M80_007`; the non-goal reads *"until M85_001"* → Test `scheduler-doc sweep`.

---

## Interfaces

```
core.zombies.required_tags : JSONB array of label strings, never NULL (empty = any-eligible).
Zombie create/config (POST/PATCH zombies): accepts `required_tags?: string[]` (validated, deduped).
Lease claim (fleet.assign): eligibility predicate
    required_tags ⊆ runner.labels  ∧  trust_ok  ∧  scope_ok(tenant_id)  ∧  tier_ok(sandbox_tier)
  evaluated inside the atomic claim; sticky last_runner_id is a tiebreak WITHIN the eligible set.
Runner side: UNCHANGED — labels advertised once at enrollment; no per-poll tag payload.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| No eligible runner | tags no runner satisfies | work **holds** (lease slot stays free); no error/dead-letter; schedules when a match appears (§3). |
| Label typo on the zombie | `required_tags=[gpuu]` | validates as a tag but matches no runner → holds (§3); surfaced via the unscheduled-zombie view, not a hard error. |
| Sticky hint now ineligible | runner's labels/trust changed | hint is skipped; another eligible runner claims; correctness never blocks on the hint (§2.2). |
| Other-tenant / weak-tier runner has the label | label match but trust/scope/tier fails | ineligible — never claims (§2.3). |
| Malformed `required_tags` | bad client input | `400 UZ-REQ-001`; zombie not created/updated (§1.2). |
| Concurrent claims race | N runners eligible | the existing atomic UPSERT still admits exactly one; eligibility narrows the field, fencing unchanged. |

---

## Invariants

1. **`required_tags ⊆ runner.labels` is enforced in the claim SQL**, not by a pre-read — a read-then-claim gap is forbidden (TOCTOU) — enforced by §2.1 + the atomic-claim test.
2. **Empty `required_tags` ⇒ any eligible runner** (back-compat with today's global race) — enforced by §1.1 + §2.
3. **Eligibility composes before the sticky hint** — a hint that fails eligibility never wins — enforced by §2.2.
4. **Trust/scope/sandbox-tier remain hard gates** independent of labels — enforced by §2.3.
5. **Unsatisfiable tags hold, never fail/thrash** — enforced by §3.1.
6. **No capacity/fairness scheduling enters** — the predicate is a boolean eligibility match only; no scoring/bin-packing — enforced by review against this Invariant + the non-goals fence.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `zombie persists required tags with empty default` | create with/without `required_tags` → stored set / empty. |
| 1.2 | unit | `zombie rejects malformed required tags` | non-array / oversize / bad chars → `400 UZ-REQ-001`. |
| 2.1 | integration | `claim respects required tag subset` | `required_tags=[gpu]`; gpu-runner claims, non-gpu never does. |
| 2.2 | integration | `sticky hint never overrides eligibility` | stale `last_runner_id` now missing the tag → an eligible runner wins. |
| 2.3 | integration | `eligibility composes trust scope and tier` | labelled but weak-tier/other-tenant runner → ineligible. |
| 3.1 | integration | `unsatisfiable tags hold then schedule` | no match → unclaimed (no error); matching runner enrolls → claimed. |
| 4.1 | regression | `scheduler-doc sweep` | no live `M80_007` placement ref; non-goal reads "until M85_001". |

**Regression:** the existing lease/fence/reclaim suite stays green (empty-tags path unchanged). **Idempotency:** the atomic claim still admits exactly one runner under concurrency.

---

## Acceptance Criteria

- [ ] `required_tags` persisted + validated — verify: `make test-integration`
- [ ] Claim respects `required_tags ⊆ labels`, composed before the hint, with trust/scope/tier gates — verify: `make test-integration`
- [ ] Unsatisfiable tags hold (no error/thrash) then schedule on a match — verify: `make test-integration`
- [ ] `make lint` clean · `make test` passes · cross-compile both linux targets
- [ ] No live `M80_007` placement reference; non-goal reconciled to M85_001 — verify: `grep -rn "M80_007" docs/architecture/ | grep -i placement`
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: required-tags subset claim + eligibility ordering
make test-integration 2>&1 | grep -iE "required tag|eligibility|sticky" | tail -10
# E2: Build + cross-compile
zig build && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo PASS || echo FAIL
# E3: Lint
make lint 2>&1 | grep -E "✓|FAIL"
# E4: scheduler-doc sweep (placement no longer M80_007)
grep -rn "M80_007" docs/architecture/ | grep -i "placement\|scheduler\|label"
# E5: Gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted (additive: one column, one predicate, one validated field).

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults/decisions.

- **Provenance (Jun 04 2026)** — authored in PR `feat/m84-dashboard-runner-enrollment` as a design artifact after Indy's CTO consult on runner state + routing. The GitLab-tags / GitHub-labels model was the reference; usezombie adopts `required_tags ⊆ labels` and explicitly **not** capacity/fairness scheduling. Indy: *"Agree to have a spec written for tag based routing and the spec is pushed as part of this PR itself."*
- **Numbering (Jun 04 2026)** — placement was reserved as `M80_007` in `runner_fleet.md`, but `M80_007` is the shipped runner-observability spec — a real ID collision. This spec is `M85_001`; M84_001 corrects the stale `M80_007` placement refs.
- **Deferrals** — populate during implementation; none at authoring.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Coverage vs the eligibility matrix (subset, hint-override, trust/scope/tier, hold). | Clean; count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial review vs spec, ZIG_RULES, the atomic-claim invariant, the non-goals fence. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Required-tags model | `make test-integration` | {paste} | |
| Eligibility claim | `make test-integration` | {paste} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste} | |
| Lint | `make lint` | {paste} | |
| Doc sweep | `grep -rn "M80_007" docs/architecture/` | {paste} | |

---

## Out of Scope

- **Capacity / fairness / bin-packing scheduling** — the match is a boolean eligibility predicate, not a scorer; weighted placement is a later, separate milestone (and still fenced by the non-goals until a spec changes it).
- **Autoscale / provisioning runners to satisfy demand** — out; the fleet does not create capacity.
- **Runner-side tag re-advertisement per poll** — labels are advertised once at enrollment; not re-sent on `/leases`.
- **Dynamic relabeling of a live runner** — changing a runner's labels after enrollment (and re-evaluating in-flight leases) is future work.
- **Operator plane (cordon/drain/revoke) + event log** — that is M84_002; `admin_state`/liveness compose as eligibility inputs but are defined there.
