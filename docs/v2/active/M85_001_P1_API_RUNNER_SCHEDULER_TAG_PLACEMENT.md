# M85_001: Runner scheduler — place a zombie only on a runner whose labels satisfy its required tags

**Prototype:** v2.0.0
**Milestone:** M85
**Workstream:** 001
**Date:** Jun 04, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — correctness + capability: today any runner claims any zombie, so capability-bound (GPU/region) work can land on an unfit host.
**Categories:** API
**Batch:** B1 — single workstream; the eligibility filter is one coherent change to the candidate scan.
**Branch:** feat/m85-runner-scheduler
**Depends on:** none — the lease-claim race (`fleet.runner_affinity`) and runner `labels` already exist.
**Provenance:** agent-generated (Indy CTO consult, Jun 04 2026 — authored as a design artifact in PR `feat/m84-dashboard-runner-enrollment`; **not implemented there**). Scope reduced to a single label gate via `/plan-ceo-review` on Jun 06 2026 (see Discovery).

**Canonical architecture:** `docs/architecture/runner_fleet.md` (the "not a general scheduler" non-goal this spec reconciles to *"placement is M85_001"*; the lease/affinity model) + `docs/architecture/roadmap.md` (the eligibility-filter / scheduler entry). This spec flips **label** placement from a documented non-goal to a built capability — it supersedes the **stale `M80_007` placement reservation** (`M80_007` shipped as the runner-observability spec). Capacity/fairness/trust-tier placement stays a non-goal here (see Out of Scope).

---

## Implementing agent — read these first

1. `docs/architecture/runner_fleet.md` — the **non-goals fence** ("not a general scheduler; placement is capped at sticky + any-eligible") this spec partially moves (label placement only); the lease/fence/affinity model the eligibility filter slots into.
2. `schema/023_fleet_runner_affinity.sql` + `src/zombied/fleet/{assign,affinity}.zig` — the per-zombie lease **claim** (the conditional UPSERT that wins iff the slot is free). The eligibility filter narrows the **candidate set** (`listCandidates`) this claim races over; the sticky `last_runner_id` hint stays a preference *within* the eligible set. The claim UPSERT is **untouched**.
3. `schema/021_fleet_runners.sql` — runner `labels` (free-form JSONB, advertised at enrollment, already stored) — the runner-side input to the match.
4. `src/zombied/http/handlers/zombies/{create,patch}.zig` + the `core.zombies` schema — where a zombie's `required_tags` is set; the data plane the scheduler reads required tags from.
5. `docs/REST_API_DESIGN_GUIDELINES.md` — the zombie create/config surface gains a `required_tags` field; no new runner-facing route (the runner advertises labels at enrollment, never re-sends them per `/leases` poll).

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Runner scheduler — match a zombie's required tags against runner labels before claim
- **Intent (one sentence):** Make a zombie run only on a runner whose advertised labels satisfy the zombie's required tags (GitLab-tags / GitHub-labels model), so capability-bound work lands on a capable host instead of the first free runner.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Key assumption to confirm: the match is **a single label gate** (`required_tags ⊆ runner.labels`) evaluated **server-side as a candidate-set filter** in `listCandidates`; the slot claim stays atomic and untouched; the runner does **not** re-advertise tags per poll. A mismatch with the Intent → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (the match operator + any well-known label key are named consts, no re-spelled `required_tags` literal), NDC, ORP (the candidate scan is touched — sweep stale call sites).
- **`docs/ZIG_RULES.md`** — the change is `*.zig` + SQL: pg-drain on any new/changed `conn.query`, tagged-union result, cross-compile both linux targets.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the zombie create/config surface gains `required_tags`; request validation + error envelope.
- **`docs/SCHEMA_CONVENTIONS.md`** — `core.zombies.required_tags` column (JSONB array, never NULL, app-enforced — RULE STS, no static DEFAULT/CHECK); single-concern migration.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets; eligibility filter factored ≤50 lines. |
| SCHEMA | yes | add `core.zombies.required_tags` by editing 007 in place (teardown-rebuild era, no `ALTER`). `'[]'::jsonb` structural default = the meter_slice_seq exception class, not an STS enum default. |
| LIFECYCLE | yes | the `listCandidates` query already drains via `PgQuery`; the added JOIN keeps that path. |
| UFS | yes | label-match operator single-sourced; no re-spelled `required_tags` literal. |
| ERROR REGISTRY | yes | a "no eligible runner" hold is **not** an error code (work waits, not fails); a malformed `required_tags` is `UZ-REQ-001`. |
| File & Function Length | yes | factor the eligibility predicate out of `listCandidates` (≤50 lines). |

