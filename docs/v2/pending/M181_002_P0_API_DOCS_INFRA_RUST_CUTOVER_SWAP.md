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

# M181_002: Cutover — the parity gate, the export, the soak, and the one-move swap

**Prototype:** v2.0.0
**Milestone:** M181
**Workstream:** 002
**Date:** Aug 30, 2026
**Status:** PENDING
**Priority:** P0 — the family's payoff; everything before it is preparation
**Categories:** API | DOCS | INFRA
**Batch:** B6 — family closer, second half; serial after M180_001 merges
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M180_001 **merged** (the full route surface — an in-flight branch is not enough, because §1 annotates the handlers it is still adding); M181_001 (the shipping binary, the metrics pipeline, the lanes, the probe runner); M178_001, M179_001, M177_001, M176_001
**Provenance:** split from the single M181_001 cutover spec (LLM-drafted, Claude Fable 5, Aug 23, 2026) on the axis "needs the full route surface or does not"; this half does
**Canonical architecture:** `docs/architecture/observability.md` §The three signal paths + `docs/architecture/runner_fleet.md` §Multi-replica + `docs/architecture/scaling.md`

---

## Overview

**Goal (testable):** the three production `agentsfleetd` machines serve the Rust binary after a staging soak in which the full-route coverage gate, the black-box HTTP parity suite, the runner parity lane, the dry lane, and the latency and memory budgets all pass against the Rust daemon — with a rehearsed one-move rollback to the warm Zig binary and metric families continuous across the boundary.

**Problem:** six milestones of parity evidence are per-surface. Cutover needs whole-system proof — all routes at once, sustained load, memory over hours, dashboards continuous — plus an exit that is boring: same schema, same stores, binary swap back. Two pieces stand between the evidence and the swap. The route surface is only gradeable once every route serves from Rust, and the Rust daemon exports no telemetry at all, so continuity across the swap is unprovable in the direction that matters.

