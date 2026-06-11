<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M90_004: Runner reports split token counts — platform token spend actually bills

**Prototype:** v2.0.0
**Milestone:** M90
**Workstream:** 004
**Date:** Jun 12, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — platform token spend bills run-fee-only in production: the renew call posts an empty body and the report sends a single total, while the server prices tokens from split fields that default to zero
**Categories:** API
**Batch:** B4 — after M90_003 (its metering tests are the harness this work extends); independent of the M91 memory family
**Branch:** feat/m90-004-token-splits
**Test Baseline:** unit=1966 integration=172
**Depends on:** the `usezombie/nullclaw` fork commit exposing cumulative split-token accessors (see §1 — the agent already normalizes `prompt_tokens`/`completion_tokens` per response but accumulates only the total; we hold no push or release rights on upstream `nullclaw/nullclaw`, so the accumulate-splits + two-accessor patch rides the fork branch `patch/split-token-accessors-v2026.5.29` at `127b5ac4`, pinned in `build.zig.zon`; drop the fork pin when upstream exposes split accessors)
**Provenance:** LLM-drafted (Claude Fable 5, Jun 12, 2026) — from the cross-model adversarial review of PR #395, which found the under-billing; fix directed by Indy

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` (slice pricing reads cumulative token counts off the renew/report wire; this workstream makes the runner actually send them). No new streams/queues/schemas.

---

## Implementing agent — read these first

1. `src/lib/contract/protocol.zig` — `RenewRequest` (input/cached/output split fields, defaults 0) and `ReportRequest` (same + the legacy single `tokens` total). The server side is DONE; do not touch it.
2. `src/zombied/fleet/service_renew.zig` + `service_report.zig` — how the splits price (via `renewal.buildMeterInputs`); the contract this workstream must satisfy from the runner side.
3. `src/runner/pipe_proto.zig` — the `[type][len][payload]` child→parent framing; `FrameType` is `enum(u8)`.
4. `src/runner/child_supervisor.zig` — the read loop that parses frames AND drives renewal ticks (`h.onTick(h.ctx, now_ms)` at `:316`): frame parsing and renew share ONE thread, so live counters are plain struct fields, no atomics.
5. `src/runner/daemon/loop.zig` (`onTick` hook at `:157`, report build at `:232`) + `src/runner/daemon/control_plane_client.zig` (`renew` posts `""` today).
6. `src/runner/engine/runner.zig` (`agent.tokensUsed()` at `:230`) — the engine boundary where the new split accessors are read.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `fix(m90-004): runner reports split token counts — token spend bills`
- **Intent (one sentence):** Every platform-model token the agent consumes is billed: the runner streams cumulative input/cached/output counts to the parent during the run, sends them on every renew, and sends the final splits on report — so the server's existing pricing stops multiplying rates by zero.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; mismatch → STOP and reconcile.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — **RULE UFS** (frame-type and JSON field names are named constants, cross-runtime identical), **RULE NDC** (no dead plumbing — every new field has a producer and a consumer in the same diff), **RULE TST** (new test files registered in discovery), **RULE TST-NAM** (milestone-free test identifiers), **RULE ESO** (a missing/zero usage frame must not silently substitute stale counters — last-known-cumulative is kept, never invented), **RULE XCC** (cross-compile both Linux targets + test graphs — the runner is Linux-first).
- `dispatch/write_zig.md` — Concurrency (the supervisor/renew single-thread claim must be verified and commented, not assumed), Memory Safety (frame payloads are borrowed; copy before retain), Buffer gate (usage frame payload is fixed-size — stack buffer, no allocation), Build Verification (`make`, not bare `zig build`).
- `docs/LIFECYCLE_PATTERNS.md` §6 — errdefer on any new init path.
- Wire compatibility is an invariant, not a rule-read: old runner ↔ new server and new runner ↔ old server must both keep working (all new fields default to 0 on parse).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all-Zig diff | façade read; cross-compile x86_64+aarch64 prod AND linux test graphs (zombied, runner, lib) |
| PUB / Struct-Shape | yes — new pub fields on `ExecutionResult`, new pub frame type, new accessor calls | per-symbol consumer named in the same diff; zlint `unused-decls` stays `error` |
| File & Function Length | yes | pipe_proto/supervisor/loop edits are small; no file approaches 350 |
| UFS | yes — new wire field names + frame tag | single named consts; JSON keys reuse the protocol struct field names via std.json reflection (no string literals) |
| LOGGING | yes — usage-frame parse errors log | `debug` level (per-frame path is hot); no new err codes |
| ERROR REGISTRY | no — no new UZ codes | N/A |
| SCHEMA / UI / DESIGN TOKEN | no | N/A |

---

## Overview

**Goal (testable):** A live runner executing a platform-model lease produces a renew request whose JSON body carries non-zero cumulative `input_tokens`/`output_tokens` once the agent has consumed tokens, and a report whose splits equal the run's final cumulative counts — proven by a wire-level integration test in which the server's metering rows record a non-zero `token_cost_nanos` without any test code constructing `MeterInputs` directly.

**Problem:** `protocol.RenewRequest`/`ReportRequest` carry split token fields and the server prices from them — but the runner never fills them. `renew` posts an empty body (`control_plane_client.zig`), `report` fills only the legacy single `tokens` total (`daemon/loop.zig:232`), and `ExecutionResult` (`src/lib/contract/execution_result.zig:41`) carries a single `token_count` because the vendored NullClaw agent exposes only `tokensUsed() u64`. Every split defaults to zero server-side; token cost prices as zero; platform runs bill run-fee only.

**Solution summary:** (§1) upstream NullClaw exposes cumulative split accessors (it already normalizes the splits per response) and this repo bumps the pin; (§2) the child engine emits a fixed-size `usage` pipe frame with cumulative splits at each agent-turn boundary and at exit, and the supervisor's read loop folds it into per-lease counters that the renewal tick reads (same thread — plain fields); (§3) renew serializes a `RenewRequest` JSON body from the live counters, `ExecutionResult` gains the split fields, and the report fills them; the server changes not at all; (§4) one wire-level metering test proves the whole path end-to-end.

---

## Prior-Art / Reference Implementations

- **Pipe frame addition** → `src/runner/pipe_proto.zig` existing `FrameType` variants + `child_supervisor`'s frame dispatch — mirror the `activity` frame's emit/parse shape.
- **Renew JSON body** → `ReportRequest` serialization in `control_plane_client.report` (std.json over the protocol struct) — renew adopts the identical pattern.
- **Cumulative-counter discipline** → the affinity-cursor semantics in `src/zombied/fleet/renewal.zig` (cumulatives only ever grow; the server diffs against its cursor — the runner never sends deltas).
- **Wire-level integration harness** → `src/zombied/fleet/service_renew_integration_test.zig` + `renewal_metering_test.zig` (the server-side harness this extends with a runner-shaped client).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `build.zig.zon` | EDIT | bump the `nullclaw` pin to the release with split accessors (§1) |
| `src/runner/engine/runner.zig` | EDIT | read cumulative split accessors at the turn boundary + exit; emit usage frames |
| `src/lib/contract/execution_result.zig` | EDIT | `ExecutionResult` gains `input_tokens`/`cached_input_tokens`/`output_tokens` (defaults 0) |
| `src/runner/pipe_proto.zig` | EDIT | new `usage` `FrameType` + fixed-size payload encode/decode helpers |
| `src/runner/child_supervisor.zig` | EDIT | parse usage frames into per-lease cumulative counters; `onTick` signature carries the snapshot |
| `src/runner/daemon/loop.zig` | EDIT | renew hook passes the snapshot; report fills the split fields from the final result |
| `src/runner/daemon/control_plane_client.zig` | EDIT | `renew` serializes the `RenewRequest` JSON body |
| `src/runner/child_supervisor_result.zig` | EDIT | thread the split fields through the supervisor result fold |
| sibling `*_test.zig` per edit | CREATE/EDIT | per Test Specification |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four slices — upstream dependency + pin (§1), child→parent live plumbing (§2), wire fill (§3), end-to-end proof (§4). The supervisor/renew single-thread property makes §2 plain-field state; no locking design needed.
- **Alternatives considered:** (a) billing the existing single total as `input_tokens` — wrong rate class, mis-bills permanently, rejected; (b) estimating splits by content length — invented numbers in a money path, rejected; (c) patching the vendored `zig-pkg` copy of NullClaw in-tree — forbidden (vendored packages are pinned releases; the fix belongs on the nullclaw source tree — and since we hold no push or release rights on upstream `nullclaw/nullclaw`, it rides the `usezombie/nullclaw` fork as one clean rebasable commit, posthog-zig precedent; full-tree vendor copy rejected at ~204K lines — see Discovery); (d) skipping renew-time counts and only fixing the report — leaves every mid-run slice billing zero tokens until the terminal report, rejected (the renew path is where long runs accrue spend).
- **Patch-vs-refactor verdict:** **patch** — every change extends an existing seam (frame enum, struct fields, JSON body); nothing re-architected.

---

## Sections (implementation slices)

### §1 — Upstream split accessors + pin bump

The `usezombie/nullclaw` fork (branch `patch/split-token-accessors-v2026.5.29`, one commit atop upstream tag v2026.5.29) accumulates `prompt_tokens`/`completion_tokens` alongside the existing `total_tokens` and exposes `promptTokensUsed()`/`completionTokensUsed()` beside `tokensUsed()`. This repo pins the fork commit in `build.zig.zon` and reads the accessors at the engine boundary. Cache-read counts are not yet surfaced upstream — `cached_input_tokens` maps as 0 (the Known-limitation entry in Discovery). Drop the fork pin and return to the upstream tag when upstream exposes split accessors.

- **Dimension 1.1** — upstream release pinned in `build.zig.zon`; engine reads cumulative input/cached/output accessors → Test: E2 (both graphs compile against the new pin) + a unit test pinning that the engine maps accessor values into `ExecutionResult` splits verbatim

### §2 — Child→parent live usage plumbing

The engine emits a `usage` pipe frame (fixed-size payload: three u64 cumulatives) after each agent-turn boundary and immediately before exit. The supervisor's read loop folds frames into per-lease cumulative counters (plain fields — the same thread drives `onTick`); `onTick` gains a usage-snapshot parameter so the renew hook prices from live state. A malformed or absent usage frame keeps the last-known cumulative (RULE ESO: never invent, never reset).

- **Dimension 2.1** — usage frame round-trips child→parent (encode/decode + supervisor fold) → Test `test_usage_frame_round_trip`
- **Dimension 2.2** — renewal tick observes the latest cumulative snapshot (frame then tick → snapshot visible; no frame → zeros) → Test `test_renew_tick_sees_live_usage`
- **Dimension 2.3** — malformed usage frame is dropped with a debug log and the last-known counters survive → Test `test_malformed_usage_frame_keeps_counters`

### §3 — Wire fill (renew body + report splits)

`control_plane_client.renew` serializes a `RenewRequest` JSON body from the snapshot (replacing the empty-string body); `ExecutionResult` gains the three split fields; `daemon/loop.zig` fills `ReportRequest`'s splits from the final result (keeping the legacy `tokens` total for the unchanged consumers). Old-runner compatibility: the server already defaults absent fields to zero — no server edits.

- **Dimension 3.1** — renew body serializes the snapshot; `classifyRenew` behavior unchanged → Test `test_renew_body_carries_cumulative_splits`
- **Dimension 3.2** — report fills splits from `ExecutionResult`; legacy `tokens` total still equals the agent total → Test `test_report_carries_final_splits`

### §4 — End-to-end wire proof

One integration test drives the real wire: a runner-shaped client renews against the live server with a non-zero snapshot, then reports — and the server's `fleet.metering_periods` rows show non-zero `token_cost_nanos` priced off the wire values, with the affinity cursor advanced to the reported cumulatives. No test code constructs `MeterInputs`.

- **Dimension 4.1** — wire-level renew bills tokens (metering row token_cost_nanos > 0; cursor == sent cumulatives) → Test `test_wire_renew_bills_tokens`
- **Dimension 4.2** — wire-level report settles the final slice from body splits → Test `test_wire_report_bills_final_slice`

---

## Interfaces

```
Pipe (child→parent), new frame:
  FrameType.usage, payload = 24 bytes: u64 cumulative_input, u64 cumulative_cached, u64 cumulative_output (little-endian)
