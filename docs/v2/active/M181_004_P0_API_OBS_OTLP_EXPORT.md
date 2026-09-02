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
**Status:** IN_PROGRESS
**Priority:** P0 — the cutover's continuity dimension is unprovable while one side of the boundary exports nothing
**Categories:** API | OBS
**Batch:** B6 — runs parallel with M181_002's close; nothing here waits on the route surface except one named producer slice
**Branch:** feat/m181-004-otlp-export
**Test Baseline:** `unit=2186 integration=not-run` — `make test-unit-all` on 2026-09-02 at the branch point: cargo workspace 2186 passed (352 ignored, the integration tier), `ui/packages/app` 2410, `ui/packages/website` 175; the `agentsfleet` and `design-system` gates were not reached before the run was stopped on Indy's override below. `verify.integration` is not run at CHORE(open): the full declared `verify.*` set runs once at the boundary (Discovery)
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
- **Handshake (filled at PLAN):** the daemon gains a real OTLP transport for spans, metrics and logs, the vocabulary and producers that give the transport something to carry, and the knobs that point it somewhere — so M181_006 can grade the same families arriving from both binaries.
- **ASSUMPTIONS I'M MAKING:**
  1. `afd_api/**` in Files Changed means the api crate family plus `afd_http`, where admission lives — Dimension 1.2 itself names `afd_api_runner`. The table below is widened to say so.
  2. Dimension 2.1 outranks §2's prose. "Every census family" reaches every crate a family's mechanism lives in — lease polls in `afd_fleet`, sweeps in `afd_runner`, signup in `afd_tenant`, streams in `afd_sse` — not two producer sites. The table is widened to the true blast radius.
  3. A family whose mechanism this daemon does not run gets no fabricated producer. It is declared in a reviewed unproduced ledger with a one-line reason, logged once at boot, and the orphan test still names any family outside both sets. The ledger (§2) is Indy's to accept or strike at REVIEW; a struck row becomes a producer or a census change.
  4. Transport is the published exporter over HTTP — protobuf by default, JSON accepted — on the SDK's own batch threads with its blocking client. A hand-rolled poster over the workspace's HTTP client is rejected: it would re-implement OTLP encoding to save one dependency.
  5. The subscriber installs in `main` before any knob is read; boot attaches the OpenTelemetry layers into a reload slot after preflight, so knobs stay preflight's and `migrate`/`check` never build an exporter.
  6. New test files carry sentence names per `docs/architecture/testing.md`; each Dimension's `Test` cell is amended to the name the file carries, in the commit that adds it.
- **Quality ceiling:** the sound build is the SDK's pipeline with the census-built instrument layer M181_001 left a slot for; a leaner build (skip the instrument layer, emit through `global::meter`) would lose the census grading that makes continuity provable. No larger refactor beats this patch.
- **Surface area:** OpenAPI no · CLI no · user docs **yes** (new operator knobs → `~/Projects/docs` env reference + changelog at CHORE(close)) · release no · schema no · spec-vs-rules: this handshake amends the spec, nothing else.

## Implementing agent — read these first

1. `rustd/crates/afd_observability/src/export.rs` — the bounded, drop-counting export wrapper. The transport plugs INTO it, not beside it; its stated property is the property the transport must not break.
2. `docs/architecture/observability.md` §The three signal paths — all three signals leave over OTLP and there is no pull endpoint.
3. `docs/metrics.census.tsv` — the family and label ledger every producer is graded against.
4. `docs/LOGGING_STANDARD.md` §4 + §8A — the export task's boundary pair; per-batch outcomes are `debug`; the endpoint is logged as `source=env:NAME`, never as a value, because the header beside it carries a credential.
5. M181_002's Discovery, "Declared divergence — the registry page emits no metric families yet" — the tenant-library producer slice this spec closes, and why it was not closed there.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_observability/**` | EDIT | the OTLP transport the crate was shaped to receive; the completed `semconv` vocabulary; the instrument layer, the producer handles, the delivery span, the trace budget; the log signal |
| `rustd/Cargo.toml` · `rustd/Cargo.lock` | EDIT | the OTLP exporter dependency and its HTTP transport features, the two tracing bridges — NEW dependencies, not feature flags on ones in the lock |
| `rustd/crates/agentsfleetd/**` | EDIT | boot constructs the exporter from configuration and supervises the flush loop under the inventoried name; preflight gains the standard knobs; gauge callbacks are registered where their sources are known |
| `rustd/crates/afd_api/**` · `afd_api_runner/**` · `afd_api_tenant/**` · `afd_api_ingress/**` · `afd_http/**` | EDIT | the admission shed counter; the fleet-delivery span, cost families and runner counters at the report handler; signup, library-read and repair-result producers in the handlers that own those operations |
| `rustd/crates/afd_redis/**` · `afd_sse/**` | EDIT | the ready-index and hub-reconnect producers; the stream ceiling's shed and dropped-frame counters; gauge callbacks over the state these crates already expose |
| `rustd/crates/afd_fleet/**` · `afd_runner/**` · `afd_tenant/**` | EDIT | lease-poll and memory producers; retention and repair-dispatch producers; signup outcome producers — each at the one call site that owns the operation |
| `docs/architecture/observability.md` · `docs/architecture/concurrency.md` | EDIT | the export path as built; the `otlp_export` row's real stop path |

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

