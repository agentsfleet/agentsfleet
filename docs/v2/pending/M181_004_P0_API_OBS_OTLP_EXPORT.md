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

# M181_004: OTLP export — all three signals leave the Rust daemon

**Prototype:** v2.0.0
**Milestone:** M181
**Workstream:** 004
**Date:** Sep 01, 2026
**Status:** PENDING
**Priority:** P0 — the cutover's continuity dimension is unprovable while one side of the boundary exports nothing
**Categories:** API | OBS
**Batch:** B6 — runs parallel with M181_002's close; nothing here waits on the route surface except one named producer slice
**Branch:** added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` from the repository's declared `verify.*` commands (`.oracle/orly.json`)
**Depends on:** M181_001 (the metrics pipeline's receiving half: registry, census grading, counting exporter); M181_002 **merged for Dimension 2.1's tenant-library slice only** — the seven `agentsfleet_library_*` producers sit in code that branch carries; every other dimension is independent of it
**Provenance:** LLM-drafted (Claude Opus 5, Sep 01, 2026) — §2 of M181_002, split out on Indy's parallelization call; section prose carried over, not re-derived
**Canonical architecture:** `docs/architecture/observability.md` §The three signal paths + `docs/architecture/runner_fleet.md` §Multi-replica

---

## Overview

**Goal (testable):** a staging daemon with `OTEL_EXPORTER_OTLP_ENDPOINT` set delivers metrics, spans and log records to that endpoint under the supervised task the inventory declares; with the knob unset it boots, serves and exports nothing; with the endpoint unreachable, request latency is unchanged and the drop counter climbs.

**Problem:** the Rust daemon emits no telemetry at all. M176 shipped the span layer and the bounded export wrapper; M181_001 shipped the metrics pipeline's RECEIVING half — the census-graded registry, the error type, the counting exporter. Two things are missing, not one: no transport (an endpoint is configuration, and those crates have none) and no producers (`afd_observability::metrics` has zero callers outside its own crate, against 71 census families the Zig daemon feeds from 38 files). A producer with no transport emits into a process nobody can read; a transport with no producer carries nothing — so both halves ship together here.

**Solution summary:** wire the published OTLP exporter over its HTTP transport into the machinery `afd_observability` was shaped to receive; complete the `semconv` vocabulary and the fleet-delivery span; produce measurements through a small number of tower layers and SDK observable-gauge callbacks rather than seventeen hand-placed call sites; read the OpenTelemetry specification's own knob names with the Zig daemon's vendor spellings as aliases through cutover; and carry logs on the same bounded exporter so the log backend does not go dark at the swap.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(observability): OTLP export — metrics, spans and logs leave the daemon
- **Intent (one sentence):** the Rust daemon exports all three signals over OTLP so metric-family continuity across the cutover becomes provable.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_observability/src/export.rs` — the bounded, drop-counting export wrapper. The transport plugs INTO it, not beside it; its stated property is the property the transport must not break.
2. `docs/architecture/observability.md` §The three signal paths — all three signals leave over OTLP and there is no pull endpoint.
3. `docs/metrics.census.tsv` — the family and label ledger every producer is graded against.
4. `docs/LOGGING_STANDARD.md` §4 + §8A — the export task's boundary pair; per-batch outcomes are `debug`; the endpoint is logged as `source=env:NAME`, never as a value, because the header beside it carries a credential.
5. M181_002's Discovery, "Declared divergence — the registry page emits no metric families yet" — the tenant-library producer slice this spec closes, and why it was not closed there.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_observability/**` | EDIT | the OTLP transport the crate was shaped to receive; the completed `semconv` vocabulary; the log signal |
| `rustd/Cargo.toml` | EDIT | the OTLP exporter dependency and its HTTP transport features — a NEW dependency, not a feature flag on one in the lock |
| `rustd/crates/agentsfleetd/**` | EDIT | boot constructs the exporter from configuration and supervises the flush loop under the inventoried name; preflight gains the standard knobs |
| `rustd/crates/afd_api/**` | EDIT | the tower layers that produce the HTTP-side families; the fleet-delivery span at the report handler |
| `rustd/crates/afd_db/**` · `afd_redis/**` | EDIT | observable-gauge callbacks over the pool state both crates already expose |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (vocabulary constants land with their consumers, never ahead of them), UFS (knob names and task names as named constants), TIM (the export queue bound and flush interval are named numbers), ECL (an unreachable collector is an environment condition, not a defect), TST-NAM, MSID, FLL.
- **`docs/RUST_ERROR_STANDARD.md`** — transport construction composes its sources by `#[from]`; a `map_err` that only relabels is deleted rather than kept.
- **`docs/LOGGING_STANDARD.md`** §4 + §8A — the boundary pair, `debug` batch outcomes, endpoint-as-source; a port preserves the event bytes dashboards match on.
- `dispatch/write_rust.md` — deterministic concurrency tests for the supervised export task; REVIEW cites the reference guideline identifiers.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length | yes | producers are layers, not seventeen call sites; the transport module stays inside the cap or splits along construct/supervise |
| LOGGING | yes | boundary pair on the export task; no new secret surfaces; endpoint never logged as a value |
| MILESTONE-ID | yes | none in source |
| UFS | yes | knob names, task name, queue bound as named constants |
| ERROR REGISTRY | yes | no new client-facing codes; internal export errors ride the crate's error type |
| SCHEMA GUARD | no | no schema change |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_observability/src/export.rs` — the wrapper the transport feeds.
- **Reference:** `src/agentsfleetd/fleet_runtime/metering.zig` (`SPAN_FLEET_DELIVERY`) — the synthesized, retro-dated span shape; a scope-based `#[instrument]` port would be the wrong shape. Rust counterpart site: `afd_api_runner/src/handler/runner/report.rs`.
- **Reference:** the Zig knob surface in `src/agentsfleetd/cmd/serve*.zig` — the vendor spellings accepted as aliases through cutover, and only through cutover.