Runner-internal callback:
  child_supervisor onTick(ctx, now_ms, usage: UsageSnapshot) RenewDecision   (was: onTick(ctx, now_ms))
HTTP (shapes unchanged — fill only):
  POST /v1/runners/me/leases/{id}/renew — body now sent: {"input_tokens":N,"cached_input_tokens":N,"output_tokens":N}
  POST report — input_tokens/cached_input_tokens/output_tokens now non-zero
Shared wire module:
  contract.execution_result.ExecutionResult += input_tokens/cached_input_tokens/output_tokens (u64, default 0)
Compatibility: every new field defaults to 0 on both sides; old runner ↔ new server and new runner ↔ old server unchanged.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Usage frame malformed/truncated | child bug, partial write at kill | frame dropped, debug log, last-known counters kept; renew continues with previous snapshot |
| No usage frame ever arrives | agent consumed zero tokens, or pre-§1 child binary | renew/report send zeros — identical to today's behavior, never worse |
| Renew body serialization fails | OOM on the frame allocator | renew attempt returns error → `keep` (existing fail-safe: retry next tick; lease expires if persistent) |
| Upstream accessors absent at pin | release not cut yet | CHORE(open) fails loud listing the required exports; no downstream work starts |
| Counter regression across child restart within one lease | new child process restarts cumulative at 0 | supervisor keeps the per-lease maximum (cumulatives only grow); the server's GREATEST clamp is the second guard |

