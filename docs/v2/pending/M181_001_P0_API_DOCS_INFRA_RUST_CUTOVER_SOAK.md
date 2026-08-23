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

# M181_001: Cutover — soak, swap, and one-move rollback to the Rust daemon

**Prototype:** v2.0.0
**Milestone:** M181
**Workstream:** 001
**Date:** Aug 23, 2026
**Status:** PENDING
**Priority:** P0 — the family's payoff; everything before it is preparation
**Categories:** API | DOCS | INFRA
**Batch:** B6 — family closer, serial after M178 + M179 + M180
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M178_001, M179_001, M180_001 (full route surface); M177_001 (runner plane); M176_001 (substrate)
**Provenance:** LLM-drafted (Claude Fable 5, Aug 23, 2026)
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Multi-replica + `docs/architecture/scaling.md`

---

## Overview

**Goal (testable):** the three production `agentsfleetd` machines serve the Rust binary after a staging soak in which the full-route OpenAPI coverage gate, the complete Zig-side integration suite, the runner parity lane, and the latency/memory budget checks all pass against `agentsfleetd-rs` — with a rehearsed one-move rollback to the warm Zig binary.
**Problem:** six milestones of parity evidence are per-surface; cutover needs whole-system proof (all routes at once, sustained load, memory over hours, dashboards continuous) plus an exit that is boring — same schema, same stores, binary swap back.
**Solution summary:** wire the full-route parity gate (a Rust route dump feeding `scripts/check_openapi_route_coverage.py`), extend the release pipeline to build and ship the Rust binary alongside the Zig one, run the staging soak with explicit budgets, execute an all-at-once production swap with load-balancer drain, rehearse rollback in staging first, and record the runbook + decision log in the repository.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): cutover — parity gate, soak lanes, deploy + runbook
- **Intent (one sentence):** production traffic moves to `agentsfleetd-rs` behind proof, with rollback reduced to swapping back a binary that still speaks the same schema.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `scripts/check_openapi_route_coverage.py` — the served-vs-documented parity gate; this milestone gives it a Rust route source.
2. `docs/architecture/runner_fleet.md` §Multi-replica — the 3-machine production shape, which gauges stay approximate across replicas, and why counters stay exact via sum-by.
3. `.github/workflows/release.yml` + `deploy/` + `Dockerfile` — the build/ship path the Rust binary joins (CI/CD edits — explicit user approval per repo rule; this spec is the record, and REVIEW re-confirms before merge).
4. `make/bench.mk` + `bench/` — the existing benchmark harness the latency-budget rows reuse.
5. `docs/architecture/data_flow.md` — the invariant tables the soak's chaos probes exercise (replay, fencing, idempotency).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/src/agentsfleetd/**` | EDIT | `routes --json` dump subcommand + `doctor`/`backfill` subcommand parity |
| `scripts/check_openapi_route_coverage.py` | EDIT | accepts the Rust route dump as a served-route source |
| `make/dry.mk` | EDIT | dry lane variant booting the Rust daemon |
| `make/bench.mk` | EDIT | adds `bench-cutover`: Zig-baseline-vs-Rust comparison on the same harness, tolerances as named constants (distinct caller: the cutover checklist) |
| `make/test-integration.mk` | EDIT | adds `test-handoff`: bidirectional cross-implementation state-handoff lane (distinct caller: the cutover checklist) |
| `Dockerfile` | EDIT | builds and ships the Rust binary alongside the Zig one |
| `.github/workflows/release.yml` | EDIT | Rust binary in the release artifact set |
| `.github/workflows/deploy-dev.yml` | EDIT | staging deploy can select the serving binary |
| `deploy/**` | EDIT | binary-selection knob + drain-swap steps for the 3-machine shape |
| `playbooks/cutover/rust_daemon.md` | CREATE | the cutover + rollback runbook (drain order, verification probes, abort criteria) |
| `playbooks/cutover/probes.sh` | CREATE | executable probe runner the runbook's per-step verification and the rehearsal rubric row both call |
| `docs/architecture/runner_fleet.md` | EDIT | production-shape note: serving binary + rollback posture |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — TIM (latency/memory budgets are explicit named numbers, not vibes), ECL, NDC, UFS, TST-NAM, MSID, FLL.
- `dispatch/write_rust.md` — REVIEW cites Microsoft guideline mnemonics for the route-dump + subcommand code.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the runbook and architecture-doc edits are published prose.
- `dispatch/verify.md` — done-claims here are exactly the rubric rows; no package-scoped substitutes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes | route dump is a thin serializer over the Route enum |
| LOGGING | yes | swap/drain steps log scoped events; no new secret surfaces |
| MILESTONE-ID | yes | none in source; runbook is docs (exempt) |
| UFS | yes | budget numbers, drain timeouts as named constants |
| SPEC TEMPLATE GATE | yes — this file | required sections filled |
| SCHEMA GUARD | no | no schema change — that is the rollback story |

