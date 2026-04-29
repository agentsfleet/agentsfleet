# M42_002: Production-Binary Test Harness — Wire-Level Redaction + Pub/Sub-Failure Coverage

**Prototype:** v2.0.0
**Milestone:** M42
**Workstream:** 002
**Date:** Apr 28, 2026: 09:35 PM
**Status:** PENDING
**Priority:** P2 — invariant is currently protected by code review; mechanical regression gate is desirable but not launch-blocking.
**Categories:** API
**Batch:** —
**Depends on:** M42_001 (streaming substrate — landed; recorder fixture + comptime executor harness already in place).

## Why this is its own workstream

M42_001 ships three test rows the spec asks for that the comptime-gated harness cannot cleanly exercise:

1. `test_executor_args_redacted_at_sandbox_boundary`
2. `test_args_redacted_no_secret_leak`
3. `test_pubsub_failure_does_not_block` — the spec wants PUBLISH-only failure with XADD/XACK still working. No clean injection point exists in the worker's `queue_redis.Client` without adding test-only state to production code (rejected during M42_001 review). The realistic path is a Redis ACL user without `+publish` permission, used by the worker for the test scope.

The redaction logic lives in `src/executor/runner_progress.Adapter` — the NullClaw observer/stream-callback adapter that intercepts tool-use and response-chunk events, scans the `args` payload for resolved secret bytes, and substitutes the placeholder before encoding the frame. The harness binary built with `build_options.executor_harness = true` **comptime-strips that path entirely** because `runner.execute` short-circuits to `runner_harness.execute` before any of NullClaw's observer pipeline runs.

A test that asserts redaction therefore needs:

- The production `zombied-executor` binary (NullClaw + tool builders + adapter all included).
- A stub or recorded LLM provider that emits a deterministic tool_use event (otherwise the test spends real tokens, costs money, and is non-deterministic).
- A real tool spec containing `${secrets.fly.api_token}` so the substitution path fires.
- The RpcRecorder fixture (already in tree at `src/zombie/test_rpc_recorder.zig`) for the byte-level assertion.

## Scope

### Files to add

- `src/executor/test_stub_provider.zig` — a NullClaw `Provider` implementation that returns a canned response containing one tool_use event whose args reference `${secrets.fly.api_token}`. ~150 lines.
- `src/zombie/event_loop_harness_redaction_test.zig` — drives the production binary spawn (parallel to `test_executor_harness.zig` but pointed at `zig-out/bin/zombied-executor`), wires the stub provider via env var, sends a real StartStage with `agent_config.api_key = SYNTHETIC_SECRET`, captures via RpcRecorder, asserts:
  - placeholder bytes appear in the captured RPC stream (`recorder.contains("${secrets.fly.api_token}")`)
  - resolved secret bytes do **not** appear (`!recorder.contains(SYNTHETIC_SECRET)`)
  - no PUBLISH frame on `zombie:{id}:activity` contains the resolved bytes
- `src/zombie/test_production_executor.zig` — fixture mirroring `test_executor_harness.zig` but for the production binary (separate file because the env-var contract differs and we want to avoid muddling the harness fixture with two execution modes).

### Files to modify

- `src/executor/runner.zig` — read a `EXECUTOR_PROVIDER_STUB` env var that swaps the runtime provider for `test_stub_provider` when set. Gate behind a build option (similar to `executor_harness`) so the production binary doesn't carry stub code at all in release builds. Add `executor_provider_stub` build option to `build.zig`.

### Files NOT to modify

- `src/executor/runner_progress.zig` — redaction logic itself is what the test exercises; don't touch it.
- `src/executor/main.zig` — the stub-provider env var read is in `runner.zig`, not `main.zig`, since the gate is per-StartStage not per-process.

## Open questions

1. **Stub provider granularity.** Does the stub return a single tool_use event, or a multi-turn conversation? The redaction test only needs one event; keep it minimal.
2. **Build option vs. env var.** A new `executor_provider_stub` build option produces a third executor binary (`zombied-executor-stub`). Worth the binary count to keep production unaffected, vs. an env var read in `runner.zig` that compiles into both. Decision deferred to PLAN.
3. **Synthetic secret value.** Random-ish bytes that are unlikely to appear elsewhere in the payload (e.g. `SYNTHETIC_SECRET = "ZMBSTUB-redaction-canary-9c8f4e1a2d"`). The `!recorder.contains` check passes only if redaction substitutes correctly; a less-distinctive value would false-positive on coincidental byte sequences in JSON noise.

## Out of scope

- Real LLM provider testing — already covered by other integration tests in the executor suite.
- Tool-execution sandboxing tests — covered by `executor/sandbox_edge_test.zig`, `executor_limits_test.zig`.
- Multi-secret redaction (GitHub token, Slack token, etc.) — once the fixture is in place, additional cases are one-line additions; not needed for invariant coverage.

## Test Specification

| Test | Asserts |
|---|---|
| `test_executor_args_redacted_at_sandbox_boundary` | Stub provider emits tool_use with args referencing `${secrets.fly.api_token}` → recorder captures `${secrets.fly.api_token}` in RPC bytes; `SYNTHETIC_SECRET` never appears. |
| `test_args_redacted_no_secret_leak` | Same scenario, but check pub/sub side: subscriber on `zombie:{id}:activity` collects all frames, none contain `SYNTHETIC_SECRET`. |
| `test_executor_passes_through_redacted_for_chunks` | `agent_response_chunk` frames containing the secret string also redact (regression — chunks reuse the same redactor as tool args). |

## Estimated effort

3-4 hours: ~1.5 h for the stub provider, ~1 h for the production-binary fixture (heavy reuse of `test_executor_harness.zig` pattern), ~30 min per redaction test. One commit per layer.

## Why now / why later

- **Why now:** invariant 6 from M42_001 (`Pub/sub never leaks secrets`) is asserted by code review only. A regression in the redactor (e.g. `runner_progress.Adapter` rewrite, NullClaw observer API change) would land silently.
- **Why later:** M42_001 substrate ships without it; the redactor code already exists and has been sanity-checked manually. The test gates regressions, not initial correctness.