---

## Invariants

1. Cumulative counts sent on the wire never decrease within a lease — supervisor folds with `@max`, enforced by `test_renew_tick_sees_live_usage` (regression case) and the server's GREATEST clamp.
2. Every new wire field has a producer and a consumer in the same diff (RULE NDC) — E8-style grep in VERIFY.
3. The supervisor's frame-parse and renew-tick run on one thread — asserted by a comment citing the read-loop ownership plus the concurrency test exercising frame+tick interleaving on the real loop.
4. Wire compatibility: absent fields parse as zero on both ends — pinned by `test_report_carries_final_splits` (legacy total) and the existing server-side suites running unmodified.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | engine maps accessors → ExecutionResult | accessors prompt=10/completion=5 → result splits (10,0,5) — cached pinned 0 until upstream surfaces it — total unchanged |
| 2.1 | unit | `test_usage_frame_round_trip` | encode (7,1,3) → frame → decode (7,1,3); supervisor counters = (7,1,3) |
| 2.2 | unit | `test_renew_tick_sees_live_usage` | frame (100,0,40) then tick → snapshot (100,0,40); regressed frame (50,0,20) → snapshot stays (100,0,40) |
| 2.3 | unit | `test_malformed_usage_frame_keeps_counters` | 23-byte payload → dropped + counters unchanged |
| 3.1 | unit | `test_renew_body_carries_cumulative_splits` | snapshot (100,0,40) → body JSON fields (100,0,40) |
| 3.2 | unit | `test_report_carries_final_splits` | result (10,2,5,total=17) → report fields (10,2,5) and tokens=17 |
| 4.1 | integration | `test_wire_renew_bills_tokens` | renew body (1000,500,800) on 20s-cursor lease → metering row token_cost_nanos == sliceCharge(...) token term; affinity cursor == (1000,500,800) |
| 4.2 | integration | `test_wire_report_bills_final_slice` | report body splits → settle row priced from wire values; lease reported |

