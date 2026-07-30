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

# M150_001: Runner activity forwarder ships the first frame eagerly

**Prototype:** v2.0.0
**Milestone:** M150
**Workstream:** 001
**Date:** Jul 30, 2026
**Status:** DONE
**Priority:** P1 — customer-facing chat latency; the first visible token of every fleet reply arrives up to `ACTIVITY_FLUSH_WINDOW_MS` late
**Categories:** API
**Batch:** B1 — standalone; no parallel workstream
**Branch:** feat/m150-eager-activity-flush
**Test Baseline:** unit=3266 integration=501
**Depends on:** none
**Provenance:** LLM-drafted (Claude Fable 5, Jul 30, 2026) — grounded in a source read of `forwarders.zig`, `lease_run.zig`, `bundle_extract.zig`
**Canonical architecture:** `docs/architecture/data_flow.md` §C. EXECUTE

---

## Overview

**Goal (testable):** The `ActivityForwarder` ships the first frame of a lease and the first `fleet_response_chunk` immediately on arrival (one POST each, one-shot latches), while every other frame keeps the existing 16-frame / 64 KiB / staleness-window batching unchanged.
**Problem:** When a user chats with a fleet, the first streamed reply frame sits in the runner's activity batch until the staleness window (`ACTIVITY_FLUSH_WINDOW_MS`, currently one full window) elapses — a lone first chunk trips neither the frame cap nor the byte cap. The user watches an idle chat while the model is already answering; perceived time-to-first-token (TTFT) is model TTFT plus up to a whole window.
**Solution summary:** Add two one-shot eager-flush latches to the `ActivityForwarder` batch state machine in the runner daemon: the first frame of the lease flushes on arrival ("the fleet is working" appears instantly), and the first `fleet_response_chunk` flushes on arrival ("the reply started" appears instantly). At most two extra POSTs per lease; the chatty middle of a run batches exactly as today. Wire shapes, frame order, and the best-effort discipline are untouched.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): eager first-frame activity flush cuts perceived reply latency
- **Intent (one sentence):** The first words of a fleet's chat reply appear the moment the model starts answering instead of up to one flush window later.
- **Handshake** (filled at PLAN) — Restatement: the runner's activity batcher currently holds the first frame of every run until a cap or the staleness window trips; this change makes the first frame of the lease and the first response chunk each ship the moment they arrive, exactly once, leaving all other batching untouched — so the chat surface shows life and first words at model speed. `ASSUMPTIONS I'M MAKING:` 1. One `ActivityForwarder` instance is constructed per lease and never reused across leases (grounded in `lease_run.zig` construction). 2. `forward()` is invoked from a single reader thread per child pipe, so the latches need no synchronization. 3. A tag comparison on the frame union identifies `fleet_response_chunk` without touching payload bytes.

## Implementing agent — read these first

1. `src/runner/daemon/forwarders.zig` — the batch state machine being extended; the three flush triggers live in `forward()`
2. `src/runner/daemon/forwarders_test.zig` — the harness pattern to mirror: dead loopback port, flush POSTs fail fast and are swallowed, assertions are on batch state only
3. `src/runner/daemon/lease_run.zig` — the forwarder's owner: construction per lease and the tick that drives `flushIfStale`
4. `docs/architecture/data_flow.md` §C. EXECUTE — where activity frames flow (child stdout → parent → control-plane `activity` verb → publish → live tail)
5. `~/Projects/dotfiles/dispatch/write_zig.md` — Zig discipline and gates for the edited surface

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/daemon/forwarders.zig` | EDIT | two one-shot eager-flush latches on `ActivityForwarder`; flush condition extended |
| `src/runner/daemon/forwarders_test.zig` | EDIT | new Dimension tests; existing cap/staleness tests pre-consume the latches so they keep proving cap behaviour |
| `docs/architecture/scaling.md` | EDIT | one line in the per-request volume story: activity POST volume gains at most two eager POSTs per run |
| `docs/v2/active/M150_001_P1_API_RUNNER_EAGER_ACTIVITY_FLUSH.md` | EDIT | lifecycle moves (pending → active → done) and Dimension DONE marks |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — UFS (named-constants rule: no new repeated literals; the latch condition reuses the existing cap constants), NDC (no dead code at write time: both latches are read by the flush condition in the same diff), FLL (file/function length: `forward()` stays within the function cap)
- `~/Projects/dotfiles/dispatch/write_zig.md` — the edited surface is `*.zig`; memory-safety and lifecycle discipline apply (no allocation changes in this diff)

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `*.zig` edits | cross-compile both linux targets; no new allocations, no lifecycle changes |
| PUB / Struct-Shape | no new pub surface | latches are fields on the existing `ActivityForwarder` struct; no new pub fns |
| File & Function Length (≤350/≤50/≤70) | yes | `forwarders.zig` stays under 350; `forward()` gains ~6 lines, stays under 50 |
| UFS (repeated/semantic literals) | yes | no new literals; booleans + existing constants only |
| MILESTONE-ID | yes | source stays milestone-free — the latch comment explains the perceived-latency why, never spec lineage |
| UI Substitution / DESIGN TOKEN / LOGGING / SCHEMA | no | no UI, no new log lines, no schema |

