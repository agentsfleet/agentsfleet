# Ripley's Log — M15_002 Zombie Observability

**Date:** Apr 12, 2026: 08:15 PM
**Branch:** `feat/m15-zombie-observability`
**PR:** [#200](https://github.com/usezombie/usezombie/pull/200) — MERGED 2026-04-12T07:53:56Z
**Spec:** `docs/v2/done/M15_002_ZOMBIE_OBSERVABILITY.md`

## What shipped

PostHog `zombie_triggered` / `zombie_completed` events + Prometheus counters (`zombie_{triggered,completed,failed}_total`, `zombie_tokens_total`, `zombie_execution_seconds` histogram) wired through webhook receiver and event loop. Then, on follow-up, sunset 22 pipeline-tier-2 dead-counter symbols that M10_002 missed.

## Decisions made

- **Split `metrics_zombie.zig` out of `metrics_counters.zig`.** Spec said "no new files" but `metrics_counters.zig` was already 327 lines — adding 5 fields + 5 functions + render wiring would cross RULE FLL (350). Amended the spec (§A3) and followed the established split pattern (`metrics_external.zig`, `metrics_histograms.zig`). Global rule beats spec instance.
- **Typed event structs, not `trackX` functions.** Spec prescribed `trackZombieTriggered(client, props) void`. Real codebase uses `telemetry_events.zig` structs with `kind` + `properties()` — the generic `Telemetry.capture(E, event)` dispatch. Conformed to existing pattern; amended spec §A2.
- **End-to-end `wall_ms`, not sandbox-only.** Initial implementation measured only `executeInSandbox`. `/review` flagged that the histogram name implied end-to-end. User confirmed intent — moved `t_start` above the approval gate so the histogram reflects operator-visible latency. Gate-blocked events short-circuit before observation, so the histogram captures only gate-passed events. Documented.
- **Monotonic clock.** Switched `std.time.milliTimestamp()` → `std.time.Instant.now()` to survive wall-clock steps. Graceful degradation to `wall_ms = 0` if `Instant.now()` errors.
- **Histogram resolution in ms.** Initial buckets were integer seconds, so everything under 1s collapsed to one bucket. Reworked to ms-storage (`[100, 500, 1000, ...]`) with render-time conversion to fractional seconds via `{d:.3}`. Metric name stayed `zombie_execution_seconds` — Prometheus base unit is still seconds.
- **Pipeline-tier-2 sunset in this PR.** User explicitly asked for 1-4 (removal of dead symbols, file deletions, test updates, non-code sweep) to happen in this PR rather than a follow-on. Would otherwise have been M10_003. User's reasoning: Greptile would flag the dead code in any case, so better to ship it clean.
- **Kept `incGateRepairExhausted` + `incRunLimit*`.** My dead-code scan showed zero prod callers, but these are recent (M28_001, M17_001) and likely about to be wired up. Refused to sunset them; flagged in the commit message.

## Assumptions surfaced

- `addAgentTokens` vs `addAgentTokensByActor`: only the flat one is live in executor; per-actor variants (echo/scout/warden/orchestrator) are dead. Confirmed by grep before removal.
- `incOrphanRunsRecovered` appeared to have no caller but was still present in telemetry. Kept the telemetry event (still has downstream references in telemetry_events.zig) but removed the metrics counter (no caller).
- Grafana dashboard `agent_run_breakdown.json` had a live "Gate Repair Loop Distribution" panel whose queries referenced the removed histogram. Removed the panel in the same PR to avoid no-data panels post-deploy.

## Dead ends / false starts

- First attempt: tried to keep everything additive in `metrics_counters.zig` + add zombie counters inline. Hit the 350-line gate at 352. Reverted, split into `metrics_zombie.zig`. Cost: one roundtrip through the build.
- Pre-existing telemetry_test bugs (`AuthLoginCompleted` missing `distinct_id`, `initFromSlices` not `pub`) surfaced only because my import chain pulled them into the test tree for the first time. Fixed in-scope rather than fighting why main was passing without them.
- `recordDeliverError` test path crossed module boundary for struct methods — inner `Failure.label()` method needed `pub`. Extracted as a named struct outside the anonymous type literal.
- Spec row 3.1 (`webhook_increments_triggered`) intentionally NOT covered. Requires full HTTP Context + mocked pool + httpz request/response — scope creep vs. what the rest of the tests cover. Left as a P2 follow-on. Event-loop path 3.2 fully covered instead.

## Review pipeline log

- `/review` — 3 informational findings: naming consistency (`zombies_*` → `zombie_*`), exit_status constants, wall_ms scope. All auto/ask-fixed in commit `0e0545c`.
- `/review-pr` — 2 warnings + 2 observations: integration test gap, distinct_id doc, monotonic clock, sub-second buckets. All fixed in `fbd1e46`.
- `/write-unit-test` — mapped spec §6.0 rows 1.1/1.2/1.3/2.1/2.3/3.2 to named tests. Split oversized test files via `telemetry_zombie_test.zig` and `event_loop_obs_integration_test.zig` to preserve 350-gate.

## Follow-ups deferred

- **P2 — webhook integration test (`webhook_increments_triggered`, spec §6.0 row 3.1).** Needs full httpz test context; out of scope for this PR's surgical focus. Primitive (incZombiesTriggered) + event-loop path are covered.
- **P3 — `RunLimit_*` counters + `incGateRepairExhausted` wiring.** Not called from prod yet (M17_001 / M28_001 additions). If they stay uncalled in the next milestone, sunset them too.
- **P3 — schema migration `CHANGELOG.md` reference to `zombie_gate_repair_loops_per_run`.** Historical release note for M28_001 — correctly describes state at that release, no edit needed.
- **v0.10.2 changelog entry for the sunset** — user-visible /metrics shape change. Not yet added; post-merge work means the v0.10.0 changelog (already on main) covers the main shipping event.

## Build & test health at close

- `zig build` ✅
- `zig build test` ✅ (all passing)
- `zig build -Dtarget=x86_64-linux` ✅
- `zig build -Dtarget=aarch64-linux` ✅
- `gitleaks detect` ✅ (pre-existing M9_001 finding is unrelated, in another branch's spec)
- 350-line gate ✅ all touched files

## Hand-off notes

Branch state post-merge-from-main: commits `42e626e` (merge), `0f14919` (sunset), `e23a3fd` (tests), `fbd1e46` (review-pr fixes), `0e0545c` (review fixes), `03772b9` (feature). PR #200 already merged — sunset + test commits are post-merge follow-ups sitting on the branch unless a new PR picks them up.

If picking this up cold: the sunset is safe to re-PR on its own (it's a pure dead-code removal with a negative-test guardrail in `metrics_counters_test.zig`). The extra tests are also standalone.