**Solution summary:** build the served-versus-documented gate against a document the Rust daemon generates from its own handlers; wire the OTLP transport that `afd_observability` was shaped to receive so all three signals leave the daemon; run the staging soak against the budgets M181_001's lanes already refuse to run without; deploy the collectors under the Zig daemon first so infrastructure and binary changes stay separately attributable; rehearse the rollback on staging; and execute the production swap from a runbook whose every step carries an executable probe.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): cutover — route parity gate, OTLP export, soak, swap and rollback
- **Intent (one sentence):** production traffic moves to the Rust daemon behind whole-system proof, with rollback reduced to serving a binary that still speaks the same schema.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. §1's correction below — the served-versus-documented gate this work was once going to extend no longer exists, and the document it graded is now a committed artifact nothing generates. Read why before planning §1.
2. §2's dependency note — the transport is a NEW dependency, not a feature flag on one already in the lock. The distinction is load-bearing in a workspace that audits its dependency tree this carefully.
3. `rustd/crates/afd_api/src/router/mount.rs` + `router/mod.rs` — the total match from route variant to handler. §1 must not break it; the reason is in §1's implementation default.
4. `docs/architecture/observability.md` §The three signal paths — all three signals leave over OTLP and there is no pull endpoint. §2 is what makes that true of the Rust daemon.
5. `docs/architecture/runner_fleet.md` §Multi-replica — the 3-machine production shape, which gauges stay approximate across replicas, and why counters stay exact via sum-by.
6. `docs/LOGGING_STANDARD.md` §8A `[JUDGMENT → EVENT-COMPAT]` — a port preserves the event bytes dashboards match on. §2's log signal is graded on exactly that.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_api/**` | EDIT | §1: the daemon emits its own OpenAPI document from annotations over the existing handlers |
| `rustd/crates/agentsfleetd/**` | EDIT | §1: `routes --json` and `openapi` subcommands, plus `doctor` and `backfill` parity; §2: boot constructs the exporter and supervises the export task; preflight gains the standard knobs |
| `scripts/check_route_coverage.py` | CREATE | §1: the checker comparing the served route dump against the emitted document, both directions |
| `scripts/check_route_coverage_test.py` | CREATE | §1: its self-test, discovered by the existing script self-test lane |
| `rustd/crates/afd_observability/**` | EDIT | §2: the OTLP transport the crate was shaped to receive, and the log signal |
| `rustd/Cargo.toml` | EDIT | §2: the OTLP exporter dependency and its transport features |
| `make/test-parity.mk` | EDIT | §3: the soak's route corpus, once every route serves |
| `deploy/**` | EDIT | §4: drain-swap steps for the 3-machine shape; collector configuration |
| `playbooks/cutover/rust_daemon.md` | EDIT | §4: the runbook's swap rows, verification probes and abort criteria |
| `playbooks/cutover/probes.sh` | EDIT | §4: probes for this milestone's rubric rows |
| `docs/architecture/runner_fleet.md` | EDIT | §4: production-shape note — serving binary and rollback posture |
| `public/openapi.json` | EDIT | §1: regenerated from the daemon rather than hand-maintained |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — TIM (budgets are named numbers), ECL (a provider outage is not a parity defect), NDC, UFS, TST-NAM, MSID, FLL, ORP.
- **`docs/RUST_ERROR_STANDARD.md`** — §2 adds fallible signatures to two crates; transport construction composes its sources by `#[from]`, and a `map_err` that only relabels is deleted rather than kept.
- **`docs/LOGGING_STANDARD.md`** §4 + §8A — the export task emits its boundary pair; per-batch outcomes are `debug`; the endpoint is logged as `source=env:NAME` and never as a value, because the header beside it carries a credential.
- `dispatch/write_rust.md` — deterministic concurrency tests for the supervised export task; REVIEW cites the Microsoft guideline mnemonics.
- `dispatch/write_http.md` → `docs/REST_API_DESIGN_GUIDELINES.md` — §1 annotates the public surface, so the generated document is graded against the design guide.
- `dispatch/write_python.md` — the coverage checker: standard-library parsing, context-managed resources, specific exceptions.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the runbook and architecture edits are published prose.
- `dispatch/verify.md` — done-claims here are exactly the rubric rows; no package-scoped substitutes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes | the route dump is a thin serializer over the route enum; annotations live beside the handlers they document |
| LOGGING | yes | swap and drain steps log scoped events; the export task carries its boundary pair; no new secret surfaces |
| MILESTONE-ID | yes | none in source; runbook is docs (exempt) |
| UFS | yes | budget numbers, drain timeouts, knob names as named constants |
| SPEC TEMPLATE GATE | yes — this file | required sections filled |
| SCHEMA GUARD | no | no schema change — that is the rollback story |
| ERROR REGISTRY | yes | the migration refusal this milestone leans on is an existing registry code, referenced not invented |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_observability/src/export.rs` — the bounded, drop-counting export wrapper. §2's transport plugs into it rather than beside it; the wrapper's stated property is the property the transport must not break.
- **Reference:** `rustd/crates/afd_api/src/router/mount.rs` — the total match from route variant to handler, which is what makes an unported endpoint a compile error. §1's tooling choice is made to preserve it.
- **Reference:** `.github/workflows/deploy-dev*.yml` + `deploy/` — the existing staged deploy, verify and acceptance shape; the cutover reuses its verification pattern rather than inventing one.
- **Reference:** the M175–M180 rubrics — every per-surface oracle re-runs here as a pre-swap checklist; this milestone adds only whole-system proofs.

## Sections (implementation slices)

### §1 — Full-route parity gate

The Rust daemon dumps its served route × method set from the route enum, emits an OpenAPI document generated from its own handlers, and a checker compares the two in both directions. The operations subcommands reach behaviour parity so tooling does not fork.

**Correction — the gate this work was written to extend is gone.** The served-versus-documented checker was deleted along with the whole OpenAPI checking family: the error checker, the URL-shape checker, the bundler, the split YAML sources, the make target and its Continuous Integration job. The reason was structural rather than incidental — the checker read the Zig daemon's route table as its source of truth for what is SERVED, and that daemon is being retired, with no Rust generator to repoint it at.

Two consequences. The committed OpenAPI document now has nothing generating or grading it, so the served-versus-documented direction is unguarded on both daemons. And this section BUILDS the gate rather than extending one.

**Implementation default — annotations over the existing handlers, and NOT the router-integrated variant.** The router-integrated idiom binds path and handler together at the registration site. This router does the opposite deliberately: the mount maps a route variant to a handler as a TOTAL match, and the router mounts from the enumerated route set with templates and scopes coming from route metadata. That totality is load-bearing — it is what makes an unported endpoint a compile error instead of a silent 404, and what the operator route inventory and scope tests key off. Plain annotations give the same generated document while keeping it.

**Sizing, measured while the split was authored:** 97 route variants across 11 enums, 46 mounted, 72 handler functions, 147 public wire types of which 115 carry a lifetime, 97 documented failure codes, against a current document of 70 paths and 45 schemas from 30 hand-written source files. The annotation pass is the bulk; reconciling hand-written prose against generated output is the part that is judgment rather than typing. The wire crate's manifest states it deliberately depends on nothing but its serializer — adding a derive macro there is a decision to take explicitly, not by default.

- **Dimension 1.1** — the route dump equals the Zig daemon's served set exactly, with the difference empty in both directions → Test `test_route_dump_matches_zig_set`
- **Dimension 1.2** — the daemon-emitted document covers every served route × method, and a seeded removal fails the check naming the route → Test `test_coverage_gate_rust_source`
- **Dimension 1.3** — the `openapi` subcommand emits the document the checker grades, and the committed artifact equals what it emits → Test `test_openapi_subcommand_is_the_source`
- **Dimension 1.4** — `doctor` and `backfill` produce parity outcomes on seeded states → Test `test_ops_subcommand_parity`

### §2 — OTLP export: the transport the crate was shaped to receive

The Rust daemon emits no telemetry today, so §4's continuity dimension cannot pass as written: metric-family continuity across the swap is unprovable when one side of the boundary exports nothing.

M176 shipped the machinery — the attribute vocabulary, the route-template span layer, and the bounded export wrapper with its drop counter — and M181_001 shipped the metrics pipeline's RECEIVING half: the registry graded against `docs/metrics.census.tsv`, the error type, the snapshot cells, the counting exporter and the admission spelling.

**Two things are missing, not one, and this section owns both.** The transport is the obvious one — an endpoint is configuration and those crates have none. The other is that nothing PRODUCES a measurement: `afd_observability::metrics` has zero callers outside its own crate, while the Zig daemon emits from 38 production files across nine subsystems for the 71 families the census declares. So the pipeline has a graded shape and no input. Neither half is observable without the other — a producer with no transport emits into a process nobody can read, and a transport with no producer carries nothing — which is why they ship together here rather than one milestone apart.

**The span half is one site and one vocabulary.** The Zig daemon opens spans in
exactly two production files: `http/server.zig`, whose per-request server span
M176 already ported to `afd_api/src/router/trace.rs`, and
`fleet_runtime/metering.zig`, whose `SPAN_FLEET_DELIVERY` has no Rust
counterpart. That span is SYNTHESIZED, not scoped — Zig builds it retro-dated
from a recorded start epoch plus a capped wall duration, called from
`fleet/service_report.zig`, whose Rust counterpart is
`afd_api_runner/src/handler/runner/report.rs`. A scope-based `#[instrument]`
port would be the wrong shape.