## Prior-Art / Reference Implementations

- **Reference:** `src/runner/daemon/forwarders.zig` itself — the eager path is a fourth trigger in the same `forward()` flush condition the frame/byte/staleness triggers already share; no new mechanism, no new wire shape. The dead-port test harness in `forwarders_test.zig` is the proof pattern to extend.

## Sections (implementation slices)

### §1 — One-shot eager-flush latches

The `ActivityForwarder` gains two boolean latches, both false at construction (one forwarder per lease, so "per lease" is "per instance"). In `forward()`, after the frame is appended: if the first latch is unconsumed, flush eagerly and consume it; if the appended frame is a `fleet_response_chunk` and the second latch is unconsumed, flush eagerly and consume it. A first frame that IS a response chunk consumes both latches with a single POST. All other flush triggers (frame cap, byte cap, staleness) are unchanged. Flush remains best-effort: a failed eager POST is swallowed and the latch stays consumed — no retry, no error propagation. **Implementation default:** check the union tag with a direct tag comparison (the tag names are the wire discriminators), because the frame value is already in scope in `forward()`.

- **Dimension 1.1** — the first frame of a lease flushes on arrival (batch empties after one `forward()`) → Test `test_first_frame_flushes_eagerly` — **DONE**
- **Dimension 1.2** — the first `fleet_response_chunk` flushes on arrival even when earlier non-chunk frames already consumed the first latch → Test `test_first_chunk_flushes_eagerly_after_earlier_frames` — **DONE**
- **Dimension 1.3** — latches are one-shot: after both are consumed, frames batch per the existing caps (a second chunk does NOT eager-flush) → Test `test_second_chunk_batches_not_eager` — **DONE**
- **Dimension 1.4** — a first frame that is a chunk consumes both latches in one POST (at most two eager flushes per lease, proven at the boundary) → Test `test_first_chunk_first_frame_consumes_both_latches` — **DONE**
- **Dimension 1.5** — a failed eager POST is swallowed and the latch stays consumed; subsequent frames batch normally → Test `test_eager_flush_failure_is_swallowed_and_latched` — **DONE**
- **Dimension 1.6** — a failed frame serialization (allocation failure) drops the frame BEFORE consuming the latch, so the eager ship is preserved for the next frame → Test `test_serialize_failure_leaves_latch_armed` — **DONE**

### §2 — Regression alignment of the existing batch proofs

The existing cap and staleness tests assume the first frame buffers. With eager flush they would flush at frame one and stop proving their caps. Each pre-consumes the latches (set both latch fields before the loop — struct fields are directly settable from the sibling test), so the frame-cap, byte-cap, and staleness proofs keep asserting exactly what they assert today.

