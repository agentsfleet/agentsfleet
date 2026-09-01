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

# M181_006: Staging soak, rollback rehearsal, and the one-move production swap

**Prototype:** v2.0.0
**Milestone:** M181
**Workstream:** 006
**Date:** Sep 01, 2026
**Status:** PENDING
**Priority:** P0 — the family's payoff; every sibling exists so this one is boring
**Categories:** API | INFRA | OBS
**Batch:** B8 — family closer, strictly serial: every dimension needs the merged whole on staging
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M181_002 **merged** (the full route surface); M181_003 **merged** (the coverage gate — the parity roster's contract is generated, not hand-kept); M181_004 **merged** (the export — continuity is unprovable without it); M181_005 **merged** (the collectors — the path continuity is graded through)
**Provenance:** LLM-drafted (Claude Opus 5, Sep 01, 2026) — §3 and §4's swap half of M181_002, split out on Indy's parallelization call; section prose carried over, not re-derived
**Canonical architecture:** `docs/architecture/observability.md` §The three signal paths + `docs/architecture/runner_fleet.md` §Multi-replica + `docs/architecture/scaling.md`

---

## Overview

**Goal (testable):** the three production `agentsfleetd` machines serve the Rust binary after a staging soak in which the black-box parity suite, the runner parity lane, the dry lane, and the latency and memory budgets all pass against the Rust daemon — with a rehearsed one-move rollback to the warm Zig binary and metric families continuous across the boundary.

**Problem:** six milestones of parity evidence are per-surface. Cutover needs whole-system proof — all routes at once, sustained load, memory over hours, dashboards continuous — plus an exit that is boring: same schema, same stores, binary swap back.

**Solution summary:** run the staging soak against the budgets M181_001's lanes already refuse to run without; rehearse the rollback on staging before any production change; execute the production swap from a runbook whose every step carries an executable probe; grade signal continuity through the collectors M181_005 proved under the incumbent binary.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): cutover — soak evidence, rollback rehearsal, and the production swap
- **Intent (one sentence):** production traffic moves to the Rust daemon behind whole-system proof, with rollback reduced to serving a binary that still speaks the same schema.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `playbooks/operations/cutover/001_playbook.md` + `probes.sh` — the runbook and its probe runner; every rubric row here is probe-tagged or manifest-declared.
2. `make/test-parity.mk` + `scripts/parity_lane.sh` — RECORD and COMPARE modes; COMPARE is the state-handoff oracle.
3. `docs/architecture/runner_fleet.md` §Multi-replica — the 3-machine shape, which gauges stay approximate across replicas, and why counters stay exact.
4. The declared-divergence register M181_001 seeded — a parity differ reads a declared difference as declared and an undeclared one as a regression.
5. M181_002's Discovery — the family's decision record; nothing here re-opens it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `deploy/**` | EDIT | drain-swap steps for the 3-machine shape; the serving-binary selection knob |
| `playbooks/operations/cutover/001_playbook.md` | EDIT | swap rows, verification probes, abort criteria, the ONE rollback story, evidence sections |
| `playbooks/operations/cutover/probes.sh` | EDIT | probes for this spec's rubric rows |
| `make/test-parity.mk` | EDIT | the soak's route corpus, now that every route serves and the contract is generated |
| `docs/architecture/runner_fleet.md` | EDIT | production-shape note — serving binary and rollback posture |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — TIM (budgets are named numbers, never widened to pass), ECL (a provider outage mid-soak is not a parity defect), UFS (knob and budget names), ORP (no orphaned runbook rows).
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the runbook and architecture edits are published prose.
- `dispatch/write_shell.md` — probe additions: quoted expansions, no untrusted `eval`.
- `dispatch/verify.md` — every done-claim here is a rubric row; no package-scoped substitutes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| CI/CD edit approval | yes | `deploy/**` and any workflow edits need Indy's explicit approval — sought at PLAN |
| LENGTH / UFS | yes | probes and make edits under caps; budgets as named constants |
| MILESTONE-ID | yes | none in source; runbook is docs (exempt) |
| LOGGING | no | no daemon source changes |
| SCHEMA GUARD | no | no schema change — that is the rollback story |

## Prior-Art / Reference Implementations

- **Reference:** `.github/workflows/deploy-dev*.yml` + `deploy/` — the staged deploy, verify and acceptance shape the swap reuses.
- **Reference:** the M175–M181 rubrics — every per-surface oracle re-runs here as a pre-swap checklist; this spec adds only whole-system proofs.

## Sections (implementation slices)

### §1 — Staging soak with budgets

The whole-system proof on staging: the black-box HTTP parity suite, the runner parity lane and the dry lane against the Rust daemon; sustained mixed load through the benchmark lane; chaos probes for the invariant tables — webhook replay, lease fencing under kill, stream reconnect; and the two budgets M181_001 embedded as constants the lane refuses to run without.

**The Zig integration corpus cannot grade the Rust daemon.** Three independent reasons, each checkable in one command: the lane was deleted with the Zig gating; the tests are in-process, importing Zig modules directly, so there is no HTTP boundary to repoint; and nothing in them names a daemon — the only environment knobs they read are datastore pointers. A green run against a Rust-served environment would report a pass rate for the implementation being retired — worse than no number, because it reads like evidence. M181_001's black-box lane is what replaces it.

**The Zig-side baseline is a manual build.** The Zig binary is still built by the release workflow and by the staging deploy workflow; the frozen revision the comparison needs is pinned deliberately, not taken from whatever last shipped.

- **Dimension 1.1** — the black-box parity suite, the runner parity lane and the dry lane are green against the Rust daemon on staging → Test `test_soak_suites_green`
- **Dimension 1.2** — per-route-class latency is within the budget the lane embeds → Test `test_latency_budget`
- **Dimension 1.3** — resident memory stays within the named ceiling across the soak window under sustained load, with a flat growth trend → Test `test_memory_ceiling_soak`
- **Dimension 1.4** — chaos probes hold mid-soak: replay suppressed, leases fenced, streams reconnect → Test `test_soak_chaos_invariants`
- **Dimension 1.5** — cross-implementation state handoff in both directions: the Rust daemon serves and writes production-shaped state, the Zig daemon then boots on the same stores and resumes serving correctly, and the reverse. Rollback safety is demonstrated, not inferred from "same schema". Graded by the parity lane in COMPARE mode across a swap — `make test-parity BASE_URL=<zig> COMPARE_URL=<rust>` after each direction — rather than by a bespoke lane whose only caller would be one rubric row → Test `state_handoff_is_bidirectional`

### §2 — Rollback rehearsal and the swap

Rollback is rehearsed BEFORE cutover: staging swaps back to the Zig binary using the runbook and verifies clean service. Production cutover is all-at-once across the three machines with load-balancer drain — mixed-fleet operation is structurally tolerated, since every cross-replica invariant is atomic in the datastores, but it doubles the drift surface, so it is the contingency rather than the plan.

**The rollback story is picked here, not inherited.** The parent family carried two incompatible statements: that the Zig binary remains built, shipped and warm as the rollback, and that rollback is a hand-dispatched redeploy of a frozen revision no longer built by CI. The second premise is false in both pipelines — the release workflow and the staging deploy workflow each build the Zig binary today. PLAN picks one story and the runbook states it once, because the two imply different deploy-knob designs.

The runbook carries the declared-divergence register, and the rollback path carries **no migration invocation**: rollback serves an older binary against a ledger it already understands, and a migration there is at best a no-op and at worst the one command that can refuse mid-incident. The probe runner asserts the absence rather than trusting the prose.

- **Dimension 2.1** — rollback rehearsal on staging: swap back, verify, recorded in the runbook's evidence section → Test `test_rollback_rehearsal`
- **Dimension 2.2** — an older binary pointed at a newer ledger refuses rather than reaping, and the rollback path invokes no migration → Test `test_rollback_carries_no_migrate_and_refuses`
- **Dimension 2.3** — every runbook probe is a copy-paste command that passes on staging post-swap, and every rubric row of the merged milestones is probe-tagged or manifest-declared → Test `test_runbook_probes`
- **Dimension 2.4** — metric, span and log families are continuous across the swap: no renamed series, no dropped family, dashboards unbroken → Test `test_signal_continuity`

The production swap itself is operator-executed from the runbook — the agent prepares and rehearses; Indy pulls the trigger, and the swap additionally requires Indy's explicit go recorded in Discovery.

## Interfaces

```
Deploy knob                       serving-binary selection per environment — ONE mechanism, named in the runbook
Runbook                           playbooks/operations/cutover/001_playbook.md — drain order, probes,
                                  abort criteria, one-move rollback, divergence register
make test-parity                  RECORD (BASE_URL) and COMPARE (BASE_URL + COMPARE_URL) modes
make bench-cutover · make dry-app-rustd   the budget and dry lanes M181_001 shipped
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Soak suite red | a parity defect surviving M175–M180 | cutover blocked; the defect routes back to its owning milestone's surface; no partial swap |
| Latency budget miss | a route class over tolerance | cutover blocked; profile, fix, re-soak; the budget is never widened to pass |
| Memory growth in soak | an unbounded buffer or a leaked task | cutover blocked; the trace is attached; fix and re-soak |
| Mid-swap abort | a probe fails on the first machine | abort criteria trigger rollback of the touched machine; a mixed fleet is tolerated structurally while recovering |
| Post-cutover regression | a defect visible only under production traffic | one-move rollback: serve the warm Zig binary; the schema is untouched by design |
| Older binary meets a newer ledger | a rollback or stale image whose migration set predates the database's | the daemon REFUSES, naming the version it does not know, and changes nothing; a rollback that trips this crossed a migration boundary and is not one-move |
| State-handoff regression | the Zig daemon cannot read or resume Rust-written state, or the reverse | the handoff lane goes red and cutover is blocked; serialization fixed before any swap |
| Dashboard discontinuity | a renamed or dropped series | blocked at §2; series names are parity surface, fixed before the swap |

## Invariants

1. Rollback requires no schema or data migration — the family rule that no `schema/` change lands in M175–M181, plus `test_rollback_rehearsal`; the daemon enforces it by refusing a ledger it does not know. The invariant is only cheap while the family rule holds — the first post-cutover migration makes rollback across that boundary a schema decision.
2. The Zig binary is reachable as the rollback by exactly ONE documented mechanism, named in the runbook — `test_rollback_rehearsal`; carrying two descriptions is the failure this invariant prevents.
3. Budgets are named constants compared mechanically, never prose judgments — `test_latency_budget`, `test_memory_ceiling_soak`.
4. Every declared divergence is in the register before cutover, and the parity oracles read it — a declared divergence never surfaces as a regression and an undeclared one always does.
5. Every runbook step carries an executable probe — `test_runbook_probes`.
6. Cutover cannot proceed with any M175–M181 rubric row ungraded or red — the probe runner's row-coverage assert: covered by a tagged probe, or named in the printed exclusion manifest; anything else is a red run.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| existing metric families, continuity asserted across the swap | ops | unchanged | unchanged | unchanged | `test_signal_continuity` |
| span and log families, continuity asserted across the swap | ops | unchanged | unchanged | no payload bytes, no credentials | `test_signal_continuity` |
| `deploy.serving_binary` (one label on existing deploy telemetry) | ops | deploy or swap | binary name, environment | none needed | `test_rollback_rehearsal` |

No product-analytics changes.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | e2e (staging) | `test_soak_suites_green` | parity suite + runner lane + dry lane green against the Rust daemon |
| 1.2 | e2e (staging) | `test_latency_budget` | per-route-class p95 within the lane's embedded constants |
| 1.3 | e2e (staging) | `test_memory_ceiling_soak` | RSS under the named ceiling, flat growth trend over the window |
| 1.4 | e2e (staging) | `test_soak_chaos_invariants` | replay suppressed, leases fenced, streams reconnect, mid-soak |
| 1.5 | e2e (staging) | `state_handoff_is_bidirectional` | COMPARE mode green after a swap in each direction over shared stores |
| 2.1 | e2e (staging) | `test_rollback_rehearsal` | swap back, clean service, evidence recorded |
| 2.2 | integration | `test_rollback_carries_no_migrate_and_refuses` | old binary refuses a newer ledger; rollback path contains no migrate step |
| 2.3 | e2e (staging) | `test_runbook_probes` | every probe passes post-swap; row coverage complete |
| 2.4 | e2e (staging + production) | `test_signal_continuity` | no renamed series, no dropped family, across the boundary |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Whole-system soak green (§1) | `make test-parity BASE_URL=<rust-staging>` + `make dry-app-rustd` | exit 0 each | P0 | |
| R2 | Budgets hold (§1) | `make bench-cutover` | exit 0 | P0 | |
| R3 | Handoff bidirectional (§1) | `make test-parity BASE_URL=<zig> COMPARE_URL=<rust>` after each swap direction | exit 0 both runs | P0 | |
| R4 | Rollback rehearsed and probes green (§2) | `bash playbooks/operations/cutover/probes.sh` on staging, post-swap and post-rollback | exit 0 both runs | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration lane green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Lint green | `make lint-all` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** S1–S5 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.unit`, `verify.integration`, `verify.lint`, `verify.version`); S6 is the template's hygiene gate; R-rows name the lanes M181_001 shipped and this spec drives.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE. The production swap additionally requires Indy's explicit go in Discovery.