Under it sits the vocabulary: `afd_observability::semconv` carries 6 constants
against the Zig module's 74, and the 68 missing ones are precisely the
GenAI/cost/fleet keys the delivery span's attributes and the census's label
columns are made of. They land HERE rather than in M181_001 because constants
ahead of their consumers are dead code at write time (RULE NDC) — the
vocabulary and the emit that uses it have to arrive together.

**The producers are NOT a file-for-file port.** Zig's seventeen `http/` emit sites are seventeen hand-placed call sites; the Rust equivalent is a small number of tower layers where one layer covers every route. The pool families are the same story: `afd_db` and `afd_redis` already expose pool state, so those are SDK observable-gauge callbacks reading a snapshot, not eight hand-placed increments. Porting the call-site COUNT would import a structure the SDK exists to replace. The crate says so itself. Confirming evidence: no OTLP import appears anywhere under the daemon's source, and the export task is inventoried as supervised but spawned only in a test, as a stub that waits for cancellation. Boot never spawns it.

**The transport is a NEW dependency.** The metrics feature was a flag on a crate already in the lock; the OTLP exporter is not in the lock at all. Adding it brings a protocol-encoding and HTTP-client subtree. The alternative — a small exporter over the workspace's existing HTTP client, which is what the Zig daemon does — is a real option in a workspace that audits its tree this carefully, and it is a PLAN decision rather than an EXECUTE discovery. The default is the published exporter over its HTTP transport, matching the wire path the Zig daemon already posts to, which keeps the gRPC stack out of the tree.