## Sections (implementation slices)

### §1 — The vocabulary and the delivery span

The Zig daemon opens spans in exactly two production files. The per-request server span is already ported; `SPAN_FLEET_DELIVERY` is not, and it is SYNTHESIZED — built retro-dated from a recorded start epoch plus a capped wall duration where a runner reports completion. Under it sits the vocabulary: `afd_observability::semconv` carries 6 constants against the Zig module's 74, and the 68 missing ones are precisely the GenAI/cost/fleet keys the delivery span's attributes and the census's label columns are made of. They land here rather than earlier because constants ahead of their consumers are dead code at write time (RULE NDC).

- **Dimension 1.1** — the attribute vocabulary is complete: every attribute key the census's label columns name, and every key the delivery span carries, resolves to a `semconv` constant rather than a string literal → Test `test_semconv_covers_every_census_label`
- **Dimension 1.2** — the fleet-delivery span is emitted where a runner reports completion, carrying operation, agent, provider, model, token counts, posture, workspace, tenant and event → Test `test_delivery_span_attributes`

### §2 — The producers

NOT a file-for-file port. Zig's seventeen `http/` emit sites become a small number of tower layers where one layer covers every route; the pool families become SDK observable-gauge callbacks reading state `afd_db` and `afd_redis` already expose. Porting the call-site COUNT would import a structure the SDK exists to replace.

- **Dimension 2.1** — every census family has a producer: each family the registry declares is recorded by a call site the daemon actually reaches, and a family with no producer fails naming it → Test `test_every_census_family_has_a_producer`. The seven `agentsfleet_library_*` families' producers live in handlers M181_002 carries; this dimension's tenant-library slice lands only after that branch merges, and the dependency is stated in the frontmatter rather than discovered.

### §3 — The transport, boot, and the knobs

The OTLP exporter is a NEW dependency bringing a protocol-encoding and HTTP-client subtree; the default is the published exporter over its HTTP transport, matching the wire path the Zig daemon already posts to, which keeps the gRPC stack out of the tree. The alternative — a small exporter over the workspace's existing HTTP client — is a PLAN decision to surface, not an EXECUTE discovery. Knobs are the OpenTelemetry specification's own names; the Zig daemon's vendor spellings are accepted as aliases through cutover so a rollback keeps exporting, and retire with that daemon.

- **Dimension 3.1** — boot constructs the transport from configuration and supervises the flush loop under the inventoried task name; the daemon's real inventory equals its declared background task set, and the task joins on termination → Test `test_boot_supervises_otlp_export`
- **Dimension 3.2** — the standard knobs configure endpoint, headers, protocol and timeout, and the vendor spellings still resolve as aliases with the standard name winning when both are set → Test `test_otlp_endpoint_knob_precedence`

### §4 — Failure posture and the three signals

Stderr stays logfmt regardless: it is the path that works before the exporter exists and after it fails. A transport that carries metrics and spans but not logs would take the log backend dark at the swap with nothing to catch it, so the log signal is graded here on event-name continuity per the logging standard's port rule.

- **Dimension 4.1** — with no endpoint configured the daemon boots and serves, exporting nothing; with an unreachable one, request latency is unchanged and the drop counter climbs → Test `test_export_absent_and_unreachable`
- **Dimension 4.2** — all three signals leave the daemon, and log records carry the event names the Zig daemon emits → Test `test_all_three_signals_exported`

## Interfaces