## Prior-Art / Reference Implementations

- **Reference:** `.github/workflows/deploy-dev*.yml` + `deploy/` — the existing staged deploy/verify/acceptance shape; the cutover reuses its verification pattern rather than inventing one.
- **Reference:** `docs/architecture/runner_fleet.md` §Multi-replica — the release workflow already sets and verifies the 3-machine shape; the swap rides that mechanism.
- **Reference:** the M175–M180 rubrics — every per-surface oracle re-runs here as a pre-swap checklist; this spec adds only whole-system proofs.

## Sections (implementation slices)

### §1 — Full-route parity gate

`agentsfleetd-rs routes --json` dumps the served route × method set from the Route enum. The coverage script today hard-codes the Zig route table as its served source and compares paths only — this milestone extends it with a `--served <file>` argument (the locked interface R1 invokes) and route × method comparison, then grades the Rust daemon with the same script that gates the Zig one. `doctor` and `backfill` subcommands reach behaviour parity so operations tooling does not fork.

- **Dimension 1.1** — route dump equals the Zig daemon's served set exactly (diff empty both directions) → Test `test_route_dump_matches_zig_set`
- **Dimension 1.2** — coverage script passes with the Rust source; a seeded missing route fails it → Test `test_coverage_gate_rust_source`
- **Dimension 1.3** — `doctor` and `backfill` produce parity outcomes on seeded states → Test `test_ops_subcommand_parity`

### §2 — Build and ship