**Vendor-neutral by construction.** The Zig daemon's knobs carry a vendor's name inside a vendor-neutral mechanism, and that is the only real coupling in the design. The Rust daemon reads the OpenTelemetry specification's own names for endpoint, headers, protocol and timeout, so an operator points it at a local collector and the collector's own configuration fans out to whichever backend is wanted — chosen in collector configuration, with no daemon redeploy. The vendor spellings are accepted as aliases through cutover so a rollback keeps exporting, and retire with that daemon.

**All three signals, including logs.** The architecture document records that the daemon's logs ride the bounded exporter as well as stderr. A transport that carries metrics and spans but not logs would take the log backend dark at the swap with nothing to catch it, so the log signal is part of this section and is graded on event-name continuity, per the logging standard's port rule. Stderr stays logfmt regardless: it is the path that works before the exporter exists and after it fails.

- **Dimension 2.0a** — the attribute vocabulary is complete: every attribute key the census's label columns name, and every key the delivery span carries, resolves to a `semconv` constant rather than a string literal → Test `test_semconv_covers_every_census_label`
- **Dimension 2.0b** — the fleet-delivery span is emitted where a runner reports completion, carrying operation, agent, provider, model, token counts, posture, workspace, tenant and event → Test `test_delivery_span_attributes`
- **Dimension 2.0** — every census family has a producer: each family the registry declares is recorded by a call site the daemon actually reaches, and a family with no producer fails naming it → Test `test_every_census_family_has_a_producer`
- **Dimension 2.1** — boot constructs the transport from configuration and supervises the flush loop under the inventoried task name; the daemon's real inventory equals its declared background task set, and the task joins on termination → Test `test_boot_supervises_otlp_export`
- **Dimension 2.2** — the standard knobs configure endpoint, headers, protocol and timeout, and the vendor spellings still resolve as aliases with the standard name winning when both are set → Test `test_otlp_endpoint_knob_precedence`
- **Dimension 2.3** — with no endpoint configured the daemon boots and serves, exporting nothing; with an unreachable one, request latency is unchanged and the drop counter climbs → Test `test_export_absent_and_unreachable`
- **Dimension 2.4** — all three signals leave the daemon, and log records carry the event names the Zig daemon emits → Test `test_all_three_signals_exported`

### §3 — Staging soak with budgets

The whole-system proof on staging: the black-box HTTP parity suite, the runner parity lane and the dry lane against the Rust daemon; sustained mixed load through the benchmark lane; chaos probes for the invariant tables — webhook replay, lease fencing under kill, stream reconnect; and the two budgets M181_001 embedded as constants the lane refuses to run without.

**The Zig integration corpus cannot grade the Rust daemon**, and the plan once assumed it could. Three independent reasons, each checkable in one command: the lane was deleted with the rest of the Zig gating; the tests are in-process rather than black-box, importing Zig modules and calling them directly, so there is no HTTP boundary to repoint; and nothing in them names a daemon — the only environment knobs they read are datastore pointers. What the shared stack shares is the DATASTORES, not the request path. Point the corpus at a Rust-served environment and it still exercises Zig handler code: a green run would report a pass rate for the implementation being retired, which is worse than no number because it reads like evidence. M181_001's black-box lane is what replaces it.