Regression: full `make test` + `make test-integration`; the M90_003 exhaustion/convergence suites must stay green (same metering SQL, new inputs). Idempotency: re-sent renew with identical cumulatives charges ≈0 (existing cursor-diff property, re-pinned by 4.1's second call).

---

## Acceptance Criteria

- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (wire-level metering proof included)
- [ ] `make memleak` clean
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` + linux test graphs (zombied, runner, lib)
- [ ] `gitleaks detect` clean · no production file over 350 lines
- [ ] Wire proof: 4.1/4.2 paste non-zero `token_cost_nanos` from `fleet.metering_periods` — verify: Verification Evidence
- [ ] Old-wire compatibility: server suites pass unchanged (no server file in the diff)

## Eval Commands (post-implementation)

```bash
# E1: wire metering proof
make test-integration 2>&1 | tail -5
# E2: Build — zig build && zig build --build-file build_runner.zig
# E3: Tests — make test
# E4: Lint — make lint-zig 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: producer/consumer pairing (every new field referenced ≥2 sites: fill + read)
grep -rn "cached_input_tokens" src/runner/ src/lib/contract/ | head
```

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| (none expected) | — |

**2. Orphaned references — zero remaining stale uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| empty-body renew (`, "",`) | `grep -n 'post(alloc, path, runner_token, ""' src/runner/daemon/control_plane_client.zig` | 0 matches |

---

## Discovery (consult log)

- **Consults** —
  - **Fork-pin decision (Indy, Jun 12, 2026).** Premise correction at EXECUTE: `gh api repos/nullclaw/nullclaw` shows `push: false, admin: false` for this account — the planned upstream-release path is not ours to drive. > Indy (2026-06-12): "Go with usezombie org" — context: carry the split-accessor patch on a `usezombie/nullclaw` fork (branch `patch/split-token-accessors-v2026.5.29`, commit `127b5ac4`) pinned via `build.zig.zon`, matching the posthog-zig org-pin precedent; full-tree vendor copy rejected (~204K-line dependency). Parity proof: pristine v2026.5.29 and patched suites fail only the same 5 pre-existing redis-environment integration tests (7213 pass / 9 skip / 5 fail each side).
- **Skill chain outcomes** — `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs` results.
- **Deferrals** — Indy-acked verbatim quotes only.
- **Known limitation carried at creation:** the cached-input split bills correctly only once the provider layer reports cache reads separately and upstream surfaces them; until then `cached_input_tokens` rides the wire as 0 and cache reads bill at whatever class the provider folds them into. Named here so it is a decision, not a surprise.

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | comments addressed before human review |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | — | |
| Integration tests | `make test-integration` | — | |
| Wire metering proof | 4.1/4.2 paste | — | |
| Memleak | `make memleak` | — | |
| Lint | `make lint-zig` | — | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | — | |
| Gitleaks | `gitleaks detect` | — | |

## Out of Scope

- Anomaly log/metric on regressed cumulative reports (the M90_003 adversarial follow-up) — separate observability workstream.
- Re-billing semantics across mid-run reclaims (the GREATEST-clamp trade documented in M90_003 Discovery) — product call, not wire plumbing.
- Provider-side cache-read split surfacing in `oss/nullclaw` beyond what its providers already normalize — tracked upstream.
- The `api_runtime` role lockdown grants (M90_003 Discovery note).