## Dead Code Sweep

N/A — no files deleted. The Zig daemon's retirement is a separate post-cutover milestone; its binary remains the rollback for the whole of this one.

## Out of Scope

- Zig retirement, behaviour changes, new dashboards, canary infrastructure beyond the selection knob.
- Everything the four sibling specs own: the route surface (002), the coverage gate (003), the export (004), the collectors (005).

## Product Clarity (authoring record)

1. **Successful user moment** — production runs the Rust daemon for a full day: fleets wake, leases complete, dashboards continuous, and nobody outside the team can tell anything changed. The rollback runbook stays unused.
2. **Preserved user behaviour** — everything; that is the entire spec.
3. **Optimal-way check** — an all-at-once swap with a rehearsed rollback beats a rolling mixed fleet: the invariants tolerate mixing, but a single boundary keeps triage unambiguous, and the canary path is named as the contingency in the runbook.
4. **Rebuild-vs-iterate** — N/A: ships proof and process, not new architecture.
5. **What we build** — the soak evidence, the rehearsal, the runbook's swap rows and probes, the swap itself.
6. **What we do NOT build** — anything a sibling owns; anything the runbook cannot probe.
7. **Fit with existing features** — rides the existing deploy and verify workflow shape; must not destabilize the Zig release path, which remains the rollback.
8. **Surface order** — N/A — operational; no new user surface.
9. **Dashboard restraint** — nothing new to show; continuity is the deliverable, and a new panel at cutover would be indistinguishable from a regression.
10. **Confused-user next step** — an operator mid-incident opens the runbook; every step has a probe and an abort criterion, and the divergence register tells them what genuinely differs between the binaries.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** two slices — prove, then swap behind a rehearsal — the irreversible-adjacent step preceded by the thing that would have caught it.
- **Alternatives considered:** keeping soak and swap inside M181_002 (rejected on Indy's parallelization call: every dimension here is serial behind ALL siblings, so holding the route-surface PR hostage to it parallelizes nothing); a rolling per-machine cutover as the plan (rejected: doubles the drift surface for little gain; kept as the contingency).
- **Patch-vs-refactor verdict:** this is a **patch** to the operational layer — pipelines, lanes, runbook; the refactor was M176–M180 and the proof machinery is M181_001's.

## Discovery (consult log)

> Indy (2026-09-01): "i wanna see what can be batched parallelized and break to smaller PRs?" … "Yes, 5 specs as drawn" — context: §3 and §4's swap half of M181_002, split into the family closer; prose carried over; the collector-first step went to M181_005.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