**The Zig-side baseline is a manual build.** The Zig binary is still built by the release workflow and by the staging deploy workflow, so a baseline binary falls out of either run — but the frozen revision the comparison needs is pinned deliberately, not taken from whatever last shipped.

- **Dimension 3.1** — the black-box parity suite, the runner parity lane and the dry lane are green against the Rust daemon on staging → Test `test_soak_suites_green`
- **Dimension 3.2** — per-route-class latency is within the budget the lane embeds → Test `test_latency_budget`
- **Dimension 3.3** — resident memory stays within the named ceiling across the soak window under sustained load, with a flat growth trend → Test `test_memory_ceiling_soak`
- **Dimension 3.4** — chaos probes hold mid-soak: replay suppressed, leases fenced, streams reconnect → Test `test_soak_chaos_invariants`
- **Dimension 3.5** — cross-implementation state handoff in both directions: the Rust daemon serves and writes production-shaped state, the Zig daemon then boots on the same stores and resumes serving correctly, and the reverse. Rollback safety is demonstrated, not inferred from "same schema" → Test `test_state_handoff_bidirectional`

### §4 — Collectors, swap, rollback rehearsal, runbook

**The collectors deploy first, under the Zig daemon.** Its endpoint is already configuration, so standing the collectors up in front of the incumbent binary proves the telemetry path with dashboards intact and nothing else changing. Infrastructure change and binary change then land as two separately attributable steps, which is what keeps a swap-day anomaly attributable to the swap.

Rollback is rehearsed BEFORE cutover: staging swaps back to the Zig binary using the runbook and verifies clean service. Production cutover is all-at-once across the three machines with load-balancer drain — mixed-fleet operation is structurally tolerated, since every cross-replica invariant is atomic in the datastores, but it doubles the drift surface, so it is the contingency rather than the plan.

**The rollback story is picked here, not inherited.** The parent spec carried two incompatible statements: that the Zig binary remains built, shipped and warm as the rollback, and that rollback is a hand-dispatched redeploy of a frozen revision no longer built by Continuous Integration. The second premise is false in both pipelines — the release workflow and the staging deploy workflow each build the Zig binary today. PLAN picks one story and the runbook states it once, because the two imply different deploy-knob designs.

The runbook carries the **declared-divergence register** M181_001 seeded, so a parity differ reads a declared difference as declared and an undeclared one as a regression. The rollback path carries **no migration invocation**: rollback serves an older binary against a ledger it already understands, and a migration there is at best a no-op and at worst the one command that can refuse mid-incident. The probe runner asserts the absence rather than trusting the prose.

- **Dimension 4.1** — the collectors serve the Zig daemon's export with dashboards unbroken, before any binary changes → Test `test_collector_path_under_zig`
- **Dimension 4.2** — rollback rehearsal on staging: swap back, verify, recorded in the runbook's evidence section → Test `test_rollback_rehearsal`
- **Dimension 4.3** — an older binary pointed at a newer ledger refuses rather than reaping, and the rollback path invokes no migration → Test `test_rollback_carries_no_migrate_and_refuses`
- **Dimension 4.4** — every runbook probe is a copy-paste command that passes on staging post-swap, and every rubric row of the merged milestones is probe-tagged or manifest-declared → Test `test_runbook_probes`
- **Dimension 4.5** — metric, span and log families are continuous across the swap: no renamed series, no dropped family, dashboards unbroken → Test `test_signal_continuity`

## Parallelization & execution map

