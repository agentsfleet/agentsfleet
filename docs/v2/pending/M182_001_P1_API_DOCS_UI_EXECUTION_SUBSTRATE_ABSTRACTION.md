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

# M182_001: Execution substrate abstraction — the wire speaks isolation classes and guarantees

**Prototype:** v2.0.0
**Milestone:** M182
**Workstream:** 001
**Date:** Aug 25, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing platform capability; nothing blocks on it, but every non-bubblewrap substrate blocks on it
**Categories:** API | DOCS | UI
**Batch:** B7 — serial after M181; the first wire change taken with one daemon left
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M181_001 (cutover retires the Zig daemon — a wire change before it costs two daemons plus the runner), M177_001 (the `Guarantee` seam in `reconcile.rs` this spec widens onto the wire)
**Provenance:** LLM-drafted (Claude Fable 5, Aug 25, 2026)
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Assigned policy and reconciliation

---

## Overview

**Goal (testable):** a runner assigned isolation class `kernel` and reporting `{substrate, isolation_class, guarantees, mechanisms}` reconciles to the same verdicts, in the same refusal order, as today's Linux-mechanism wire — while a mechanism-vocabulary grep over `rustd/crates/afd_runner/src`, `rustd/crates/afd_fleet/src`, and `rustd/crates/afd_wire/src` returns zero lines.
**Problem:** only a bubblewrap-shaped Linux host can join the fleet. The control plane interrogates every runner about Landlock, seccomp, cgroup controllers, and a bubblewrap binary by name, so a Firecracker microVM host or a whole-Virtual-Machine (VM) host delivering identical isolation reconciles as permanently degraded and is issued no work.
**Solution summary:** the assignment vocabulary becomes an **isolation class** (`none | kernel | machine` — what the tenant's lease is promised, never how a host builds it), and the capability report becomes `{substrate, isolation_class, guarantees, mechanisms}` — the runner maps its own mechanisms onto the five `Guarantee` outcomes at the probe, so `Guarantee::proven_by` (the one substrate-aware function left in `afd_fleet`) is deleted and the verdict is `required_guarantees ⊆ reported_guarantees` plus a class floor. One rename rides the whole surface (`sandbox_tier` → `isolation_class`: wire, columns, API fields, dashboard) with a value migration and no compatibility aliases. The lease model — fencing, lease expiry, debits, the twelve EXECUTE hot-path writes — is untouched; serverless substrates are parked by owner directive (Discovery).

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(wire): isolation classes and guarantees replace the Linux tiers
- **Intent (one sentence):** an operator describes any execution substrate to the control plane by the isolation it delivers, so a Firecracker or VM runner can join the fleet without the daemon learning one Linux fact.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_runner/src/reconcile.rs` — the module documentation is the design brief; its evidence table and `proven_by` are exactly what this milestone deletes. **Path note:** this spec was drafted against `afd_fleet::runner`; `cf3f75199` ("the event log, the money and the host each get the crate they always were") moved the whole runner surface — `reconcile.rs`, `policy.rs`, `bounds.rs`, `spelling.rs`, `heartbeat.rs`, `record.rs`, `store.rs`, `sql/` — into its own `afd_runner` crate, while the runner INTEGRATION tests stayed in `afd_fleet/tests/`. Both crates are in the blast radius; every path and grep below is corrected to the post-split tree.
2. `docs/architecture/runner_fleet.md` §Assigned policy and reconciliation + §Sandbox tiers — the assign-down / report-up model this spec preserves, and the tier table it rewrites.
3. `rustd/crates/afd_wire/src/runner.rs` — the wire being changed; every struct doc carries the reason for its shape.
4. `src/lib/contract/protocol_policy.zig` + `src/lib/contract/fixture_export.zig` — Zig is the wire's source of truth; `make wire-fixtures` regenerates `samples/fixtures/wire-v2/`, which `afd_wire`'s roundtrip/strictness tests consume.
5. `docs/architecture/data_flow.md` §C. EXECUTE — the twelve hot-path writes, read to verify this diff touches none of them.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/lib/contract/**` | EDIT | `IsolationClass` + `Guarantee` vocabulary, capability-report reshape, selftest echo rename, fixture export |
| `samples/fixtures/wire-v2/**` | EDIT | regenerated via `make wire-fixtures` (Zig is the source of truth) |
| `rustd/crates/afd_wire/**` | EDIT | Rust mirror of the wire; roundtrip / strictness / redaction tests re-pinned |
| `rustd/crates/afd_runner/src/**` | EDIT | verdict on guarantees + class floor (`reconcile.rs`); `policy.rs`, `bounds.rs`, `spelling.rs`, `heartbeat.rs`, `record.rs`, `store.rs`, `view/decode.rs` follow the vocabulary |
| `rustd/crates/afd_runner/src/sql/**` | EDIT | statements name the renamed columns (`runner.rs`, `runner_view.rs`) |
| `rustd/crates/afd_fleet/tests/**` | EDIT | `verdict_matrix.rs` and the runner-row / beat / admin integration lanes re-pinned in class vocabulary |
| `rustd/crates/afd_runner/tests/**` | EDIT | `runner_suite.rs` and the sweep lanes re-pinned in class vocabulary |
| `rustd/crates/afd_api/src/handler/runner/**` | EDIT | enrolment / self / heartbeat surfaces carry `isolation_class` |
| `rustd/crates/afd_state/tests/support/**` | EDIT | fixtures speak class vocabulary |
| `src/runner/**` | EDIT | probe asserts substrate + guarantees (`engine/capability_probe.zig`); `AppliedPolicy` decode, release-build refusal of `none`, sandbox gate reads (`child_supervisor.zig`, `sandbox_args.zig`), selftest echo, `cmd/status.zig` render |
| `schema/NNN_isolation_class.sql` (next free slot) | CREATE | column rename + tier→class value migration + stored-verdict reset; registered per the migration model in force post-M181 |
| `public/openapi/components/schemas.yaml` | EDIT | renamed fields and the new capability-report shape |
| `ui/packages/app/app/(dashboard)/admin/runners/**` | EDIT | Add-Runner / Edit-Policy pickers and read panels speak class, substrate, guarantees |
| `docs/architecture/runner_fleet.md` | EDIT | §Sandbox tiers becomes isolation classes; reconciliation prose drops mechanism vocabulary |
| `~/Projects/docs` (own branch, never through this worktree) | EDIT | public field rename + operator runbook maps guarantee reasons to repair steps |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NLR (the tier vocabulary retires outright, touch-it-fix-it), NLG (no compatibility aliases: old spellings are refused, never mapped), NDC (no dead vocabulary — classes with no assignable substrate are excluded), ORP (cross-layer orphan sweep is this spec's Dead Code Sweep), UFS (class/guarantee spellings single-sourced beside their enums), TGU (verdict inputs vs display diagnostics are distinct types, not optional-field soup), STS (vocabulary never becomes a schema CHECK — plain TEXT, app constants), TST-NAM, MSID, ERR (existing codes referenced; none declared), GRP.
- `dispatch/write_rust.md` §Functional design (illegal states unrepresentable — class and verdict stay enums), §Constant discipline, §Evolution — mandatory before any `rustd/` edit.
- `dispatch/write_zig.md` — the probe, applied-policy, and protocol edits; PUB shape verdicts on changed structs.
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` §Migration Model — new numbered slot; shipped slots stay frozen; the rename sits outside the additive default an agent may author alone (owner decision recorded in Discovery).
- `docs/REST_API_DESIGN_GUIDELINES.md` — renamed public fields on the runner and fleet-runner routes.
- `docs/DOCUMENTATION_RULES.md` via `write_documentation` — the architecture rewrite and the docs-repo branch.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — `fleet.runners` rename + value migration | one new numbered slot per §Migration Model; idempotent guards; no vocabulary inside schema statements (RULE STS) |
| ZIG GATE / PUB GATE | yes — protocol + runner edits | read `dispatch/write_zig.md`; pub-shape verdict per changed struct; both linux targets cross-compile |
| UI GATE / DESIGN TOKEN GATE | yes — admin/runners components | design-system primitives and token utilities only |
| File & Function Length (≤350/≤50/≤70) | yes | reconcile stays one module; probe mapping is one function beside the probe; UI panels stay per-component |
| UFS | yes | wire spellings live beside their enums, single-sourced; no scattered literals |
| LOGGING | yes — refusal + degraded logs re-vocabulary | scoped events keep their existing `error_code`s; reasons log the guarantee, never a secret |
| MILESTONE-ID | yes | no `M182` in source or tests |
| ERROR REGISTRY | no | no new `UZ-*` code: degraded verdicts are row state, not error codes; refusals reuse existing validation codes and `ERR_EXEC_RUNNER_INVALID_CONFIG` |
| SPEC TEMPLATE GATE | yes — this file | required sections filled |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_runner/src/reconcile.rs` — the `Guarantee` seam M177 landed at zero wire cost; this spec is its second half, and the module doc names the deletion contract for `proven_by`.
- **Reference:** `docs/architecture/runner_fleet.md` §Assigned policy and reconciliation — assigned and achievable stay separate columns; policy flows down, capability flows up; the report stays unauthenticated self-assertion.
- **Reference:** `src/lib/contract/fixture_export.zig` + `make wire-fixtures` — the established wire-evolution mechanism: edit the Zig vocabulary, regenerate `samples/fixtures/wire-v2/`, mirror in `afd_wire` until roundtrip and strictness are green.

## Sections (implementation slices)

### §1 — The wire speaks classes and guarantees

The vocabulary lands at the wire, Zig first. `IsolationClass` (`none | kernel | machine`, ordered) replaces `SandboxTier`; `Guarantee` moves into the wire with the five spellings `reconcile.rs` already names; `CapabilityReport` becomes `{substrate, isolation_class, guarantees, mechanisms}` where `substrate` is a bounded free string (`bubblewrap` today) and `mechanisms` is bounded prose an operator reads and nothing branches on. `SelftestReport` and `SelfResponse` rename their tier echoes. Fixtures regenerate; `afd_wire` mirrors until roundtrip, strictness, and redaction are green. **Implementation default:** vocabulary extension is a lockstep change by design — unknown spellings fail decode closed on both sides, so a version skew reads degraded rather than mis-caged, and `managed` joins the enum only with the serverless milestone.

- **Dimension 1.1** — every class and guarantee spelling round-trips through the regenerated fixtures in both languages → Test `test_wire_class_guarantee_round_trip`
- **Dimension 1.2** — an unknown class, an unknown guarantee, and every retired tier spelling fail decode closed → Test `test_wire_unknown_vocabulary_fails_closed`
- **Dimension 1.3** — report bounds reshape (substrate and mechanisms length caps, guarantee-list cap); an over-bounds report is refused while the heartbeat still lands liveness → Test `test_capability_bounds_reshaped`

### §2 — The verdict is substrate-blind

`reconcile` becomes: no assigned policy → degraded; an isolating network posture on class `none` → structurally undeliverable (today's cage rule, re-worded); then each demanded guarantee — the four cage guarantees for `kernel` and `machine`, plus `egress_control` when the posture demands it — checked in the fixed refusal order, first unmet names the reason in guarantee vocabulary; then the class floor: reported class below the assigned class degrades with its own reason (a bubblewrap host assigned `machine` reads degraded even with every guarantee proven). `proven_by`, the controller table, and every mechanism-named reason constant are deleted; the operator sentences become guarantee sentences, with the report's `mechanisms` prose beside them on the row carrying the host-specific hint. `StoredVerdict` movement detection, the lenient bounds refusals, and the write-only-on-change heartbeat behaviour are preserved.

- **Dimension 2.1** — the full (class × report) verdict matrix holds, refusal order pinned, guarantee-vocabulary reasons → Test `test_verdict_matrix_guarantee_order`
- **Dimension 2.2** — full guarantees but reported class below assignment → degraded with the class-floor reason, checked after guarantee gaps → Test `test_verdict_class_floor`
- **Dimension 2.3** — missing policy and missing report degrade exactly as today; a demand-free `none` assignment is healthy with no report → Test `test_verdict_missing_inputs_degrade`

### §3 — Assignment surfaces and the row

The rename rides every surface at once, no aliases: `POST /v1/runners`, `PATCH /v1/fleets/runners/{id}`, `GET /v1/runners/me`, and the fleet-runner reads carry `assigned_policy.isolation_class`; retired tier spellings and unknown classes are refused by the existing validation path. The migration renames `fleet.runners.sandbox_tier` → `isolation_class`, maps values (`landlock_full` → `kernel`, `container_nested` → `kernel` — the runner branches only on `!= dev_none`, so the collapse is behaviour-free; `dev_none` → `none`), renames the selftest tier echo column, and resets stored capability reports and selftest verdicts to not-reported — the next beat repopulates them, and a demanding assignment reads degraded for at most one beat after deploy (named as the rollout window, not hidden). An unexpected stored value aborts the migration rather than guessing.

- **Dimension 3.1** — register and PATCH accept the class vocabulary and refuse retired tier spellings and unknown classes with the existing validation refusal → Test `test_assignment_accepts_class_refuses_tier`
- **Dimension 3.2** — the migration maps every seeded tier row, resets stored verdicts, and aborts loudly on an unexpected value → Test `test_migration_maps_every_tier`
- **Dimension 3.3** — self, list, and detail reads return `isolation_class`, `substrate`, `guarantees`, `mechanisms`; `token_hash` stays structurally absent → Test `test_operator_read_exposes_substrate`

### §4 — The runner asserts its substrate

The mechanism→guarantee mapping moves to the party that knows the substrate: `engine/capability_probe.zig` keeps probing Landlock ABI, seccomp, delegated controllers, bubblewrap, and egress enforcement, and now emits `substrate: "bubblewrap"`, the class the driver builds (`kernel`; `none` on the no-sandbox posture), the guarantee set those probes prove (`egress_control` stays absent while enforcement is unwired — today's pinned-false behaviour, preserved), and one `mechanisms` prose line naming what an operator would repair. `AppliedPolicy` decodes the renamed field fail-closed; the release-build refusal of `dev_none` becomes the refusal of class `none` under the same existing error code; the selftest echoes the class; the sandbox gate reads (`child_supervisor.zig`, `sandbox_args.zig`) follow the rename with unchanged semantics.

- **Dimension 4.1** — on a fully capable host the probe asserts substrate `bubblewrap`, class `kernel`, and the four cage guarantees → Test `test_probe_asserts_kernel_class`
- **Dimension 4.2** — a missing mechanism drops exactly its guarantee and names the gap in `mechanisms` prose → Test `test_probe_drops_unproven_guarantee`
- **Dimension 4.3** — a release-mode build refuses class `none` with the existing invalid-config code → Test `test_release_refuses_none_class`
- **Dimension 4.4** — a heartbeat reply in the retired shape decodes to no assignment; the runner refuses to lease, fail-closed → Test `test_stale_reply_fails_closed`

### §5 — Operator surfaces and the doc set

The dashboard's Add-Runner and Edit-Policy pickers offer exactly the class vocabulary; the runner detail renders substrate, guarantees, and the mechanisms prose as reported facts (prose secondary, no controls promising class-aware placement — none exists). `docs/architecture/runner_fleet.md` rewrites §Sandbox tiers into isolation classes and re-words the reconciliation story; the docs-repo branch updates the public field rename and maps each guarantee reason to a runbook repair step. The end-to-end (e2e) proof is the existing stock-runner lane green under the new vocabulary.

**The class-selection table ships with the vocabulary, not after it.** `kernel` and `machine` demand an IDENTICAL guarantee set (§2, Interfaces) — the class floor is the only thing between them — so an operator choosing between them is choosing a threat model, not a capability. A vocabulary that does not say so is three words with no decision rule behind them, which is how a fleet ends up uniformly on one class. `docs/architecture/runner_fleet.md` replaces its tier table with this one, and the docs-repo runbook carries it verbatim:

| Class | What the tenant's lease is promised | Assign it when |
|---|---|---|
| `none` | nothing — no cage is built | own-tenant development work only. A release build refuses it at boot (Invariant 6), so it is never a production answer. |
| `kernel` | a compromised workload cannot reach the host's filesystem, syscalls, processes, or resource pool. Tenants still SHARE a kernel, so a kernel-level escape crosses the boundary. | every ordinary production host — including a runner that is itself inside a container or a VM and forks sandboxed children. Both of today's cage tiers migrate here (§3). |
| `machine` | a compromised workload does not share a kernel with any other tenant's workload; the boundary is the machine. | hostile multi-tenancy, or a compliance regime that forbids a shared kernel. **No substrate reports `machine` until the driver milestone lands (Out of Scope)** — assigning it before then is a deliberate degrade, which is the correct and intended behaviour. |

**The per-lease boundary decides the class — not how the host itself is packaged.** "The runner is a VM" is not `machine`. A runner running *inside* a VM and forking sandboxed children gives every lease on that runner the same kernel: the VM is a boundary against the host, not between two tenants sharing the runner. That is `kernel`, and it is exactly what today's `container_nested` migrates to. `machine` is honest only where the substrate builds a machine boundary PER LEASE — a Firecracker driver spawning a microVM per lease. This is the likeliest mis-assignment the class floor exists to catch, so the runbook names it as the first thing to check on a class-floor degrade, and the architecture doc states the per-lease rule where it used to state the tier table.

- **Dimension 5.1** — the pickers offer exactly `none | kernel | machine` and submit the renamed field → Test `test_policy_picker_class_vocabulary`
- **Dimension 5.2** — the runner detail renders substrate, guarantees, and mechanisms; a degraded row shows its guarantee-vocabulary reason → Test `test_runner_detail_renders_substrate`
- **Dimension 5.3** — a stock runner enrols under class `kernel`, beats, reports capability, leases, and reports against real Postgres and Redis → Test `test_e2e_runner_class_vocabulary`

## Interfaces

```
Wire vocabulary (afd_wire::runner ⇄ src/lib/contract/protocol_policy.zig;
Zig is the fixture source of truth — make wire-fixtures):
  IsolationClass   = none | kernel | machine     (ordered: none < kernel < machine)
  Guarantee        = filesystem_isolation | syscall_filtering | resource_limits
                   | process_containment | egress_control
  AssignedPolicy   = { isolation_class, network_policy, registry_allowlist[],
                       worker_count, extra_binds[] }        (field rename only)
  CapabilityReport = { substrate: string, isolation_class, guarantees[],
                       mechanisms: string }        (replaces five Linux booleans)
  SelftestReport   = { checks[], all_ok, isolation_class, network_policy }
  SelfResponse     = { …, isolation_class, achievable: CapabilityReport }
Verdict rule (afd_runner::reconcile — pure, signature takes only the
assignment and the report's class + guarantees):
  required(none)   = {}          a class that builds no cage demands nothing
  required(kernel) = required(machine)
                   = the four cage guarantees, + egress_control when the
                     assigned network posture is isolating
  degraded unless required(class, network_policy) ⊆ reported guarantees
  AND reported class ≥ assigned class; first unmet guarantee names the reason.
  The class FLOOR is therefore the only thing separating kernel from machine.
  They demand an identical guarantee set, so the operator choosing between
  them is choosing a threat model, not a capability (§5 selection table).
HTTP: POST /v1/runners · PATCH /v1/fleets/runners/{id} · GET /v1/runners/me ·
  GET /v1/fleets/runners[/{id}] — assigned_policy.sandbox_tier → isolation_class;
  no route, verb, auth, or error-code change.
Rows: fleet.runners.sandbox_tier → isolation_class (values migrated);
  capability_report JSONB follows the wire shape; selftest tier echo renamed.
Untouched: every lease verb body, fencing_seq, LEASE_TTL_MS / MAX_RUNTIME_MS,
  both debit points, and all twelve §C. EXECUTE hot-path writes.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unknown class in a stored row | out-of-band edit / future vocabulary | decode voids the whole assignment → degraded, no lease issued (today's fail-closed decode, preserved) |
| Old runner binary reports to the new daemon | deploy skew | report fails decode → treated as not-reported → degraded with the no-report reason; the beat still lands liveness |
| New daemon replies to an old runner binary | deploy skew | `AppliedPolicy` holds nothing → the runner refuses to lease, fail-closed; never executes under a mis-read cage |
| Report exceeds bounds | buggy or hostile host | refused, stored value kept reconciling, beat succeeds — a runner token cannot fail its own liveness (preserved) |
| Migration meets an unexpected tier value | out-of-band row | migration aborts loudly; no partial vocabulary ever lands |
| Class over-assignment | operator assigns `machine` to a bubblewrap host | degraded with the class-floor reason; lease gate closed; recovery is reassignment or a substrate upgrade |
| Class read off host packaging, not the per-lease boundary | operator reads "this runner is a VM / a container" as `machine`, though every lease on it shares one kernel | degraded with the class-floor reason — the same path as over-assignment, and deliberately so: the floor is what makes an honest-looking mistake visible instead of silently promising tenants a boundary that is not there. The runner detail's `mechanisms` prose and §5's selection table name the per-lease rule; recovery is reassignment to `kernel` |
| Isolating egress posture, unproven egress | `allow_list_egress` assigned anywhere today | degraded — `egress_control` is reported only when enforcement is wired (today's behaviour, preserved) |

## Invariants

1. The daemon names no execution mechanism — `git grep -inwE 'landlock|seccomp|cgroup|bubblewrap|bwrap' rustd/crates/afd_runner/src rustd/crates/afd_fleet/src rustd/crates/afd_wire/src` returns zero lines, code and doc-prose alike; mechanisms belong to the runner and the architecture doc.
2. The verdict is a pure function of the assignment and the report's class + guarantee set — signature-enforced: `substrate` and `mechanisms` are not parameters to `reconcile`, so a diagnostic string cannot reach a verdict by construction.
3. Unknown vocabulary fails closed on both sides — daemon-side decode voids the assignment or reads the report absent; runner-side decode holds nothing and refuses to lease (Dimensions 1.2, 4.4).
4. Assigned and achievable never overwrite each other — separate columns, no code path from self-report to assignment (M148 model, unchanged and re-pinned by 3.3).
5. The lease model is untouched — fencing_seq monotonicity, at-most-one-lease-per-fleet, `LEASE_TTL_MS` / `MAX_RUNTIME_MS`, both debit points, and all twelve EXECUTE writes; enforced by the Files Changed table carrying no lease-path file and by rubric R5.
6. Release builds refuse class `none` — the existing compile-mode check, re-vocabularied, same error code (Dimension 4.3).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | — | the four runner metric families, their labels, and all analytics events are untouched; the operator-greppable `degraded_reason` vocabulary moves from mechanism to guarantee sentences, which is a §5 docs/runbook change, not a signal change | — | — | `test_verdict_matrix_guarantee_order` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_wire_class_guarantee_round_trip` | every class/guarantee spelling encodes and re-parses identically in Zig and Rust via regenerated fixtures |
| 1.2 | unit (negative) | `test_wire_unknown_vocabulary_fails_closed` | `landlock_full`, `quantum_cage`, unknown guarantee → decode refuses; nothing resolves to a known value |
| 1.3 | unit (negative) | `test_capability_bounds_reshaped` | over-cap substrate/mechanisms/guarantee list → report refused, beat outcome unchanged |
| 2.1 | unit (negative-heavy) | `test_verdict_matrix_guarantee_order` | (class × report) matrix → pinned verdicts; first unmet guarantee names the reason, fixed order |
| 2.2 | unit (negative) | `test_verdict_class_floor` | all guarantees + reported `kernel`, assigned `machine` → degraded with the class-floor reason |
| 2.3 | unit | `test_verdict_missing_inputs_degrade` | no policy / no report → today's degraded reasons; `none` assignment healthy with no report |
| 3.1 | integration (negative) | `test_assignment_accepts_class_refuses_tier` | `kernel` accepted end to end; `landlock_full` and unknown class → existing validation refusal, no row written |
| 3.2 | integration | `test_migration_maps_every_tier` | seeded rows in all three tiers → mapped classes, verdict columns reset; an alien value aborts, zero rows changed |
| 3.3 | integration | `test_operator_read_exposes_substrate` | self/list/detail carry class, substrate, guarantees, mechanisms; `token_hash` absent from every item shape |
| 4.1 | unit | `test_probe_asserts_kernel_class` | full-capability probe → substrate `bubblewrap`, class `kernel`, four cage guarantees, no `egress_control` |
| 4.2 | unit (negative) | `test_probe_drops_unproven_guarantee` | one mechanism absent → exactly one guarantee missing; `mechanisms` prose names the gap |
| 4.3 | unit (negative) | `test_release_refuses_none_class` | release-mode + class `none` → refused with the existing invalid-config code |
| 4.4 | unit (negative) | `test_stale_reply_fails_closed` | reply carrying the retired field shape → no applied policy; lease refusal, not a permissive default |
| 5.1 | unit (component) | `test_policy_picker_class_vocabulary` | pickers render exactly the three classes and submit `isolation_class` |
| 5.2 | unit (component) | `test_runner_detail_renders_substrate` | detail renders the three report fields; degraded row shows its guarantee-vocabulary reason |
| 5.3 | e2e | `test_e2e_runner_class_vocabulary` | stock runner enrols (`kernel`) → beat → capability stored → lease → report, green on real Postgres + Redis |
| FM | integration (negative) | `test_stored_old_report_reads_absent` | a pre-migration report JSON left in the column → reads as not-reported, degraded reason names it, next beat heals |
| regression | integration (negative) | `test_egress_assignment_stays_degraded` | `allow_list_egress` on a `kernel` runner → degraded exactly as before this change |
| regression | integration | `test_selftest_refusals_preserved` | over-bounds and `all_ok`-disagrees selftests → same refusals as today, class echo renamed |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Verdict parity in class vocabulary (§2) | `cd rustd && cargo test verdict` | exit 0 | P0 | |
| R2 | The daemon names no mechanism (§1, §2) | `git grep -inwE 'landlock\|seccomp\|cgroup\|bubblewrap\|bwrap' rustd/crates/afd_runner/src rustd/crates/afd_fleet/src rustd/crates/afd_wire/src \| wc -l` | `0` | P0 | |
| R3 | Retired vocabulary swept (§1–§5) | `git grep -rnw 'SandboxTier' ; git grep -rnw 'proven_by'` | 0 matches each | P0 | |
| R4 | Stock runner green under the class wire (§5) | `make test-integration-rustd` | exit 0 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S4 and R4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.unit`, `verify.lint`, `verify.version`, `verify.integration`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates; R1–R3 name oracles this spec's own sections create.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no file is deleted; symbols and spellings are.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `SandboxTier` | `git grep -rnw 'SandboxTier'` | 0 matches |
| `sandbox_tier` | `git grep -rnw 'sandbox_tier'` | matches only in the value-migration slot, changelog history, and `docs/v*/done/` |
| `landlock_full` / `container_nested` / `dev_none` | `git grep -rnw '<spelling>'` each | migration slot, changelog history, and `docs/v*/done/` only |
| `proven_by` | `git grep -rnw 'proven_by'` | 0 matches |
| mechanism-named reason constants (`REASON_LANDLOCK_UNAVAILABLE`, `REASON_SECCOMP_UNAVAILABLE`, `REASON_CGROUP_CONTROLLERS_MISSING`, `REASON_BUBBLEWRAP_MISSING`) | `git grep -rnwE 'REASON_(LANDLOCK_UNAVAILABLE\|SECCOMP_UNAVAILABLE\|CGROUP_CONTROLLERS_MISSING\|BUBBLEWRAP_MISSING)'` | 0 matches — §2 replaces them with guarantee-vocabulary reasons; they are imported by `afd_fleet/tests/verdict_matrix.rs` today, so the test re-pin is what removes the last use |
| `tier_wire` | `git grep -rnw 'tier_wire'` | 0 matches |
| `selftest_sandbox_tier` | `git grep -rnw 'selftest_sandbox_tier'` | migration slot only |

## Out of Scope

- **Serverless substrates (Cloudflare Workers / Durable Objects)** — parked by owner directive (Discovery). That includes the broker-vs-push-plane decision, the `managed` class, and every lease-model consequence (heartbeat/renew re-derivation, fencing for push delivery, at-most-once vs at-least-once, new billing hooks). The follow-up serverless milestone owns the broker-vs-push question; this spec's vocabulary is the seam it plugs into — `managed` joins the class enum, and a broker enrols as an ordinary runner with zero further wire change.
- **Firecracker / VM substrate drivers** in `src/runner/` — a follow-up driver milestone with its own risks (image supply, boot latency, networking). This milestone makes those substrates *describable*, not runnable; `machine` is assignable and correctly degrades until a driver reports it.
- **Class-aware placement** — label placement (M85_001) stays the only placement input; the class gates eligibility through the degraded verdict only.
- **Tenant-visible or tenant-requirable isolation class and pricing** — the class stays operator-facing; promotion later is additive (a fleet-level requirement column + a placement filter on the existing label seam).
- **Attestation** — the capability report stays unauthenticated self-assertion; placement trust stays operator-assigned.
- **`extra_binds` semantics on non-kernel substrates** — defined by the driver milestone that first needs them.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator assigns isolation class `machine` to a bubblewrap host and watches it degrade with "isolation class not deliverable" while the same host under `kernel` reads Healthy with `substrate: bubblewrap` and four proven guarantees — the control plane described a substrate whose internals it never learned.
2. **Preserved user behaviour** — every enrolled runner keeps working through the migration (values mapped, verdicts re-reported within one beat); runner operation is unchanged (same token, verbs, backoff); tenants see nothing; lease, billing, and reclaim behaviour byte-identical.
3. **Optimal-way check** — vocabulary at the wire plus mapping at the probe is the shortest path; the unconstrained-optimal adds class-aware placement and tenant promises, deliberately deferred until a second substrate exists to justify them.
4. **Rebuild-vs-iterate** — iterate: M177 already landed the `Guarantee` seam at zero wire cost; this finishes it. Determinism is untouched — the verdict stays a pure function with a pinned matrix.
5. **What we build** — the class/guarantee wire, the substrate-blind verdict, one rename migration, the probe-side mapping, operator surfaces, and the doc set.
6. **What we do NOT build** — substrate drivers, serverless execution, class-aware placement, tenant-visible classes, attestation (each rejected above with its reason).
7. **Fit with existing features** — compounds with M148 reconciliation and M85 label placement (the class rides the same assign-down/report-up rails); the one feature it must not destabilize is lease-issuance gating on the degraded verdict.
8. **Surface order** — API and dashboard move together (the rename forces both); no CLI surface exists for runner administration (Add Runner is dashboard-only, M84_001).
9. **Dashboard restraint** — substrate, guarantees, and mechanisms render as reported facts; no placement controls, no class filters, no quality claims until a second substrate reports real data; mechanisms prose stays secondary.
10. **Confused-user next step** — a degraded row's reason names the unproven guarantee; the runner detail's mechanisms prose names the host-specific gap; the runbook (docs branch, §5) maps each guarantee to its repair step.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five slices in dependency order — wire (Zig-first, fixtures regenerate), verdict, row + API surfaces, runner probe, operator surfaces + docs. Each slice leaves the suite green; the rename lands whole, never half.
- **Alternatives considered:** keeping the `sandbox_tier` names and changing only values — rejected: a column and field named tier holding classes is retained legacy (RULE NLR) and this is the cheapest rename window the project will ever have. A five-class vocabulary (`process`, `managed` now) — rejected: no assignable substrate distinguishes `process` from a degraded `kernel`, `managed` has no substrate until the serverless milestone, and "a class that cannot be applied must not be assignable" (`docs/architecture/runner_fleet.md` §Sandbox tiers). Landing before M181 — rejected: two daemons plus the runner triple every wire edit; the prompt's sequencing recommendation is adopted as this spec's dependency line.
- **Patch-vs-refactor verdict:** this is a **refactor** — one vocabulary swap at an existing seam, behaviour-preserving by test matrix, deliberately touching no lease-model code. The larger refactor (drivers, placement, serverless) is named as follow-up milestones rather than folded in.

## Discovery (consult log)

- **Consults** — the five open questions from the authoring prompt, and their resolutions:
  1. *Cloudflare: broker or push plane?* Parked entirely — see Deferrals below.
  2. *Sequencing:* after M181 cutover, adopting the prompt's own recommendation; encoded in **Depends on** and Batch B7.
  3. *Is the isolation class tenant-visible?* Authoring resolution: operator-facing only; promotion later is additive (Out of Scope names the path). Flagged for Indy's ratification at spec review, before CHORE(open).
  4. *Placement input or reported fact?* Authoring resolution: the **class** is the operator's assignment; the **substrate** is a reported diagnostic nothing branches on (Invariant 2). Flagged for Indy's ratification at spec review.
  5. *How much of the runner side is owned here?* Authoring resolution: the probe-side guarantee mapping and vocabulary only; substrate drivers are a named follow-up (Out of Scope). Flagged for Indy's ratification at spec review.
  The schema rename sits outside the additive default an agent may author alone (`docs/SCHEMA_CONVENTIONS.md` §Migration Model); Indy's approval of this spec is the owner decision that migration cites.
- **Spec amended before CHORE(open) — post-split paths, and the class-selection table that was missing.** Two corrections, both found by reading the tree this spec grades rather than the tree it was drafted against. (1) **Paths.** The spec named `afd_fleet/src/runner/{reconcile,policy,bounds}.rs`, which were correct on the authoring date — `afd_fleet/src/runner/` was created the same day by `5f7beed8b` — and were invalidated three days later by `cf3f75199`, an unrelated crate split that moved the runner surface to `afd_runner`. The consequence was not cosmetic: Invariant 1 and rubric R2 grep `afd_fleet/src` + `afd_wire/src`, which today return 4 hits, all in `afd_wire`, while the 3 files actually holding the mechanism vocabulary (`afd_runner/src/{reconcile,bounds,view/decode}.rs`) were outside the grep. Cleaning `afd_wire` alone would have turned R2 green over untouched code — a rubric row grading a boundary the milestone does not own. `dispatch/write_spec.md` §Authoring discipline — "the spec's invariant/rubric greps must use the same pattern as the discovery grep" — so every path and both greps are corrected to the post-split tree, and `afd_runner/tests/**` joins Files Changed beside `afd_fleet/tests/**`. (2) **Selection guidance.** The spec defined three classes and gave an operator no rule for choosing between them, while §2 makes `required(kernel)` and `required(machine)` identical sets — so the choice is a threat-model decision that nothing in the spec stated. §5 now carries the selection table and the per-lease-boundary rule, Interfaces states the required-set algebra explicitly, and Failure Modes carries the nested-VM mis-assignment as its own row. No Dimension, test, or rubric row was added: the table is a docs deliverable inside §5's existing scope, and the behaviour it describes is already pinned by 2.1 and 2.2.
- **Metrics review** — no analytics or funnel change; no playbook update required (the Metrics table's no-signal row carries the reason).
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — > Indy (2026-08-25 ~19:20): "I would like to skip scoping out the cloudflare durable boxes scoped out." — context: authoring-prompt open question 1; serverless (Cloudflare Workers / Durable Objects) is parked from this milestone entirely, including the broker-vs-push decision; the follow-up serverless milestone picks it up against the class/guarantee seam this spec lands.