```
OTEL_EXPORTER_OTLP_ENDPOINT       standard knobs: endpoint, headers, protocol, timeout
                                  vendor spellings accepted as aliases through cutover
OTLP_EXPORT (inventory name)      the supervised flush loop agentsfleetd already inventories
afd_observability::export         the bounded, drop-counting wrapper the transport feeds
stderr                            logfmt, unconditionally — before the exporter and after it
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Export silent after boot | the transport constructed but never supervised | `test_boot_supervises_otlp_export` fails on the inventory comparison |
| Log backend dark at swap | the transport carries metrics and spans but not logs | `test_all_three_signals_exported` fails; the signal nobody checks is the one that disappears quietly |
| Collector outage stalls requests | export back-pressure reaching the request path | impossible by the wrapper's bound: batches drop and the drop counter climbs — `test_export_absent_and_unreachable` |
| Endpoint credential in a log line | the header beside the endpoint carries a secret | endpoint logged as `source=env:NAME`, never a value; LOGGING gate + review |
| A census family with no producer | a family declared and never fed | `test_every_census_family_has_a_producer` fails naming the family |

## Invariants

1. The export path is bounded and drop-counting; a dead collector costs dropped batches, never request latency — `test_export_absent_and_unreachable`.
2. The daemon's supervised inventory equals its declared background task set — `test_boot_supervises_otlp_export` over `agentsfleetd::inventory::BACKGROUND_TASKS`.
3. Every attribute key resolves to a `semconv` constant; a string-literal label fails the vocabulary test — `test_semconv_covers_every_census_label`.
4. Stderr remains logfmt with the exporter present, absent, or failed — graded inside `test_export_absent_and_unreachable`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| every census family, now produced by the Rust daemon | ops | per family ledger | census label columns only | no payload bytes, no credentials | `test_every_census_family_has_a_producer` |
| export task boundary pair and drop counter | ops | export loop start, stop, batch failure | task name, count | endpoint logged as source, never value | `test_boot_supervises_otlp_export` |
| fleet-delivery span | ops | a runner reports completion | the `semconv` GenAI/cost/fleet keys | tenant ids as ids, never payload bytes | `test_delivery_span_attributes` |

No product-analytics changes.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | unit | `test_semconv_covers_every_census_label` | every census label column and delivery-span key resolves to a constant; the diff of the two sets is empty both ways |
| 1.2 | unit | `test_delivery_span_attributes` | the span carries the nine declared attributes, retro-dated from the recorded epoch with the capped duration |
| 2.1 | unit | `test_every_census_family_has_a_producer` | each declared family maps to a reachable producer; a seeded orphan family fails naming it |
| 3.1 | unit | `test_boot_supervises_otlp_export` | real inventory == declared set; the task joins on shutdown within the drain deadline |
| 3.2 | unit | `test_otlp_endpoint_knob_precedence` | standard name wins over alias when both set; alias alone still exports |
| 4.1 | integration | `test_export_absent_and_unreachable` | no endpoint → serving daemon, zero export attempts; unreachable endpoint → drop counter climbs, request latency unchanged |
| 4.2 | integration | `test_all_three_signals_exported` | a collector fixture receives metrics, spans and logs; log records carry the Zig event names |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | All three signals leave the daemon (§1–§4) | `cd rustd && cargo test --package agentsfleetd otlp_` | exit 0 | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration lane green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Lint green | `make lint-all` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S5 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.unit`, `verify.integration`, `verify.lint`, `verify.version`); S6–S7 are the template's hygiene gates. R1 names the oracle this spec's own sections create.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE.

## Dead Code Sweep

N/A — no files deleted. The export-task stub that today waits for cancellation in a test is REPLACED by the real spawn, in the same diff (RULE NDC: the stub must not survive beside its replacement).

## Out of Scope

- The collectors themselves — M181_005 stands them up under the Zig daemon first.
- Continuity grading across the swap — M181_006's soak and swap own `test_signal_continuity`.
- Dashboards: nothing new to show; continuity is the deliverable.
- Zig knob retirement — the vendor aliases retire with that daemon, after cutover.

## Product Clarity (authoring record)

1. **Successful user moment** — an operator points staging at a collector and the existing dashboards fill from the Rust daemon with no renamed series.
2. **Preserved user behaviour** — every request path is byte-identical; export is additive and bounded.
3. **Optimal-way check** — layers and gauge callbacks over seventeen hand-placed call sites; the published exporter over hand-rolling a poster; both choices argued in §2/§3.
4. **Rebuild-vs-iterate** — iterate: the receiving machinery exists and is graded; this adds transport and producers.
5. **What we build** — vocabulary, delivery span, producers, transport, knobs, log signal.
6. **What we do NOT build** — collectors, dashboards, continuity grading, a pull endpoint.
7. **Fit with existing features** — plugs into the M176/M181_001 machinery; must not perturb the request path (Invariant 1).
8. **Surface order** — N/A — no user surface.
9. **Dashboard restraint** — N/A — nothing new to show by design.
10. **Confused-user next step** — an operator with a dark backend reads the export task's boundary pair and drop counter on stderr, which never went away.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four slices — vocabulary+span, producers, transport+boot, failure posture — each independently testable.
- **Alternatives considered:** shipping transport without producers (rejected: a graded pipeline with no input is unobservable); staying inside M181_002 (rejected on Indy's parallelization call — this work shares no oracle with the route surface and idles behind it).
- **Patch-vs-refactor verdict:** this is a **patch** onto machinery shaped to receive it; the refactor was M176/M181_001.

## Discovery (consult log)

> Indy (2026-09-01): "i wanna see what can be batched parallelized and break to smaller PRs?" … "Yes, 5 specs as drawn" — context: this spec is §2 of M181_002, split out to run parallel with that branch's close; prose carried over verbatim where it survives, and the split is recorded in M181_002's Discovery.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