(Internal batch labels here sequence THIS milestone's work only; the frontmatter **Batch:** line is the family-level ordering — two different axes, deliberately.)

| Batch | Scope | Runtime · model · reasoning tier | Why |
|---|---|---|---|
| B1 | §1 parity gate | Claude Code · Opus 5 · high | a broad annotation pass with an exact oracle, plus judgment where generated output meets hand-written prose |
| B1 | §2 OTLP transport | Claude Code · Opus 5 · xhigh | a dependency decision, a supervised task, and the signal that §4 grades continuity on |
| B2 (serial) | §3 soak | Claude Code · Opus 5 · xhigh | budget grading and failure triage across the whole system |
| B3 (serial) | §4 collectors, swap, runbook | Claude Code · Opus 5 · max | the irreversible-adjacent step; strongest tier, and the production swap is operator-executed |

The production swap itself is operator-executed from the runbook — the agent prepares and rehearses; Indy pulls the trigger.

## Interfaces

```
agentsfleetd routes --json        route × method dump from the route enum
agentsfleetd openapi              the generated document the checker grades
agentsfleetd doctor|backfill      behaviour parity with the Zig subcommands
scripts/check_route_coverage.py   --served <dump> --spec <document>, both directions
OTEL_EXPORTER_OTLP_ENDPOINT       standard knobs: endpoint, headers, protocol, timeout
                                  vendor spellings accepted as aliases through cutover
Deploy knob                       serving-binary selection per environment
Runbook                           playbooks/cutover/rust_daemon.md — drain order,
                                  probes, abort criteria, one-move rollback
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Soak suite red | a parity defect surviving M175–M180 | cutover blocked; the defect routes back to its owning milestone's surface; no partial swap |
| Latency budget miss | a route class over tolerance | cutover blocked; profile, fix, re-soak; the budget is never widened to pass |
| Memory growth in soak | an unbounded buffer or a leaked task | cutover blocked; the trace is attached; fix and re-soak |
| Coverage gate red | a served route absent from the generated document, or the reverse | the checker names the route and method and the direction; the document is regenerated, never hand-patched to match |
| Export silent after swap | the transport constructed but never supervised | `test_boot_supervises_otlp_export` fails on the inventory comparison, before any swap |
| Log backend dark at swap | the transport carries metrics and spans but not logs | `test_all_three_signals_exported` fails; the signal nobody checks is the one that disappears quietly |
| Mid-swap abort | a probe fails on the first machine | abort criteria trigger rollback of the touched machine; a mixed fleet is tolerated structurally while recovering |
| Post-cutover regression | a defect visible only under production traffic | one-move rollback: serve the warm Zig binary; the schema is untouched by design |
| Older binary meets a newer ledger | a rollback or a stale image starts a binary whose migration set predates the database's | the daemon REFUSES, naming the version it does not know, and changes nothing. It does not reap and proceed. A rollback that trips this has crossed a migration boundary and is not one-move |
| State-handoff regression | the Zig daemon cannot read or resume Rust-written state, or the reverse | the handoff lane goes red and cutover is blocked; serialization is fixed before any swap |
| Dashboard discontinuity | a renamed or dropped series | blocked at §4; series names are parity surface, fixed before the swap |

## Invariants

1. Rollback requires no schema or data migration — enforced by the family rule that no `schema/` change lands in M175–M181, and by `test_rollback_rehearsal`. The daemon enforces it too: a binary whose migration set predates the ledger refuses rather than reconciling. Two consequences: the rollback path carries no migration step, and the invariant is only cheap while the family rule holds — the first release after cutover that adds a migration makes a rollback across that boundary a schema decision, not a binary swap.
2. The Zig binary is reachable as the rollback by exactly ONE documented mechanism, named in the runbook, and a test keeps that mechanism reachable — `test_rollback_rehearsal`. Which mechanism is a PLAN decision; carrying two descriptions of it is the failure this invariant exists to prevent.
3. Budgets are named constants compared mechanically, never prose judgments — `test_latency_budget`, `test_memory_ceiling_soak`.
4. Every declared divergence is in the register before cutover, and the parity oracles read it, so a declared divergence never surfaces as a regression and an undeclared one always does.
5. Every runbook step carries an executable probe; a deviation surfaces as a failed probe run rather than a judgment call — `test_runbook_probes`.
6. Cutover cannot proceed with any M175–M181_001 rubric row ungraded or red — enforced mechanically by the probe runner's row-coverage assert, whose buckets are "covered by a tagged probe" or "named in the exclusion manifest the script prints on every run". An uncovered row, an untagged probe, or an undeclared skip is a red run, not a silent gap.
7. The generated document is the only source of the committed artifact — `test_openapi_subcommand_is_the_source` fails if the artifact is edited by hand.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| the existing metric families, continuity asserted across the swap | ops | unchanged | unchanged | unchanged | `test_signal_continuity` |
| span and log families, continuity asserted across the swap | ops | unchanged | unchanged | no payload bytes, no credentials | `test_all_three_signals_exported` |
| export task boundary pair and drop counter | ops | export loop start, stop, and batch failure | task name, count | endpoint logged as source, never value | `test_boot_supervises_otlp_export` |
| `deploy.serving_binary` (one label on existing deploy telemetry) | ops | deploy or swap | binary name, environment | none needed | `test_rollback_rehearsal` |

No product-analytics changes.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_route_dump_matches_zig_set` | the set difference is empty in both directions |
| 1.2 | integration (negative) | `test_coverage_gate_rust_source` | the full set passes; a seeded removal fails naming route and method |
| 1.3 | integration | `test_openapi_subcommand_is_the_source` | the committed artifact equals the subcommand's output byte for byte |
| 1.4 | integration | `test_ops_subcommand_parity` | doctor and backfill outcomes equal the Zig daemon's on seeded states |
| 2.1 | integration | `test_boot_supervises_otlp_export` | a booted daemon's supervisor inventory equals the declared set, export included, and the task joins on termination |
| 2.2 | unit | `test_otlp_endpoint_knob_precedence` | the standard knob resolves; the vendor spelling alone resolves as an alias; with both set the standard name wins |
| 2.3 | integration (negative) | `test_export_absent_and_unreachable` | no endpoint → boots and serves, exports nothing; unreachable endpoint → request latency unchanged and the drop counter climbs |
| 2.4 | integration | `test_all_three_signals_exported` | a collector receives metrics, spans and logs; log records carry the Zig event names |
| 3.1 | e2e | `test_soak_suites_green` | the parity, runner and dry lanes exit 0 against the Rust daemon |
| 3.2 | e2e | `test_latency_budget` | per-class latency within the embedded budget; the lane refuses to run with it unset |
| 3.3 | e2e (negative-sensitive) | `test_memory_ceiling_soak` | resident memory within the ceiling over the window; growth trend flat |
| 3.4 | e2e (chaos) | `test_soak_chaos_invariants` | replay, fencing and reconnect probes hold under load |
| 3.5 | e2e (negative-sensitive) | `test_state_handoff_bidirectional` | Rust-written live state is served correctly by the Zig daemon after a swap, and the reverse |
| 4.1 | e2e | `test_collector_path_under_zig` | the Zig daemon exports through the collector and every dashboard family still resolves |
| 4.2 | e2e | `test_rollback_rehearsal` | a staged swap back is verified by the probe runner exiting 0 |
| 4.3 | e2e (negative) | `test_rollback_carries_no_migrate_and_refuses` | the rollback section invokes no migration; a binary seeded with a shortened migration set, pointed at the full ledger, refuses and leaves the ledger unchanged |
| 4.4 | e2e | `test_runbook_probes` | the probe runner passes post-swap on staging; every merged rubric row is probe-tagged or manifest-declared, and every probe carries a row tag |
| 4.5 | integration | `test_signal_continuity` | series names and labels are identical across the swap boundary for all three signals |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Full-route parity (§1) | `agentsfleetd routes --json > /tmp/served.json && agentsfleetd openapi > /tmp/spec.json && python3 scripts/check_route_coverage.py --served /tmp/served.json --spec /tmp/spec.json` | exit 0 | P0 | |
| R2 | All three signals leave the daemon (§2) | `cd rustd && cargo test --package agentsfleetd otlp_` | exit 0 | P0 | |
| R3 | Whole-system soak green (§3) | `make test-parity BASE_URL=<rust>` + `make dry-app` (Rust variant) + `make test-handoff` | exit 0 each | P0 | |
| R4 | Budgets hold (§3) | `make bench-cutover` | exit 0 | P0 | |
| R5 | Rollback rehearsed and probes green (§4) | `bash playbooks/cutover/probes.sh` on staging, post-swap and post-rollback | exit 0 both runs | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.lint`, `verify.unit`, `verify.version`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates (secret scan, oversize sweep), deliberately outside the declared set; R-rows name oracles this spec's own Files Changed create, so every command is copy-paste by merge time.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE. The production swap additionally requires Indy's explicit go in Discovery.

## Dead Code Sweep

N/A — no files deleted. The Zig daemon's retirement is a separate post-cutover milestone, and its binary remains the rollback for the whole of this one. M181_001 carried the family's only sweep.

## Out of Scope

- Deleting Zig source. Its lanes are already gone; the source and binary stay, because the binary IS the rollback.
- Any behaviour improvement on a live surface — see the parity rule below, which bounds what the port owes rather than freezing every superseded path into it.
- New dashboards, or canary infrastructure beyond the binary-selection knob.
- Public docs (`~/Projects/docs`): no endpoint, command, flag or behaviour change ships, so no docs-repository branch — recorded here as the why-not. The `openapi` and `routes` subcommands are operator tooling on the daemon binary, not the product's command-line surface.

**Single-implementation parity.** The Rust daemon implements exactly ONE implementation of each behaviour — the current one. Where the Zig daemon carries a superseded or compatibility path alongside it, the Rust port implements only the current path; the Zig copy is left in place and retires with that daemon. Live observable behaviour stays at parity: anything a client actually reaches today behaves identically, and the parity oracles compare the current path. "Superseded" is a claim requiring evidence recorded in Discovery — no in-tree emitter, plus Indy's sign-off on the specific path — never the implementing agent's judgment alone, and every instance is written into the declared-divergence register the cutover reads.

---

## Product Clarity (authoring record)

1. **Successful user moment** — production runs the Rust daemon for a full day: fleets wake, leases complete, dashboards continuous, and nobody outside the team can tell anything changed. The rollback runbook stays unused.
2. **Preserved user behaviour** — everything; that is the entire milestone.
3. **Optimal-way check** — an all-at-once swap with a rehearsed rollback beats a rolling mixed fleet: the invariants tolerate mixing, but a single boundary keeps triage unambiguous, and the canary path is named as the contingency in the runbook. Deploying the collectors under the incumbent binary first beats deploying them with the swap, because it turns one ambiguous change into two attributable ones.
4. **Rebuild-vs-iterate** — N/A at this milestone; it ships proof and process, not new architecture.
5. **What we build** — the parity gate, the OTLP transport for all three signals, the soak with graded budgets, the collectors, the runbook and the swap.
6. **What we do NOT build** — Zig retirement, behaviour changes, new dashboards, canary infrastructure beyond the selection knob.
7. **Fit with existing features** — rides the existing deploy and verify workflow shape; must not destabilize the release path for the Zig binary, which remains the rollback.
8. **Surface order** — N/A — operational milestone; no new user surface.
9. **Dashboard restraint** — nothing new to show; continuity is the deliverable, and a new panel at cutover would be indistinguishable from a regression.
10. **Confused-user next step** — an operator mid-incident opens the runbook; every step has a probe and an abort criterion, and the divergence register tells them what genuinely differs between the binaries.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four slices — prove the surface, export the signals, soak against budgets, swap behind a rehearsal — ordered so every irreversible-adjacent step is preceded by the thing that would have caught it.
- **Alternatives considered:** running this as one milestone with the preparation work (rejected: the preparation half is blocked on nothing and carries both unknowns, so serializing it behind the ingress port idles the risky work); a rolling per-machine cutover as the plan (rejected: a mixed fleet doubles the drift surface for little gain; kept as the contingency); repointing the Zig integration corpus at the Rust daemon (rejected on three independent structural grounds recorded in §3 — it would report a pass rate for the implementation being retired).
- **Patch-vs-refactor verdict:** this is a **patch** to the operational layer — pipelines, lanes, runbook — plus one genuinely new surface, the generated OpenAPI document. The refactor was M176–M180; this milestone proves it and moves the traffic.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