### §1 — The vocabulary and the delivery span — DONE

The Zig daemon opens spans in exactly two production files. The per-request server span is already ported; `SPAN_FLEET_DELIVERY` is not, and it is SYNTHESIZED — built retro-dated from a recorded start epoch plus a capped wall duration where a runner reports completion. Under it sits the vocabulary: `afd_observability::semconv` carries 6 constants against the Zig module's 74, and the 68 missing ones are precisely the GenAI/cost/fleet keys the delivery span's attributes and the census's label columns are made of. They land here rather than earlier because constants ahead of their consumers are dead code at write time (RULE NDC).

- **Dimension 1.1** — the attribute vocabulary is complete: every attribute key the census's label columns name, and every key the delivery span carries, resolves to a `semconv` constant rather than a string literal → Test `every_census_label_resolves_to_a_constant` — **DONE** (`afd_observability::semconv`, with `no_key_is_spelled_twice` and `every_delivery_span_key_is_namespaced` beside it)
- **Dimension 1.2** — the fleet-delivery span is emitted where a runner reports completion, carrying operation, agent, provider, model, token counts, posture, workspace, tenant and event → Test `the_delivery_span_carries_every_declared_key` — **DONE** (`afd_observability::delivery`, recorded at `afd_api_runner`'s report handler; `the_delivery_span_is_retro_dated_from_the_reported_duration` and `the_delivery_span_is_a_root` carry the shape)

### §2 — The producers — DONE

NOT a file-for-file port. Zig's seventeen `http/` emit sites become a small number of tower layers where one layer covers every route; the pool families become SDK observable-gauge callbacks reading state `afd_db` and `afd_redis` already expose. Porting the call-site COUNT would import a structure the SDK exists to replace.

- **Dimension 2.1** — every census family has a producer: each family the registry declares is recorded by a call site the daemon actually reaches, and a family with no producer fails naming it → Test `every_census_family_has_a_producer` — **DONE** (71 census families: 48 claimed across twelve crates and 23 excused by name in `metrics::produced`, which the same test reads in both directions — the earlier 57/14 split in this cell was stale, and these two are counted from the census file and the ledger array). The test proves a family is CLAIMED, not that its producer is called: the coverage audit found five producers with no call site, so `runner_last_seen_seconds` and `runner_active_leases` gained theirs (heartbeat, lease grant, settled report) and two families whose mechanism this build does not run moved to the ledger. The seven `agentsfleet_library_*` families' producers live in handlers M181_002 carries; this dimension's tenant-library slice lands only after that branch merges, and the dependency is stated in the frontmatter rather than discovered.

**Producer map (PLAN).** Handles are claimed from the census-built instrument layer at boot; a family nobody claims is named by `Instruments::unclaimed`, which is what the test and the boot log both read. Grouped by the site that owns the operation:

| Site | Families |
|---|---|
| `afd_http` admission · `afd_api` router | `api_in_flight_requests` (gauge over the semaphore) · `api_backpressure_rejections_total` |
| `afd_observability` exporter self-signal | `otlp_entries_discarded_total` (the three drop counters) · `otel_attribute_omitted_total`. `otlp_queue_depth`, `http_trace_suppressed_total` and `agentsfleet.telemetry.samples_dropped` are LEDGER rows, not producers — see Discovery |
| `afd_api_runner` report + lease handlers | the five cost families · `runner_*` (through `RunnerMetrics`) · `lease_polls_total` · `lease_poll_candidates_scanned_total` · `lease_poll_db_roundtrips_total` · `memory_*` where the Rust memory path performs the operation |
| `afd_api_tenant` · `afd_tenant` | `signup_*` · `library_*` (the M181_002 slice) |
| `afd_api_ingress` · `afd_runner` repair sweep | `repair_*` — results and correlations at the ingress handler; intents, retries, synthetic events, verifier runs, due-batch and oldest-age at the dispatcher |
| `afd_runner` retention sweep | `runner_retention_swept_total` · `runner_retention_sweep_failures_total` |
| `afd_redis` · `afd_sse` | `fleet_ready_depth` · `fleet_ready_write_failures_total` · `sse_hub_reconnects_total` · `sse_in_flight_streams` · `sse_backpressure_rejections_total` · `sse_dropped_frames_total` |
| `agentsfleetd` flush loop | `worker_running` · `process_resident_memory_bytes` (published each interval, never read in a callback) |

**Unproduced ledger (PLAN — Indy accepts or strikes each row at REVIEW).** Declared once in `afd_observability`, logged at boot with its reason, and excluded from nothing else: a family here still builds no instrument and still fails the orphan test if the census stops declaring it.

| Family | Why this daemon cannot feed it |
|---|---|
| `redis_pool_*` (eight) | `afd_redis` holds one multiplexed connection; there is no pool, so active, idle, dials, overflow, poisoning, reconnects, forced closes and acquire timeouts measure a mechanism that does not exist here |
| `sensitive_request_erased_bytes_total` · `sensitive_response_erased_bytes_total` · `sensitive_response_write_failures_total` | no request- or response-buffer erasure path in the Rust daemon |
| `account_teardown_unregister_failures_total` | the Rust ingress declares `user.deleted` unported (`identity_route.rs`) |
| `fleet_triggered_total` | the Zig daemon declares the family and never increments it; nothing to carry over |
| `agentsfleet_otlp_queue_depth` | the SDK owns the export queue and exposes no depth |
| `agentsfleet_repair_*` — provider results, correlations, intents created, and both latency histograms | the repair-result ingress has no Rust home; `app_route.rs` says so. The dispatcher and the verification run ARE ported and record |
| `agentsfleet_library_cache_outcome_total` | the revision-keyed response cache is a declared non-port, so no read consults one |
| `agentsfleet_library_pool_result_total` | the acquire happens inside the store, where the read path cannot see how it ended |

Every `memory_*` family and the remaining `repair_*` families DO have producers — the ledger closed at fourteen rows, not the open-ended set this table first anticipated.

### §3 — The transport, boot, and the knobs — DONE

The OTLP exporter is a NEW dependency bringing a protocol-encoding and HTTP-client subtree; the default is the published exporter over its HTTP transport, matching the wire path the Zig daemon already posts to, which keeps the gRPC stack out of the tree. The alternative — a small exporter over the workspace's existing HTTP client — is a PLAN decision to surface, not an EXECUTE discovery. **PLAN decision:** the published exporter, HTTP transport, protobuf default with JSON accepted, blocking client on the SDK's batch threads; gRPC is refused at preflight as a knob this build does not carry. The tracing bridges (spans, log records) are two further published crates, pinned to the SDK line already in the lock. Knobs are the OpenTelemetry specification's own names; the Zig daemon's vendor spellings are accepted as aliases through cutover so a rollback keeps exporting, and retire with that daemon.

- **Dimension 3.1** — boot constructs the transport from configuration and supervises the flush loop under the inventoried task name; the daemon's real inventory equals its declared background task set, and the task joins on termination → Test `boot_supervises_the_export_under_its_inventoried_name` — **DONE** (`agentsfleetd::telemetry`; the declared-set half stays `test_the_daemon_supervises_what_it_claims`)
- **Dimension 3.2** — the standard knobs configure endpoint, headers, protocol and timeout, and the vendor spellings still resolve as aliases with the standard name winning when both are set → Test `the_standard_endpoint_outranks_the_vendor_alias` — **DONE** (`preflight::otlp`, with the vendor pair resolving to one basic credential a standard header replaces)

### §4 — Failure posture and the three signals — DONE

Stderr stays logfmt regardless: it is the path that works before the exporter exists and after it fails. A transport that carries metrics and spans but not logs would take the log backend dark at the swap with nothing to catch it, so the log signal is graded here on event-name continuity per the logging standard's port rule.

- **Dimension 4.1** — with no endpoint configured the daemon boots and serves, exporting nothing; with an unreachable one, request latency is unchanged and the drop counter climbs → Test `an_unreachable_collector_costs_spans_and_not_requests` — **DONE** (the absent half is `no_endpoint_supervises_nothing_and_is_not_a_failure` plus `test_boot_to_ready_on_compose`, whose inventory has never carried the export task)
- **Dimension 4.2** — all three signals leave the daemon, and log records carry the event names the Zig daemon emits → Test `all_three_signals_reach_a_collector` — **DONE** (a collector fixture on a real socket receives traces, metrics and logs, and the log body carries a ported event name)

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
| 1.1 | unit | `every_census_label_resolves_to_a_constant` | every census label column resolves to a constant and every constant to a column — the diff of the two sets is empty both ways |
| 1.2 | unit | `the_delivery_span_carries_every_declared_key` | the span carries every declared attribute and no other, retro-dated from the settle by the capped duration, as a root |
| 2.1 | unit | `every_census_family_has_a_producer` | each declared family is claimed by a producer or excused by name; an excuse for a family that HAS a producer fails too |
| 3.1 | unit | `boot_supervises_the_export_under_its_inventoried_name` | a configured endpoint spawns the flush loop under the inventoried name and it joins on shutdown; an unconfigured one supervises nothing |
| 3.2 | unit | `the_standard_endpoint_outranks_the_vendor_alias` | the standard name wins over the alias when both are set; the alias alone still exports; an unusable protocol or timeout faults naming its knob |
| 4.1 | integration | `an_unreachable_collector_costs_spans_and_not_requests` | a refused collector climbs the span-drop counter while the emit stays off the caller's path; no endpoint supervises nothing |
| 4.2 | integration | `all_three_signals_reach_a_collector` | a collector fixture receives all three signal paths; the bodies carry a ported event name and this service's identity |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | All three signals leave the daemon (§1–§4) | `cd rustd && cargo test --package agentsfleetd --all-features --test daemon_suite -- --ignored integration_telemetry` | exit 0, 2 passed | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration lane green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Lint green | `make lint-all` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** R1 was amended at §4 to name the tests that exist: the original filter `otlp_` matched a `test_`-prefixed naming this repository left behind (`docs/architecture/testing.md` §Rust test naming). S1–S5 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.unit`, `verify.integration`, `verify.lint`, `verify.version`); S6–S7 are the template's hygiene gates. R1 names the oracle this spec's own sections create.

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
- **Census ceilings corrected** — > Indy (2026-09-02 08:22): "Correct the five rows" — context: six census rows declared a `fixed:` series ceiling BELOW the product of their own closed label sets (trace suppression 4<5, discarded entries 9<18, both library stage families 24<30, read outcomes 16<27, signup failures 4<6). A ceiling under the real count folds live series into `otel.metric.overflow`, the backstop `runner.rs` says must never fire. Corrected in the census with the rule recorded in its own header, and `every_declared_ceiling_admits_its_label_product` fails on any future understatement. The sixth row was found after the decision and corrected under the same rule.
- **Producers without call sites (coverage audit, 2026-09-02)** — `/orly-write-unit-test` over the whole diff found five `pub` producers that nothing calls, so `every_census_family_has_a_producer` passed while four families could never carry a value. It grades CLAIMED, not CALLED. Three had a real mechanism and gained their call site: `runner::seen` at the heartbeat that writes liveness (`afd_runner/src/heartbeat.rs`), `runner::lease_taken` where a poll grants one (`afd_fleet/src/lease/assign.rs`), `runner::lease_released` where a report wins its fence (`afd_fleet/src/lease/report.rs`) — without them `agentsfleet_runner_last_seen_seconds` and `agentsfleet_runner_active_leases` were registered and permanently empty. Two had none and moved to the unproduced ledger with their producers deleted (RULE NDC): `http_trace_suppressed_total`, because this daemon head-samples nothing and runs no per-class span budget, so nothing is suppressed to count; and `agentsfleet.telemetry.samples_dropped`, because the ring it counted belongs to the daemon being replaced and this build's visible loss is already counted per signal and reason by `otlp_entries_discarded_total`. **Both are NEW rows for Indy's ledger review — twenty-three now.** `metrics::produced::is_excused` was deleted unread in the same pass.
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
- **Verification cadence override** — > Indy (2026-09-02 00:24): "Just run all the clippy, fmt, test-unit-rustd, test-integration-rustd after the code is complete on all the dimension/sections. Avoid running them for every section/dimension since you would end up waiting for the disk space to be pruned and purged" — context: the declared `verify.*` set and the lint pair run ONCE at the milestone boundary; inside a Section the inner loop is `cargo check`/`cargo test -p <crate>` over the crate touched, which proves a package and never satisfies a VERIFY row.