- **Dimension 2.1** — frame-cap proof still trips at the frame cap with latches pre-consumed → Test `the frame-count cap auto-flushes and resets the batch` (existing, adjusted) — **DONE**
- **Dimension 2.2** — byte-cap proof still trips before the frame cap with latches pre-consumed → Test `the byte cap auto-flushes before the frame cap` (existing, adjusted) — **DONE**
- **Dimension 2.3** — staleness proof still ships a buffered frame only past the window with latches pre-consumed → Test `flushIfStale ships a buffered frame once the window passes` (existing, adjusted) — **DONE**
- **Dimension 2.4** — serialize-and-join proof still asserts comma-separated batch accumulation with latches pre-consumed → Test `frames serialize on arrival and join into one comma-separated batch` (existing, adjusted) — **DONE**

### §3 — Cadence documentation

`docs/architecture/scaling.md` carries the per-request volume story; the eager path changes that arithmetic by a bounded constant. One sentence records it so the scaling math and the code cannot drift.

- **Dimension 3.1** — scaling.md states the activity batch volume bound: at most two eager POSTs per run, all other frames batched → Test: grep proof in the Acceptance Rubric (R3) — **DONE**

## Interfaces

```
No interface changes. The activity POST body (JSON array of serialized frames),
the frame shapes, the batch caps, and the forwarder's pub fn signatures are all
unchanged. Only flush timing changes.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Eager POST fails | control plane unreachable / slow at first frame | flush swallows the transport error (existing best-effort discipline); batch resets; latch stays consumed so there is no eager retry storm; the live tail degrades to the next batched flush — execution is never disturbed |
| Double eager fire | both latch conditions true on the same frame | one flush consumes both latches; proven at the boundary by Dimension 1.4 |
| Latch starves batching | eager flush misinteracting with cap triggers | flush resets count/bytes exactly as cap flushes do; `forward()` is single-threaded per child pipe reader, so no interleaving exists; Dimension 1.3 proves post-latch batching |

## Invariants

1. At most two eager flushes per `ActivityForwarder` lifetime — enforced by the two boolean latches (set on consumption, never cleared); proven by Dimensions 1.3 and 1.4.
2. Non-eager flush semantics are byte-identical to today — frame cap, byte cap, and staleness window untouched; proven by the §2 regression trio.
3. `forward()` never propagates an error and never blocks execution — the existing best-effort discipline; the eager path adds no new error surface (proven by Dimension 1.5).
4. No new wire shape — the batch POST body remains a JSON array of frames; enforced by reusing `flush()` verbatim.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

No new metrics, events, or log lines: the change is flush timing inside an existing best-effort path, observable through the already-published activity frames themselves. The analytics/funnel playbook needs no update.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_first_frame_flushes_eagerly` | one `forward()` of a non-chunk frame → count and buffer are zero (flushed), first latch consumed |
| 1.2 | unit | `test_first_chunk_flushes_eagerly_after_earlier_frames` | non-chunk frame (eager flush #1), then buffered non-chunk frames, then first chunk → chunk flush fires immediately, batch empties |
| 1.3 | unit | `test_second_chunk_batches_not_eager` | after both latches consumed, a further chunk → count grows, no flush until an existing cap trips |
| 1.4 | unit | `test_first_chunk_first_frame_consumes_both_latches` | first frame is a chunk → one flush, both latches consumed; a following non-chunk frame buffers |
| 1.5 | unit (negative) | `test_eager_flush_failure_is_swallowed_and_latched` | dead-port POST on the eager flush → no error, batch reset, latch consumed, next frame buffers |
| 1.6 | unit (negative, injection) | `test_serialize_failure_leaves_latch_armed` | `FailingAllocator` at the serialize site → frame dropped, latch unconsumed, next frame still ships eagerly |
| 2.1 | unit (regression) | existing frame-cap test, latches pre-consumed | cap-count frames → flush exactly at the cap |
| 2.2 | unit (regression) | existing byte-cap test, latches pre-consumed | oversized frames → byte cap trips before frame cap |
| 2.3 | unit (regression) | existing staleness test, latches pre-consumed | one buffered frame → ships only past the window |
| 2.4 | unit (regression) | existing serialize-and-join test, latches pre-consumed | two frames → one comma-joined batch buffer |
| 3.1 | docs | rubric R3 grep | scaling.md names the two-eager-POST bound |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Eager-flush behaviour proven (§1: both latches, one-shot, failure-swallowed) | `zig build --build-file build_runner.zig test --summary all 2>&1 \| tail -3` | exit 0, `0 fail` in the suite line | P0 |  ✅ `384 pass, 7 skip (391 total)`, exit 0 |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 |  ✅ 4 paths, every one in Files Changed |
| R3 | Cadence documented (§3) | `grep -c "eager" docs/architecture/scaling.md` | ≥1 match | P1 |  ✅ `1` match |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 |  ✅ `✓ All unit lanes passed` (coverage gates 99%+) |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 |  ✅ `✓ All lint checks passed` |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 |  ✅ both build graphs × x86_64-linux + aarch64-linux, exit 0 |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 |  ✅ `no leaks found` |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 |  ✅ no output |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

Pruned standard rows: S3 integration (no HTTP handler, schema, or Redis surface changes — the forwarder is proven at the batch state machine with the dead-port harness, the established pattern for this file), S4 e2e (no new user-visible surface or command; the user-visible effect is timing on an existing surface), S5 memleak (no allocation or lifecycle changes; `std.testing.allocator` in the touched tests already fails on leaks), S9 orphan sweep (no deletions).

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- **Wake-the-poller lease long-poll** (kills the 0–1 s pickup gap; the other second of orchestration latency) — own workstream; different surface (`agentsfleetd` lease handler + runner loop), different review profile.
- **Tuning `ACTIVITY_FLUSH_WINDOW_MS` or adding a cadence knob** — no evidence a smaller window is needed once the eager path lands; a knob without a caller is dead surface.
- **Bundle tar caching** — discovered already implemented in `src/runner/bundle_extract.zig` (content-addressed `.bundle-cache/{hash}.tar`, read-through + atomic write-through); recorded in Discovery, nothing to build.
- **Changelog `<Update>`** — rides CHORE(close) in `~/Projects/docs` (cross-repo, own-branch flow), not this repo's Files Changed table.

---

## Product Clarity (authoring record)

1. **Successful user moment** — you send "Howdy" to a fleet; the moment the model starts answering, the first words appear in your chat — no dead second of an idle screen.
2. **Preserved user behaviour** — every activity frame still arrives, same order, same shapes; the Events tab, SSE live tail, and gap recovery behave identically; chatty runs still batch.
3. **Optimal-way check** — the unconstrained optimum is a persistent streaming channel from runner to daemon (no batching at all); rejected now: the batch verb exists, works, and the entire gap this spec closes is the first-frame hold. Two latches close it with no new infrastructure.
4. **Rebuild-vs-iterate** — patch. A streaming-channel refactor is infrastructure-sized for a one-second problem and would touch the auth plane and connection budget for no additional user-visible win.
5. **What we build** — two one-shot latches in `forward()`, five new unit proofs, three adjusted regression proofs, one scaling.md sentence.
6. **What we do NOT build** — wake-the-poller (own workstream), cadence knob (no caller), streaming channel (disproportionate), bundle cache (already exists).
7. **Fit with existing features** — compounds with the SSE live tail and the M122 client gap recovery (earlier first frame → earlier reconnect anchor); must not destabilize the report path, which this diff does not touch.
8. **Surface order** — N/A — no user surface; internal timing change on an existing flow.
9. **Dashboard restraint** — N/A — no new UI; the existing chat surface simply fills sooner.
10. **Confused-user next step** — N/A — no user-facing control; operators diagnosing live-tail delay read the existing forwarder debug logs, unchanged by this spec.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one behavioural Section (the latches), one regression Section (the existing proofs keep proving), one documentation Section — the smallest split where every Dimension is independently verifiable.
- **Alternatives considered:** (a) shrink `ACTIVITY_FLUSH_WINDOW_MS` to a small value — rejected: multiplies steady-state POST volume for every chatty run, which is exactly what the batching exists to prevent; (b) flush on every empty→non-empty transition — rejected: degenerates to one POST per frame whenever the child emits slower than the window, silently deleting the batching; (c) persistent streaming channel — rejected as disproportionate (see Product Clarity 3–4).
- **Patch-vs-refactor verdict:** this is a **patch** because the batching design is sound and stays; the defect is a single missing trigger (first-frame hold), and the fix is two latches inside the existing state machine. The wake-the-poller workstream is the named follow-up for the remaining orchestration second.

## Discovery (consult log)

- **Consults** — Architecture: `docs/architecture/data_flow.md` §C and `scaling.md` §Event-delivery latency answer the flow; no conflict (doc-silent on flush cadence → §3 lands the doc line in the same PR). Bundle-cache scope check: `bundle_extract.zig` read end-to-end — the cache already exists (content-addressed, atomic, degrade-to-miss), so the second latency item left scope at authoring.
- **Metrics review** — no analytics/funnel playbook update required: no product or operator signal changes; observable effect is timing on already-published frames.
- **Skill-chain outcomes** — `/write-unit-test`: Change-set mode; diff ledger fully resolved; surfaced one real gap (serialize-failure must leave the latch armed) → became Dimension 1.6. `/write-integration-test`: reasoned N/A — no seam changed (no handler/repo/service/schema/Redis surface; tier-2 trigger paths untouched; the diff's two failure paths are unit-tier with deterministic injection); existing integration suite is the regression gate. gstack `/review`: adversarial + testing + maintainability passes in parallel plus a Codex cross-model pass; six informational findings auto-fixed in-branch (three new hardening tests — chunk-latch serialize failure, staleness re-anchor after eager flush, eager+byte-cap coincidence — one strengthened one-shot assertion, the struct doc trigger enumeration extended, and both latches normalized to the guarded consume-once idiom); suite 387 pass / 0 fail after fixes.
- **Review INVESTIGATE findings (disposition: Indy)** — F1 (confidence 7/10): with a slow-but-alive control plane (activity POST taking 1–5 s against the 5 s deadline), the up-to-two eager POSTs block the single read-loop thread before the first chunk is read, which can invert the latency win and consume renewal-tick slack; a mode of the pre-existing synchronous-flush design, new in degree not kind. Candidate fixes: tighter sub-window deadline on eager flushes, or skip-eager-after-slow-flush. F2 (confidence 4/10): back-to-back eager POSTs make the pre-existing unfenced out-of-order publish window (deadline-abandoned POST republishing late) the common case at run start; cosmetic on the live tail by design. Neither blocks merge per the reviewing agent; both recorded here for fix-or-defer. F3 (Codex cross-model pass, corroborates F1 with renewal arithmetic and adds one adjacent gap): the CLI steer flow sends the message and only then opens its Server-Sent Events (SSE) stream, so an unusually fast run can publish its first frames before the subscriber exists — pre-existing race; the old staleness hold masked it with a ~1 s margin the eager path removes. Dashboard chat is unaffected (stream already open); the durable `fleet_events.response_text` always carries the full reply. Candidate fix is CLI-side (subscribe before send) — outside this spec's Files-Changed scope, surfaced for a follow-up call. Codex's remaining findings dispositioned in source: dropped-batch-on-failed-send and the dead-port harness are the pre-existing best-effort doctrine and house proof pattern; POST amplification is bounded at +2 per lease behind the existing admission ceiling.
- **Deferrals** — (none; wake-the-poller is Out of Scope by authoring decision, not a deferral of in-scope work)
