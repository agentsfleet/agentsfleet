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

# M162_001: Closed label values index by enum, generated cells own their accumulator slots, and an unregistered value stops compiling

**Prototype:** v2.0.0
**Milestone:** M162
**Workstream:** 001
**Date:** Aug 11, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — an unvalidated posture string from Postgres silently drops its label today, so a failed run can be counted under the wrong series on an operator dashboard
**Categories:** API, DOCS, OBS
**Batch:** B1 — §1 lands first (it renumbers value indices); §2 and §3 follow and are independent of each other; §4 last
**Branch:** feat/m159-otlp-runtime-metrics (folds into open Pull Request (PR) #597 — continuation of M159/M160/M161)
**Test Baseline:** unit=3547 integration=588
**Depends on:** M161_001 (the generated instrument layer and the family registry this milestone re-indexes)
**Provenance:** LLM-drafted (claude-opus-5[1m], Aug 11, 2026), verified against source on feat/m159-otlp-runtime-metrics
**Canonical architecture:** `docs/architecture/observability.md` §"Label registry — money stays in Postgres"

---

## Overview

**Goal (testable):** A closed label value that has no registry home fails the build instead of being dropped at runtime, generated cells resolve their accumulator slot by comptime offset rather than by hash probe, and both frozen wire suites pass with zero edits.

**Problem:** `agentsfleet.execution.posture` is documented as a closed set with no overflow value, and every omission is supposed to increment `agentsfleet_otel_attribute_omitted_total`. Neither holds. `Attribution.posture` is a raw `[]const u8`, filled at three sites directly from a Postgres column (`service_report.zig:206`, `service_renew.zig:241`, `reclaim.zig:108`) with no validation. When the column holds a spelling the interned table does not know, `addClosedLabel` returns `false`, all eight call sites discard that `false`, and the sample exports without the posture label — folding a self-managed run into the platform series with nothing counted anywhere. `observeInvokeAgentDuration`'s `error_type` parameter has the identical shape, so a new error spelling folds a failure into the no-error series and reads as a success.

**Solution summary:** Closed label values stop being strings at the writer boundary. Each closed enum gets a contiguous comptime block in the interned value table and resolves as `VALUE_BASE[E] + @intFromEnum(v)`, one add, so an unregistered value is a compile error rather than a runtime `false` nobody reads. `Attribution.posture` and `error_type` become their enum types, pushing the parse to the boundary where a bad database value is counted once and explicitly instead of vanishing at the metric writer. The same comptime-offset idea gives generated cells static accumulator slots, leaving the hash table to the evented samples that are the only runtime-varying identities, and a generation stamp retires the per-flush bucket clear. Nothing the exporter emits changes: the wire is graded by the two frozen suites passing untouched.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(obs): index closed label values by enum so an unregistered value cannot compile
- **Intent (one sentence):** An operator reading a dashboard can trust that a run's posture and error verdict are on the series they name, because a value with no registry home stops the build rather than quietly disappearing from the export.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/observability/otel_metrics_dims.zig` — the registry being re-indexed: `dimsFor` is the declarative source, `dedup()` builds today's flat `VALUES`, and `runtimeValueIndex` is the string walk this milestone deletes.
2. `docs/architecture/observability.md` §"Label registry — money stays in Postgres" — the authority on which labels are closed sets and on the rule that every omission is counted. The doc wins until reconciled.
3. `~/Projects/oss/ghostty/src/terminal/modes.zig` — the structural canon: one declarative `entries` table generates both the enum and its packed storage, and `ModeState.set`/`get` turn a runtime enum into a comptime field via `switch (x) { inline else => |c| … }` with zero lookup.
4. `~/Projects/oss/ghostty/src/terminal/ScreenSet.zig` — the generation-counter idiom for §3: a wrapping `+%` bump lets a stale entry identify itself without clearing storage.
5. `src/agentsfleetd/observability/otel_metrics_census_test.zig` — the suite that pins accumulator-slot ownership. Read it before §2, not after it goes red.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/observability/otel_metrics_dims.zig` | EDIT | Per-enum contiguous value blocks replace the deduplicated flat table; `runtimeValueIndex` deleted; `valueIndexOf(E, v)` added |
| `src/agentsfleetd/observability/otel_metrics_payload.zig` | EDIT | `addClosedLabel` becomes enum-typed and infallible; the string overload is removed |
| `src/agentsfleetd/observability/otel_metrics.zig` | EDIT | `Attribution.posture` and `error_type` become enum-typed; call sites stop stringifying |
| `src/agentsfleetd/observability/otel_instruments.zig` | EDIT | `collect()` hands the aggregator the comptime slot it already computes instead of a bare sample |
| `src/agentsfleetd/observability/otel_metrics_aggregate.zig` | EDIT | Static slots for generated cells; generation stamp replaces the per-flush bucket clear |
| `src/agentsfleetd/fleet/service_report.zig` | EDIT | Posture parsed at the Postgres boundary; an unparseable column value counts an omission once, explicitly |
| `src/agentsfleetd/fleet/service_renew.zig` | EDIT | Same boundary parse for the renewal path |
| `src/agentsfleetd/fleet/reclaim.zig` | EDIT | Same boundary parse for the reclaim path |
| `src/agentsfleetd/state/fleet_telemetry_store.zig` | EDIT | Same boundary parse for the telemetry-store path |
| `src/agentsfleetd/observability/otel_metrics_dims_test.zig` | CREATE | Negative coverage for the per-enum index arithmetic and the boundary parse |
| `docs/architecture/observability.md` | EDIT | Record that closed values are enum-indexed and that the posture omission is now counted at the boundary |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (the deleted string-walk lookups leave no commented-out corpse), **NLR** (touch-it-fix-it on the four boundary files), **NLG** (no "legacy value table" framing pre-2.0.0), **UFS** (the value-block base offsets are named constants, never repeated literals), **ORP** (orphan sweep after `runtimeValueIndex` and the dedup helper go).
- **`dispatch/write_zig.md`** — memory safety, `errdefer` on the boundary parse paths, pub shape verdict for every new public declaration, cross-compile both linux targets.
- **`dispatch/write_any.md`** — File & Function Length, the milestone-identifier gate, and the end-of-turn greptile read.
- **`dispatch/name_architecture.md`** — the architecture consult is already recorded in Discovery; the doc wins until reconciled.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — every edited file is `*.zig` | Cross-compile both linux targets; `conn.query()` drain audit unaffected (no new query sites) via `make check-pg-drain` |
| PUB / Struct-Shape | yes — `valueIndexOf` and the enum-typed `addClosedLabel` are new public surface | Shape verdict recorded at PLAN; the removed `runtimeValueIndex` shrinks public surface net |
| File & Function Length (≤350/≤50/≤70) | yes — `otel_metrics_payload.zig` sits at 350 and `otel_metrics.zig` at 346 | Both are at the cap before the diff: the enum-typed writer must land net-neutral or the file splits; split plan decided at PLAN, not mid-EXECUTE |
| UFS (repeated/semantic literals) | yes | Per-enum base offsets are generated, never hand-written; posture and error spellings exist only as enum members |
| UI Substitution / DESIGN TOKEN | no | No TypeScript, no user interface surface |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING yes, others no | The boundary omission counts an existing metric and adds no new log line; no schema change, no new error registry entry |

## Prior-Art / Reference Implementations

- **Reference:** `~/Projects/oss/ghostty/src/terminal/modes.zig` — one declarative table generating both an enum and its storage, with `switch (x) { inline else => |c| … }` converting a runtime enum to a comptime field at zero cost. This milestone applies the same trade ghostty documents in that file: heavier comptime in exchange for types and logic that cannot drift apart.
- **Reference:** `~/Projects/oss/ghostty/src/terminal/ScreenSet.zig` — the wrapping generation counter for §3.
- **Divergence:** ghostty generates one packed field per entry; the interned value table stays a flat array with per-enum base offsets, because the exporter resolves indices to strings at egress and a packed struct would not give a stable numbering to resolve against.
- **In-repo precedent:** `otel_instruments.zig:104` already uses `@Struct(.auto, null, &names, &types, &@splat(.{}))` on Zig 0.16.0, so this extends an idiom the module already ships rather than importing a new one.

## Sections (implementation slices)

### §1 — Closed values index by enum

Each closed enum gets a contiguous block in the interned value table, so a value resolves as its block base plus `@intFromEnum`. Today's `dedup()` collapses equal spellings across enums into one index, which is why a per-enum offset cannot be layered on top of it — the numbering has to change first, and everything downstream compares indices. **Implementation default:** blocks stay in `dimsFor` declaration order because that order already fixes wire emission order, so a reader diffing the table against the wire sees one sequence rather than two.

- **Dimension 1.1** — Every closed enum has a comptime base offset and its members occupy consecutive indices → Test `test_value_blocks_are_contiguous_per_enum`
- **Dimension 1.2** — `valueIndexOf(E, v)` equals `VALUE_BASE[E] + @intFromEnum(v)` for every member of every registered enum → Test `test_value_index_is_base_plus_enum_ordinal`
- **Dimension 1.3** — Two enums sharing a value spelling receive distinct indices, and each still resolves to that spelling at egress → Test `test_shared_spelling_keeps_distinct_indices`
- **Dimension 1.4** — `runtimeValueIndex` and the dedup helper are gone, with no caller left → Test `test_no_runtime_value_lookup_remains`

### §2 — Generated cells own static accumulator slots

`collect()` already computes `CELL_OFFSETS[i] + cell`, the comptime-known slot, then discards it by handing the aggregator a bare sample the aggregator re-derives by Wyhash and probe. Generated cells take slots `[0, TOTAL_CELLS)` outright; the hash table serves only the evented and dynamic-label identities that genuinely vary at runtime. This slice moves accumulator-slot ownership, which is the invariant the census suite pins, so it is graded by that suite passing untouched. **Implementation default:** the aggregator keeps one accumulator array with generated cells occupying the low slots, because a second array would fork `accumulate` and the serializer.

- **Dimension 2.1** — A generated cell folds into its comptime slot with no hash computed → Test `test_generated_cell_skips_hash_probe`
- **Dimension 2.2** — Evented samples allocate only above the generated watermark → Test `test_evented_series_start_above_cell_watermark`
- **Dimension 2.3** — Series ceiling and overflow accounting are unchanged, and an overflow still counts `aggregate_cap` → Test `test_series_ceiling_and_overflow_unchanged`

### §3 — A generation stamp retires the per-flush bucket clear

`Aggregator.init()` clears the whole bucket table every flush. A per-bucket generation stamp compared against the aggregator's current generation makes a stale entry self-identifying, so the storage is never touched. The counter wraps with `+%`, matching the ghostty idiom. **Implementation default:** the generation is `u32` and wraps, because a stale entry from exactly 2^32 flushes ago is indistinguishable only if the bucket was never rewritten in between, which the series ceiling already forbids.

- **Dimension 3.1** — A bucket carrying a prior generation reads as empty without being written → Test `test_stale_generation_reads_empty`
- **Dimension 3.2** — Generation wrap does not resurrect a stale accumulator → Test `test_generation_wrap_does_not_resurrect`

### §4 — The posture boundary parses once and counts explicitly

The four sites that fill `Attribution.posture` straight from a Postgres column parse it into the closed enum at the boundary. An unparseable column value counts one omission with an explicit reason and proceeds without the label, which is what the architecture doc already promises and the code does not currently do. `error_type` receives the same treatment. **Implementation default:** the boundary keeps the measurement and drops only the attribute, matching the model-attribution rule already documented for `gen_ai.request.model`.

- **Dimension 4.1** — `Attribution.posture` and `error_type` are enum-typed, so a string cannot reach the writer → Test `test_attribution_rejects_stringly_typed_posture`
- **Dimension 4.2** — An unparseable posture column counts exactly one omission and still exports the measurement → Test `test_unparseable_posture_counts_one_omission`
- **Dimension 4.3** — A run whose posture parses cleanly is unchanged on the wire → Test `test_parsed_posture_wire_unchanged`

## Interfaces

| Surface | Before | After |
|---|---|---|
| `payload.addClosedLabel` | `(sample, comptime key, val: []const u8) bool` — false on miss, discarded by all eight callers | `(sample, comptime key, val: anytype) void` — enum-typed, infallible; an unregistered value is a compile error |
| `dims.runtimeValueIndex` | `(val: []const u8) ?u16` — walks up to ~120 strings | deleted |
| `dims.valueIndexOf` | absent | `(comptime E: type, v: E) u16` — one add against the enum's comptime base |
| `metrics.Attribution.posture` | `[]const u8` | the closed posture enum |
| `metrics.observeInvokeAgentDuration` | `(wall_ms, error_type: ?[]const u8, attr)` | `(wall_ms, error_type: ?ErrorType, attr)` |
| `aggregate.Aggregator.addAtSlot` | absent | `(slot: usize, sample)` — folds a generated cell into its comptime slot |

No OpenTelemetry Protocol (OTLP) wire surface changes: metric names, label keys, label value spellings, kinds, units, and temporality are all unchanged. No Application Programming Interface (API) endpoint, Command-Line Interface (CLI) command, or user-visible surface is touched.

## Failure Modes

| Failure | Detection | Negative test |
|---|---|---|
| A posture spelling arrives from Postgres that the enum does not know | Boundary parse returns null; one omission counted with an explicit reason | `test_unparseable_posture_counts_one_omission` |
| A new closed value is added to an enum but not to the registry | Build fails at the `valueIndexOf` site | `test_unregistered_enum_value_fails_build` |
| Per-enum blocks renumber indices such that two live series merge | Census suite series count changes | `test_series_ceiling_and_overflow_unchanged` |
| A generated cell writes into an evented series' slot | Census suite slot-ownership assertion | `test_evented_series_start_above_cell_watermark` |
| Generation wrap makes a stale bucket read as live | Generation comparison against the current stamp | `test_generation_wrap_does_not_resurrect` |
| The value table outgrows its index width | Comptime assertion on `VALUES.len` against the index type | `test_value_table_fits_index_width` |

## Invariants

1. **The wire does not move.** `otel_metrics_census_test.zig` and `otel_metrics_egress_test.zig` pass with zero edits. Enforced by: those files being untouched in the diff, checked mechanically.
2. **A closed value with no registry home does not compile.** Enforced by: `valueIndexOf` resolving through a comptime base table, so an absent enum is a compile error, not a runtime branch.
3. **Every omission is counted.** No path drops a label without incrementing `agentsfleet_otel_attribute_omitted_total`. Enforced by: the writer becoming infallible, leaving the boundary parse as the only place a value can be rejected.
4. **Value blocks are contiguous and disjoint.** Enforced by: a comptime assertion that consecutive base offsets differ by exactly the preceding enum's member count.
5. **Generated cells and evented series never share a slot.** Enforced by: evented allocation starting at the `TOTAL_CELLS` watermark, asserted at comptime.
6. **Index width holds.** `VALUES.len` fits the index type after de-duplication is removed. Enforced by: a comptime assertion beside the table.

## Metrics & Observability

No new metric family, label key, or label value is introduced, and no existing one changes spelling. `agentsfleet_otel_attribute_omitted_total{attribute,reason}` gains real traffic on the posture path, where the architecture doc already specifies it and the code currently emits nothing — an operator-visible correction, not a new signal. No product analytics event changes; no funnel or playbook update is required.

## Test Specification (tiered)

**Unit** — the registry arithmetic and the aggregator slot rules: Dimensions 1.1–1.4, 2.1–2.3, 3.1–3.2, 4.1. These run under `make test-unit-all`.

**Integration** — the boundary paths that read a real Postgres column: Dimensions 4.2 and 4.3, exercised through the report and renewal services with an injected out-of-enum posture value, under `make test-integration`.

**Frozen suites (graded, never edited)** — `otel_metrics_census_test.zig` and `otel_metrics_egress_test.zig` are the wire-neutrality proof. They are not extended by this milestone; a change to either means the diff is wrong.

**Memory** — `make memleak` covers the boundary parse paths, which allocate on the report and renewal sites today.

## Acceptance Rubric (single scoring surface)

| # | Outcome | Verify command | Expected | Graded |
|---|---|---|---|---|
| 1 | The wire did not move | `git diff --stat main -- src/agentsfleetd/observability/otel_metrics_census_test.zig src/agentsfleetd/observability/otel_metrics_egress_test.zig` | no output | |
| 2 | Both frozen suites pass | `make test-unit-all` | exit 0 | |
| 3 | The runtime string walk is gone | `grep -rn "runtimeValueIndex" src --include="*.zig"` | 0 matches | |
| 4 | No caller discards a closed-label result | `grep -rn "_ = payload.addClosedLabel" src --include="*.zig"` | 0 matches | |
| 5 | Posture cannot be a bare string | `grep -rn "posture: \[\]const u8" src --include="*.zig"` | 0 matches | |
| 6 | Whole repository lints | `make lint-all` | exit 0 | |
| 7 | Integration suite passes | `make test-integration` | exit 0 | |
| 8 | No leaks | `make memleak` | exit 0 | |
| 9 | Both linux targets cross-compile | `make dry` | exit 0 | |
| 10 | Version stays in sync | `make check-version` | exit 0 | |
| 11 | Unit count grew against the CHORE(open) baseline | `make _lint_zig_test_depth` | unit strictly greater than baseline | |

### Behaviour evals

An operator filtering a dashboard by `agentsfleet.execution.posture` sees every settled run under exactly one posture, and a run whose stored posture is unrecognisable appears as a counted omission rather than as a silent member of the platform series.

## Dead Code Sweep

`runtimeValueIndex`, the `dedup` helper, and `containsString` lose their last callers when §1 lands; `EXTRA_CLOSED_VALUES` loses the entries that existed only to register enum spellings reachable through `label()`. Each is deleted in the same commit as the change that orphans it, per RULE NDC, and the orphan sweep at CHORE(close) re-checks with a repository-wide grep.

## Out of Scope

- Cache-line padding of the generated cell array. Adjacent counters share a line, but contention here is per-request-stage rather than per-packet, and padding without a measurement is speculative.
- Any change to metric names, label keys, label value spellings, kinds, units, or temporality.
- The exporter hardening landed in `ea1222ed4` — idle-window, payload-overflow, and replica-collision behaviour is not touched.
- Retiring the `dynamic` label buffer or the model-attribution budget.

## Product Clarity (authoring record)

1. **Successful user moment** — an operator trusts a posture-filtered dashboard because a run cannot be missing its posture label without that omission being counted.
2. **Preserved user behaviour** — every existing dashboard, alert, and recording rule keeps working; the wire is byte-identical for values that parse today.
3. **Optimal-way check** — counting the drop was the alternative. Making it a compile error is strictly stronger for the closed sets, and the boundary parse still counts the one case that genuinely varies at runtime.
4. **Rebuild vs iterate** — iterate. The declarative registry from M161 is the right shape; this milestone changes how values are indexed within it, not the shape.
5. **What we build** — per-enum value indexing, enum-typed closed labels, static accumulator slots for generated cells, a generation-stamped bucket table, and a posture parse at the database boundary.
6. **What we do NOT build** — no new metric, no new label, no wire change, no cache-line padding, no aggregator rewrite for the evented path.
7. **Fit with existing features** — extends the M161 registry directly and uses the same comptime idiom already shipped in `otel_instruments.zig`.
8. **Surface order** — N/A: no user surface, so nothing is ordered or ranked for a user.
9. **Dashboard restraint** — N/A: no dashboard is added; existing panels gain accuracy, not count.
10. **Confused-user next step** — N/A: no user-facing flow. The operator-facing equivalent is the omission counter, which names both the attribute and the reason.

## Decomposition & alternatives (patch vs refactor)

The patch alternative is to count the `addClosedLabel` miss and leave the strings in place. It was rejected because it makes a preventable class of error permanent: closed sets are closed precisely because their membership is known at build time, so a runtime counter for an impossible-by-construction case is a worse trade than a compile error.

The larger alternative is collapsing the entire module around one generated entries table, deleting the flat value array and the evented probe path outright. Investigation showed M161 already delivered most of that: `otel_metrics_families.zig` and `otel_metrics_dims.zig` are the declarative table, and every consumer already reads it. The remaining delta is exactly this milestone plus the deletions §1 unlocks, so the larger refactor collapses into this one rather than sitting beyond it.

Sequencing is forced: §1 renumbers value indices and everything downstream compares indices, so §2 and §3 land after it. §4 depends on §1's enum-typed writer existing.

## Discovery (consult log)

- **Architecture consult, Aug 11, 2026** — `docs/architecture/observability.md` §"Label registry — money stays in Postgres" and §"The OTLP exporter substrate". The doc lists `agentsfleet.execution.posture` as a closed set with no overflow value and requires every omission to be counted. Source reading found the closed-label path counts nothing: `addClosedLabel` returns `false` and all eight call sites discard it. The code is out of step with the doc; this milestone reconciles the code, leaving the doc's rule intact.
- **Source verification, Aug 11, 2026** — `Attribution.posture` is filled from an unvalidated Postgres column at `service_report.zig:206`, `service_renew.zig:241`, `reclaim.zig:108`, and `fleet_telemetry_store.zig:252`, and from an enum round-trip at `service_billing.zig:197` and `service_report.zig:285`. The raw-column sites are the live hazard; the round-trip sites are safe today only because the enum's spellings happen to be registered.
- **Frozen-suite check, Aug 11, 2026** — neither `otel_metrics_census_test.zig` nor `otel_metrics_egress_test.zig` references `payload.Sample`, `newSample`, `addClosedLabel`, `.labels[]`, `val_idx`, or `key_idx`. They assert emitted output, so they constrain the wire without constraining the internal representation, which is what makes them a usable neutrality proof for a refactor of this size.
- Skill-chain outcomes, deferral quotes, and reviewer verdicts are recorded here as the work proceeds.