The release pipeline builds the Rust binary (matching the Zig binary's target matrix), ships both in the artifact set, and the deploy layer selects the serving binary per environment via one knob. The Zig binary remains built, shipped, and warm — it IS the rollback.

- **Dimension 2.1** — release artifacts contain both binaries; versions agree with `VERSION` → Test `test_release_artifact_set`
- **Dimension 2.2** — deploy binary-selection knob flips the serving binary on a staging machine with a clean drain → Test `test_deploy_binary_selection`

### §3 — Staging soak with budgets

The whole-system proof on staging: full Zig integration suite + runner parity lane + dry lane against the Rust daemon, sustained mixed load via the new `make bench-cutover` lane (Zig baseline vs Rust, same harness and hardware — the existing bench harness has no comparison mode today, this milestone adds it), chaos probes for the invariant tables (webhook replay, lease fencing under kill, SSE reconnect), and two explicit budgets embedded in `bench-cutover` as named constants — p95 latency per route class within tolerance of the baseline, and a flat RSS ceiling over the soak window (the Rust memory-safety story replaces `make memleak`, which stays Zig-only — recorded decision). **Implementation default:** p95 tolerance = baseline + 10% per route class; RSS ceiling = the Zig daemon's soak peak + 20% — Indy may override either at PLAN; whichever constants Discovery records are the ones the lane embeds, and the lane refuses to run with unset constants, so the P0 gate is a real command with real numbers, never a vibe.

- **Dimension 3.1** — full integration + runner + dry lanes green against the Rust daemon on staging → Test `test_soak_suites_green`
- **Dimension 3.2** — p95 per route class within the budget vs the same-harness Zig baseline → Test `test_latency_budget`
- **Dimension 3.3** — RSS flat within the named ceiling across the soak window under sustained load → Test `test_memory_ceiling_soak`
- **Dimension 3.4** — chaos probes: replay/fencing/reconnect invariants hold mid-soak → Test `test_soak_chaos_invariants`
- **Dimension 3.5** — cross-implementation state handoff, both directions: the Rust daemon serves and writes production-shaped state (in-flight leases, stream entries, billing rows, migration ledger); the Zig daemon then boots on the same stores and resumes serving correctly — and the reverse. Rollback safety is demonstrated, not inferred from "same schema" → Test `test_state_handoff_bidirectional` (the `make test-handoff` lane this milestone adds)

### §4 — Swap, rollback rehearsal, runbook

Rollback is rehearsed BEFORE cutover: staging swaps Rust → Zig using the runbook and verifies clean service. Production cutover is all-at-once across the three machines with load-balancer drain (mixed-fleet operation is structurally tolerated — every cross-replica invariant is atomic in Postgres/Redis — but doubles the drift surface, so it is the contingency, not the plan; recorded decision). The runbook lands in `playbooks/cutover/rust_daemon.md` with drain order, per-step verification probes, abort criteria, and the one-move rollback. Post-swap, the OTLP dashboards must show continuous metric families across the boundary.

- **Dimension 4.1** — rollback rehearsal on staging: swap back, verify, documented in the runbook's own evidence section → Test `test_rollback_rehearsal`
- **Dimension 4.2** — runbook probes are copy-paste commands that pass on staging post-swap → Test `test_runbook_probes`
- **Dimension 4.3** — metric-family continuity across the swap (no renamed series, dashboards unbroken) → Test `test_metric_continuity`

## Parallelization & execution map

(Internal batch labels here sequence THIS milestone's work only; the frontmatter **Batch:** line is the family-level ordering — two different axes, deliberately.)

| Batch | Scope | Runtime · model · reasoning tier | Why |
|---|---|---|---|
| B1 | §1 parity gate | Claude Code · Opus 5 · high | mechanical serializer + script extension with exact oracle |
| B1 | §2 build/ship | Claude Code · Opus 5 · high | pipeline work inside existing workflow shapes |
| B2 (serial) | §3 soak | Claude Code · Opus 5 · xhigh | budget setting + failure triage across the whole system |
| B3 (serial) | §4 swap/runbook | Claude Code · Opus 5 · max | the irreversible-adjacent step; strongest tier, human (Indy) executes the production swap |

The production swap itself is operator-executed from the runbook — the agent prepares and rehearses; Indy pulls the trigger.

## Interfaces

```
agentsfleetd-rs routes --json     route × method dump (feeds coverage script)
agentsfleetd-rs doctor|backfill   behaviour parity with the Zig subcommands
Deploy knob                       serving-binary selection per environment
Runbook                           playbooks/cutover/rust_daemon.md — drain order,
                                  probes, abort criteria, one-move rollback
Budgets (named constants in the bench lane): p95-per-route-class tolerance vs
Zig baseline; RSS ceiling over the soak window. Numbers are set at PLAN from
the first baseline run and recorded in the spec's Discovery, not invented here.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Soak suite red | any parity defect surviving M175–M180 | cutover blocked; defect routed back to its owning milestone's surface; no partial swap |
| Latency budget miss | route class over tolerance | cutover blocked; profile → fix → re-soak; budget never widened to pass |
| Memory growth in soak | leak-equivalent (unbounded buffer, task leak) | cutover blocked; RSS trace attached; fix and re-soak |
| Mid-swap abort | probe failure on machine 1 of 3 | abort criteria trigger rollback of the touched machine; mixed fleet tolerated structurally while recovering |
| Post-cutover regression | defect only visible under production traffic | one-move rollback: redeploy the warm Zig binary; schema untouched by design |
| State-handoff regression | Zig cannot read or resume Rust-written state (or reverse) | `make test-handoff` red → cutover blocked; serialization fixed before any swap |
| Dashboard discontinuity | renamed/dropped metric series | blocked at §4; series names are parity surface, fixed before swap |
| Release artifact drift | Rust binary missing from a release | release lane asserts both binaries present; red release, no deploy |

## Invariants

1. Rollback requires no schema or data migration — enforced by the family rule (no `schema/` changes in M175–M181) + `test_rollback_rehearsal`.
2. The Zig binary stays built, shipped, and deployable until a post-cutover retirement milestone Indy opens — enforced by the release lane asserting both artifacts.
3. Budgets are named constants compared mechanically — never prose judgments — `test_latency_budget`, `test_memory_ceiling_soak`.
4. Every runbook step carries an executable probe in `playbooks/cutover/probes.sh`; a deviation surfaces as a failed probe run, not a judgment call — `test_runbook_probes` proves the script executable end-to-end.
5. Cutover cannot proceed with any M175–M180 rubric row ungraded or red — enforced mechanically: `probes.sh`'s pre-swap section is **derived from the Acceptance Rubric tables of the five merged specs**, and every row lands in exactly one of two buckets: (a) **covered** — one or more executable probes, each tagged with its source row id, normalized into runnable shell (a multi-command cell expands to several probes under the same row tag, stated prerequisites become setup steps), or (b) a **declared exclusion** — a row whose evidence is historical and not re-runnable (e.g. a one-time seeded-defect record), listed in an exclusion manifest the script prints on every run and Indy signs off in Discovery at PLAN. The completeness assert is over **rows, not probes**: every R+S row id in those rubrics is either tagged by ≥1 probe or named in the manifest, and every probe carries a row tag. An uncovered row, an untagged probe, or an undeclared skip is a red run, not a silent gap.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| existing OTLP families (continuity asserted) | ops | unchanged | unchanged | unchanged | `test_metric_continuity` |
| `deploy.serving_binary` (one gauge/label on existing deploy telemetry) | ops | deploy/swap | binary name, environment | none needed | `test_deploy_binary_selection` |

No product-analytics changes.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_route_dump_matches_zig_set` | set difference in both directions is empty |
| 1.2 | integration (negative) | `test_coverage_gate_rust_source` | full set passes; seeded removal fails naming the route |
| 1.3 | integration | `test_ops_subcommand_parity` | doctor/backfill outcomes equal Zig on seeded states |
| 2.1 | integration | `test_release_artifact_set` | release contains both binaries at `VERSION` |
| 2.2 | e2e | `test_deploy_binary_selection` | staging flip serves Rust; probes green; flip back serves Zig |
| 3.1 | e2e | `test_soak_suites_green` | integration + runner + dry lanes exit 0 vs the Rust daemon |
| 3.2 | e2e | `test_latency_budget` | per-class p95 within the named tolerance of the same-harness Zig baseline |
| 3.3 | e2e (negative-sensitive) | `test_memory_ceiling_soak` | RSS within ceiling over the window; growth trend flat |
| 3.4 | e2e (chaos) | `test_soak_chaos_invariants` | replay/fencing/reconnect probes hold mid-load |
| 3.5 | e2e (negative-sensitive) | `test_state_handoff_bidirectional` | Rust-written live state served correctly by Zig after swap, and reverse (`make test-handoff`) |
| 4.1 | e2e | `test_rollback_rehearsal` | staged Rust→Zig swap verified by `probes.sh` exit 0 |
| 4.2 | e2e | `test_runbook_probes` | `bash playbooks/cutover/probes.sh` passes post-swap on staging; every M175–M180 R+S row id is probe-tagged or manifest-declared, and every probe carries a row tag |
| 4.3 | integration | `test_metric_continuity` | series names/labels identical across the swap boundary |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Full-route parity (§1) | `agentsfleetd-rs routes --json > /tmp/served-routes.json && python3 scripts/check_openapi_route_coverage.py --served /tmp/served-routes.json` | exit 0 | P0 | |
| R2 | Whole-system soak green (§3) | `make test-integration DAEMON=rust` + `make dry-app` (Rust variant) + `make test-handoff` | exit 0 each | P0 | |
| R3 | Budgets hold (§3) | `make bench-cutover` | exit 0 (tolerance constants embedded in the lane) | P0 | |
| R4 | Rollback rehearsed (§4) | `bash playbooks/cutover/probes.sh` on staging, post-swap and post-rollback | exit 0 both runs | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.lint`, `verify.unit`, `verify.version`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates (secret scan, oversize sweep), deliberately outside the declared set; R-rows name oracles this spec's own Files Changed create, so every command is copy-paste by merge time.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE. The production swap additionally requires Indy's explicit go in Discovery.

## Dead Code Sweep

N/A — no files deleted (Zig daemon retirement is a separate post-cutover milestone).

## Out of Scope

- Deleting or de-listing the Zig daemon, its lanes, or `make memleak` — retirement is its own milestone after a stable production window Indy defines.
- Any behaviour improvement unlocked by Rust — the parity-first family rule holds through cutover.
- M136 activation: per Indy (Aug 23, 2026), M136 testing begins once `agentsfleetd-rs` is ready to replace the Zig daemon — that readiness is this milestone's R-rows going green; M173/M174 are revisited at the same point.
- Public docs (`~/Projects/docs`): no endpoint/command/flag/behaviour changes ship, so no docs-repo branch — recorded here as the why-not.

---

## Product Clarity (authoring record)

1. **Successful user moment** — production runs Rust for a full day: fleets wake, leases complete, dashboards continuous — and nobody outside the team can tell anything changed. The rollback runbook stays unused.
2. **Preserved user behaviour** — everything; that is the entire milestone.
3. **Optimal-way check** — all-at-once swap with rehearsed rollback beats a rolling mixed fleet: the invariants tolerate mixing, but a single boundary keeps triage unambiguous; the gap to "canary one machine first" is named as the contingency path in the runbook.
4. **Rebuild-vs-iterate** — N/A at this milestone; it ships proof and process, not new architecture.
5. **What we build** — parity gate plumbing, dual-binary release, soak lanes with budgets, the runbook.
6. **What we do NOT build** — Zig retirement, behaviour changes, new dashboards, canary infrastructure beyond the binary-selection knob.
7. **Fit with existing features** — rides the existing deploy/verify workflow shape; must not destabilize the release path for the Zig binary (which remains the rollback).
8. **Surface order** — N/A — operational milestone; no new user surface.
9. **Dashboard restraint** — nothing new to show; continuity is the deliverable.
10. **Confused-user next step** — an operator mid-incident opens `playbooks/cutover/rust_daemon.md`; every step has a probe and an abort criterion.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four slices — prove (gate), ship (artifacts), soak (budgets), swap (runbook) — ordered so every irreversible-adjacent step is preceded by its rehearsal.
- **Alternatives considered:** rolling per-machine cutover as the plan (rejected: mixed fleet doubles the drift surface for little gain; kept as contingency); making the codecov `rust_afd` flag and Rust CI contexts required at this milestone (accepted as a §2 deploy-gate decision recorded at PLAN — the flip happens here, not in M175, once the lanes have history).
- **Patch-vs-refactor verdict:** this is a **patch** to the operational layer (pipelines, lanes, runbook); the refactor was M176–M180 and this milestone only proves it.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