---

## Overview

**Goal (testable):** A zombie carrying `required_tags = [gpu, us-east]` is leased only by a runner whose `labels` ⊇ `{gpu, us-east}`; a runner missing either label never claims it; a zombie with empty `required_tags` is claimable by any runner (today's behaviour, preserved); the match is enforced as a **filter on the candidate set** in `listCandidates`, leaving the atomic slot claim untouched.

**Problem:** Runner `labels` are stored at enrollment but **no assignment code reads them** — `listCandidates` lists every active zombie and the claim reads nothing about the host. So an operator cannot pin GPU work to GPU hosts or region work to a region — the exact thing GitLab (tags) and GitHub (labels) exist to do. `runner_fleet.md` names this gap as the reserved scheduler milestone.

**Solution summary:** Add `required_tags` to the zombie model (data plane), and a **single label-eligibility filter** to the candidate scan (control plane): a runner may claim a zombie only if `required_tags ⊆ runner.labels`. The match is a candidate-set filter (`JOIN fleet.runners` + JSONB containment `<@`), not a change to the claim UPSERT — sound because `required_tags` (set at create) and `labels` (set at enrollment; relabeling is out of scope) are **immutable for the work's lifetime**, so there is no check-then-claim window. The runner advertises its labels once at enrollment (unchanged); `zombied` does the match server-side. When no runner is eligible, the work **holds** (it is not failed or thrashed) until a capable runner appears.

This ships **one gate**. Trust-class, registration-scope, and sandbox-tier eligibility — and the funnel that composes them — are deferred to a follow-up spec (see Out of Scope). The predicate is written as a composable filter so those gates slot in later without reshaping it.

---

## Prior-Art / Reference Implementations

- **API/claim** → `src/zombied/fleet/assign.zig` (`listCandidates`) + `affinity.zig` — the candidate scan + existing conditional-UPSERT claim; the eligibility predicate composes into the scan's `WHERE`, the sticky hint stays a tiebreak within the eligible set, the claim UPSERT is unchanged.
- **Data plane** → `src/zombied/http/handlers/zombies/{create,patch}.zig` + `core.zombies` — where `required_tags` is written and validated.
- **Model reference** → GitLab-16 runner **tags** (job runs only on a runner with all tags; untagged-job pickup is an opt-in) and GitHub Actions **labels** (`runs-on` matches all labels). usezombie adopts the subset relation `required_tags ⊆ labels`; capacity/fairness scheduling is explicitly **not** adopted (see Out of Scope).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/007_core_zombies.sql` | EDIT | Add `core.zombies.required_tags JSONB NOT NULL DEFAULT '[]'::jsonb`. Edited in place (pre-v2.0.0 teardown-rebuild era forbids `ALTER`; no schema has one). No `embed.zig`/migration-array change — 007 is already registered. |
| `src/zombied/http/handlers/zombies/{create,patch}.zig` | EDIT | Persist `skill_meta.tags` → `required_tags` (create on insert; patch re-derives when `source_markdown` reparses), bounds-validated → `UZ-REQ-001`. |
| `src/zombied/http/handlers/zombies/create_stream.zig` | CREATE | Event-stream-setup concern extracted from create.zig to stay ≤350 (RULE FLL); create.zig is the sole consumer. |
| `src/zombied/zombie/config_types.zig` (+ `config.zig` re-export) | EDIT | `validRequiredTags` bounds helper (shared by create+patch) + corrected `tags` doc (no longer "uninterpreted"). |
| `src/zombied/fleet/assign.zig` | EDIT | Add the label-eligibility filter (`JOIN fleet.runners` + `required_tags <@ labels`) to `listCandidates`. Claim UPSERT untouched. |
| `src/zombied/fleet/placement_eligibility_test.zig` (+ `tests.zig` discovery) | CREATE | Integration tests: subset, empty=any, sticky-never-overrides, hold-then-schedule. |
| `docs/architecture/runner_fleet.md` | NO CHANGE | The non-goal line already reads *"Label placement … is built in M85_001"* — §4 satisfied by the existing text. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, four Sections — the data-plane field (§1), the control-plane label filter (§2), the no-eligible-runner hold (§3), and doc confirmation (§4). The filter is a surgical addition to the existing candidate scan, not a new scheduler subsystem.
- **Alternatives considered:** (a) push the subset check **inside** the claim UPSERT (`ON CONFLICT … WHERE`) — **rejected**: the UPSERT's WHERE sees only the affinity row + EXCLUDED, so pulling `core.zombies.required_tags` and `fleet.runners.labels` in needs an `INSERT…SELECT` restructure that tangles the fencing bump, and it guards a check-then-claim race that cannot occur (tags/labels immutable for the work's lifetime). (b) a separate scheduler service that pushes work — **rejected**: inverts the pull/lease model. (c) runner re-advertises tags per poll — **rejected**: server already holds the labels; re-sending is redundant and spoofable.
- **Patch-vs-refactor verdict:** **patch** — extend the candidate scan + add one data-plane field. The trust/scope/tier funnel and the capacity/fairness scheduler are deliberately *different, later* milestones.

---

## Sections (implementation slices)

### §1 — Zombie `required_tags` (data plane) [S1, S3]

A zombie carries `required_tags` (a set, possibly empty). Empty = any runner (preserves today's behaviour); non-empty = only runners whose labels are a superset. `required_tags` is **derived from the SKILL.md frontmatter `tags:`** the author already writes (`skill_meta.tags`, previously parsed-then-discarded) — no new client-supplied API field, so the user declares tags once in the manifest they already author. **Implementation default:** validate bounds only (≤32 tags, each 1..64 chars) → `UZ-REQ-001`; char-class is intentionally unchecked because runner labels are not validated either and the match is exact-string (a bad-char tag simply never matches). Dedup is unnecessary — `<@` is set-semantic.

- **Dimension 1.1** — a zombie persists `required_tags` from create/config; empty default when omitted → Test `zombie persists required tags with empty default`.
- **Dimension 1.2** — malformed `required_tags` (non-array / oversize / bad chars) is rejected `UZ-REQ-001` → Test `zombie rejects malformed required tags`.

### §2 — Label-eligibility filter in the candidate scan (control plane) [S2]

`listCandidates` joins the polling runner's `fleet.runners` row and admits a zombie only where `required_tags ⊆ runner.labels`, evaluated as JSONB containment (`z.required_tags <@ r.labels`). The sticky `last_runner_id` hint stays a tiebreak **within** the eligible set. The slot claim (`affinity.claim`) is **unchanged**.

- **Dimension 2.1** — a zombie with `required_tags=[gpu]` is claimed only by a runner whose labels include `gpu`; a non-`gpu` runner never wins it → Test `claim respects required tag subset`.
- **Dimension 2.2** — eligibility filters before the hint: a sticky `last_runner_id` whose labels no longer satisfy the tags does **not** surface the zombie to that runner; an eligible runner wins → Test `sticky hint never overrides eligibility`.

### §3 — No-eligible-runner hold [S2, free]

If no runner satisfies a zombie's tags, the work **holds** — it is neither failed nor thrashed across ineligible hosts — and becomes claimable the moment a capable runner enrolls/returns. **Implementation default:** the zombie simply never enters any ineligible runner's candidate set; its lease slot stays free; no error event, no dead-letter. **Known open risk (accepted):** an unsatisfiable hold is currently **silent** — no write-time match-count feedback and no held-zombie metric ship in this slice (see Out of Scope, deferred as a fast-follow).

- **Dimension 3.1** — a zombie whose tags no current runner satisfies stays unclaimed (no error, no failed state), then is claimed once a matching runner appears → Test `unsatisfiable tags hold then schedule`.

### §4 — Confirm the scheduler non-goal in the architecture docs [S8]

`runner_fleet.md`/`roadmap.md` already describe placement as **M85_001**, not the stale `M80_007`. This slice confirms the **label**-placement line is accurate to the shipped single gate (not over-claiming capacity/tier) and that the non-goals fence still forbids capacity/fairness/autoscale/trust-tier.

- **Dimension 4.1** — the `runner_fleet.md` non-goal line reads *"label placement is M85_001"* and no live doc reserves placement under `M80_007` as a pending item → Test `scheduler-doc sweep`.

---

## Interfaces

```
core.zombies.required_tags : JSONB array of label strings, never NULL (empty = any runner).
Zombie create/config (POST/PATCH zombies): accepts `required_tags?: string[]` (validated, deduped).
Candidate scan (fleet.assign.listCandidates): eligibility predicate
    required_tags ⊆ runner.labels          -- the ONLY gate this spec ships
  evaluated as `z.required_tags <@ r.labels` against the polling runner's stored labels;
  sticky last_runner_id is a tiebreak WITHIN the eligible set. The slot claim is unchanged.
Runner side: UNCHANGED — labels advertised once at enrollment; no per-poll tag payload.

Deferred (own follow-up spec — written as composable AND-clauses so they slot in):
    ∧ scope_ok(tenant_id)  ∧ trust_ok  ∧ tier_ok(required_tier)
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| No eligible runner | tags no runner satisfies | work **holds** (zombie never enters an ineligible candidate set); no error/dead-letter; schedules when a match appears (§3). |
| Label typo on the zombie | `required_tags=[gpuu]` | validates as a tag but matches no runner → **holds silently** (§3). **Accepted open risk this slice** — no write-time "matches 0 runners" feedback, no held metric; surfaced only when the agent visibly never runs. Fast-follow guardrails deferred (Out of Scope). |
| Sticky hint now ineligible | runner's labels changed (out-of-scope relabeling) | the zombie is simply not in that runner's candidate set; an eligible runner claims; correctness never blocks on the hint (§2.2). |
| Malformed `required_tags` | bad client input | `400 UZ-REQ-001`; zombie not created/updated (§1.2). |
| Concurrent claims race | N eligible runners | the existing atomic UPSERT still admits exactly one; eligibility narrows the field, fencing unchanged. |

---

## Invariants

1. **`required_tags ⊆ runner.labels` is the only placement gate** this slice ships, evaluated as a candidate-set filter in `listCandidates`. The slot claim stays atomic and **untouched** — a check-then-claim gap is harmless here because `required_tags` (create-time) and `labels` (enrollment-time; relabeling out of scope) are immutable for the work's lifetime — enforced by §2.1 + the unchanged concurrency suite.
2. **Empty `required_tags` ⇒ any runner** (back-compat with today's global race; `'[]' <@ labels` is always true) — enforced by §1.1 + §2.
3. **Eligibility filters before the sticky hint** — a hint whose runner fails eligibility never surfaces the zombie to it — enforced by §2.2.
4. **Unsatisfiable tags hold, never fail/thrash** — enforced by §3.1. (The hold is silent this slice; surfacing is a deferred fast-follow.)
5. **No capacity/fairness/trust/tier gating enters** — the predicate is a single boolean label-subset match; no scoring, no bin-packing, no second gate — enforced by review against this Invariant + the non-goals fence.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `zombie persists required tags with empty default` | create with/without `required_tags` → stored set / empty `[]`. |
| 1.2 | unit | `zombie rejects malformed required tags` | non-array / oversize / bad chars → `400 UZ-REQ-001`. |
| 2.1 | integration | `claim respects required tag subset` | `required_tags=[gpu]`; gpu-runner claims, non-gpu never does; empty-tags claimable by any. |
| 2.2 | integration | `sticky hint never overrides eligibility` | stale `last_runner_id` runner now missing the tag → an eligible runner wins. |
| 3.1 | integration | `unsatisfiable tags hold then schedule` | no match → unclaimed (no error); matching runner enrolls → claimed. |
| 4.1 | regression | `scheduler-doc sweep` | `runner_fleet.md` non-goal line reads "label placement is M85_001"; no live `M80_007` pending placement reservation. |

**Regression:** the existing lease/fence/reclaim + concurrency suite stays green (empty-tags path unchanged). **Idempotency:** the atomic claim still admits exactly one runner under concurrency.

---

## Acceptance Criteria

- [ ] `required_tags` persisted + validated (empty default; malformed → `UZ-REQ-001`) — verify: `make test-integration`
- [ ] Candidate scan respects `required_tags ⊆ labels`; empty = any runner; sticky hint never overrides — verify: `make test-integration`
- [ ] Unsatisfiable tags hold (no error/thrash) then schedule on a match — verify: `make test-integration`
- [ ] `make lint` clean · `make test` passes · cross-compile both linux targets
- [ ] `runner_fleet.md` non-goal line reads "label placement is M85_001"; no live `M80_007` *pending* placement reservation — verify: `grep -n "M85_001" docs/architecture/runner_fleet.md` shows the label-placement line
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: required-tags subset claim + empty-default + sticky ordering
make test-integration 2>&1 | grep -iE "required tag|eligibility|sticky|hold" | tail -10
# E2: Build + cross-compile
zig build && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo PASS || echo FAIL
# E3: Lint
make lint 2>&1 | grep -E "✓|FAIL"
# E4: scheduler-doc — label placement attributed to M85_001
grep -n "M85_001" docs/architecture/runner_fleet.md | grep -i "label"
# E5: Gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

N/A — no files deleted (additive: one column, one candidate-scan filter, one validated field).

---

## Discovery (consult log)

- **Provenance (Jun 04 2026)** — authored in PR `feat/m84-dashboard-runner-enrollment` as a design artifact after Indy's CTO consult on runner state + routing. The GitLab-tags / GitHub-labels model was the reference; usezombie adopts `required_tags ⊆ labels` and explicitly **not** capacity/fairness scheduling. Indy: *"Agree to have a spec written for tag based routing and the spec is pushed as part of this PR itself."*
- **Numbering (Jun 04 2026)** — placement was reserved as `M80_007` in `runner_fleet.md`, but `M80_007` is the shipped runner-observability spec — a real ID collision. This spec is `M85_001`; the placement refs already point to M85_001 in `runner_fleet.md`/`roadmap.md`.
- **Scope reduction (Jun 06 2026, `/plan-ceo-review`)** — the original spec composed four eligibility gates (label ∧ scope ∧ trust ∧ tier) and mandated the match live *inside* the atomic claim. CEO review found: (F1) the in-claim "TOCTOU" invariant guards a race that cannot occur, since tags/labels are immutable for the work's lifetime; (F2) `tier_ok` gated a `required_tier` field the spec never defined; (F3) tenant-scope + trust are an access-control boundary that doesn't exist in `listCandidates` today and should not ride inside a capability patch. Reduced to a **single label gate filtered in `listCandidates`**, claim untouched, Invariant 1 relaxed.
  - **Deferrals (Indy-acked):**
    - Trust/scope/sandbox-tier composition + `required_tier` → own follow-up spec. Ack: Indy (2026-06-06): *"I want 1 gate via tags."*
    - Tag source (Path B, chosen): `required_tags` derives from the SKILL.md `tags:` already in the manifest (`skill_meta.tags`, previously discarded) rather than a new manual API field — cheaper than the manual field and matches "user doesn't separately input the tag." Ack: Indy (2026-06-06): *"Persist skill_meta.tags (Path B)"* (and *"if the user doesnt need to input the tag"*). The literal "bare manual field" (S5-deferred) was dropped because the CLI sends only trigger+source, so it would have shipped inert.
    - L4 write-time `matching_runner_count` echo + L5 `zombies_held_unmatched` metric (the silent-hold / F4 guardrails) → fast-follow; F4 accepted open this slice. Ack: Indy (2026-06-06): *"S1–S4 bare."*

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Coverage vs the label-match matrix (subset, empty-default, sticky, hold, malformed). | Clean; count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial review vs spec, ZIG_RULES, the unchanged-claim invariant, the non-goals fence. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Required-tags model | `make test-integration` | {paste} | |
| Eligibility filter | `make test-integration` | {paste} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste} | |
| Lint | `make lint` | {paste} | |
| Doc sweep | `grep -n "M85_001" docs/architecture/runner_fleet.md` | {paste} | |

---

## Out of Scope

- **Trust / registration-scope / sandbox-tier eligibility (gates 2–4) + `required_tier`** — the composable funnel for these is a separate follow-up spec; they are an access-control boundary that earns its own threat model, and tenant-scope interacts with the trusted-fleet NULL model. Not folded into this capability patch.
- **A separate client-supplied `required_tags` API override** — tags derive from the SKILL.md manifest only; an explicit per-request override field (diverging from the manifest) is out.
- **Silent-hold guardrails (F4)** — write-time `matching_runner_count` echo (with "did you mean") and a `zombies_held_unmatched` metric; deferred fast-follow. A typo'd tag holds silently this slice.
- **Capacity / fairness / bin-packing scheduling** — the match is a boolean label predicate, not a scorer; weighted placement is a later, separate milestone (still fenced by the non-goals).
- **Autoscale / provisioning runners to satisfy demand** — out; the fleet does not create capacity.
- **Runner-side tag re-advertisement per poll** — labels are advertised once at enrollment; not re-sent on `/leases`.
- **Dynamic relabeling of a live runner** — changing a runner's labels after enrollment (and re-evaluating in-flight leases) is future work; the immutability it guarantees is what makes the candidate-set filter sound.
- **`accepts_untagged` runner opt-out** — a specialist runner cannot refuse untagged work in this slice (untagged = any runner); the GitLab "run untagged jobs" toggle is future.
- **Operator plane (cordon/drain/revoke) + event log** — that is M84_002.
