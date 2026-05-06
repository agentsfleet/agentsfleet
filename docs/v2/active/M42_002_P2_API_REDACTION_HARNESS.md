# M42_002: Production-Binary Test Harness — Wire-Level Redaction

**Prototype:** v2.0.0
**Milestone:** M42
**Workstream:** 002
**Date:** Apr 28, 2026: 09:35 PM
**Status:** IN_PROGRESS
**Branch:** feat/m42-002-redaction-harness
**Priority:** P2 — invariant is currently protected by code review; mechanical regression gate is desirable but not launch-blocking.
**Categories:** API
**Batch:** —
**Depends on:** M42_001 (streaming substrate — landed; recorder fixture + comptime executor harness already in place).

## Why this is its own workstream

M42_001 ships two test rows the spec asks for that the comptime-gated harness cannot cleanly exercise:

1. `test_executor_args_redacted_at_sandbox_boundary`
2. `test_args_redacted_no_secret_leak`

A third row from M42_001 — `test_pubsub_failure_does_not_block` (PUBLISH-only failure with XADD/XACK still working, realistically driven by a Redis Access Control List (ACL) user without `+publish`) — is **split out to M42_003**. Different fixture, different failure mode (transport-level not redactor-level), different teardown. Bundling it here muddies the slice.

The redaction logic lives in `src/executor/runner_progress.Adapter` — the NullClaw observer/stream-callback adapter that intercepts tool-use and response-chunk events, scans the `args` payload for resolved secret bytes, and substitutes the placeholder before encoding the frame. The harness binary built with `build_options.executor_harness = true` **comptime-strips that path entirely** because `runner.execute` short-circuits to `runner_harness.execute` before any of NullClaw's observer pipeline runs.

A test that asserts redaction therefore needs:

- The production `zombied-executor` binary (NullClaw + tool builders + adapter all included).
- A stub or recorded LLM provider that emits a deterministic tool_use event (otherwise the test spends real tokens, costs money, and is non-deterministic).
- A real tool spec containing `${secrets.llm.api_key}` so the substitution path fires.
- The RpcRecorder fixture (already in tree at `src/zombie/test_rpc_recorder.zig`) for the byte-level assertion.

## Scope

### Files to add

- `src/executor/test_stub_provider.zig` — a NullClaw `Provider` implementation that returns a canned response containing one tool_use event whose args reference `${secrets.llm.api_key}`. ~150 lines.
- `src/zombie/event_loop_harness_redaction_test.zig` — drives the stub-provider binary spawn (parallel to `test_executor_harness.zig` but pointed at `zig-out/bin/zombied-executor-stub`), sends a real StartStage with `agent_config.api_key = SYNTHETIC_SECRET`, captures via RpcRecorder, asserts:
  - placeholder bytes appear in the captured RPC stream (`recorder.contains("${secrets.llm.api_key}")`)
  - resolved secret bytes do **not** appear (`!recorder.contains(SYNTHETIC_SECRET)`)
  - no PUBLISH frame on `zombie:{id}:activity` contains the resolved bytes
### Files to modify

- `build.zig` — add `executor_provider_stub` build option (default `false`); produce a third executor binary `zombied-executor-stub` when set. Production `zombied-executor` and harness `zombied-executor-harness` builds remain unaffected. `zig build test` depends on the stub install step.
- `src/executor/runner.zig` — at comptime, when `build_options.executor_provider_stub` is true, swap the provider for `test_stub_provider`. No env-var read; the binary identity is the gate. When the flag is false the stub module is not imported at all (mirrors the existing `executor_harness` pattern).
- `src/zombie/test_executor_harness.zig` — parameterise over a `BinaryTarget` enum (`.harness` default, `.stub`) rather than introducing a parallel fixture file. Same spawn skeleton, same socket polling, same teardown — only the default binary path, env-var override name, and presence-of-script-file branch differ. Avoids ~150 LOC of near-duplicate spawn plumbing the previous draft would have produced.

### Files NOT to modify

- `src/executor/runner_progress.zig` — redaction logic itself is what the test exercises; don't touch it.
- `src/executor/main.zig` — provider selection lives in `runner.zig` for parity with the harness gate.

## Resolved decisions

1. **Stub provider granularity** → single `tool_use` event. Multi-turn is YAGNI for this invariant; additional cases are one-line additions later.
2. **Build option vs. env var** → **build option**, third binary `zombied-executor-stub`. Compile-time strip is the only way to be confident the production binary carries no stub bytes; matches the existing `executor_harness` pattern reviewers already trust. Cost is one extra binary in `zig-out/bin/`, paid only when the stub build option is set.
3. **Synthetic secret value** → `SYNTHETIC_SECRET = "ZMBSTUB-redaction-canary-9c8f4e1a2d"`. Distinctive enough that `!recorder.contains` does not false-positive on coincidental byte sequences in JSON noise.

## Out of scope

- Real LLM provider testing — already covered by other integration tests in the executor suite.
- Tool-execution sandboxing tests — covered by `executor/sandbox_edge_test.zig`, `executor_limits_test.zig`.
- Multi-secret redaction (GitHub token, Slack token, etc.) — once the fixture is in place, additional cases are one-line additions; not needed for invariant coverage.
- Pub/sub-failure non-blocking behaviour — split out to M42_003 (different fixture: Redis ACL user without `+publish`; different assertion: XADD/XACK still progress).

## Test Specification

| Test | Asserts |
|---|---|
| `test_executor_args_redacted_at_sandbox_boundary` | Stub provider emits tool_use with args referencing `${secrets.llm.api_key}` → recorder captures `${secrets.llm.api_key}` in RPC bytes; `SYNTHETIC_SECRET` never appears. |
| `test_args_redacted_no_secret_leak` | Same scenario, but check pub/sub side: subscriber on `zombie:{id}:activity` collects all frames, none contain `SYNTHETIC_SECRET`. |
| `test_executor_passes_through_redacted_for_chunks` | `agent_response_chunk` frames containing the secret string also redact (regression — chunks reuse the same redactor as tool args). |

## Estimated effort

3-4 hours: ~1.5 h for the stub provider, ~1 h for the production-binary fixture (heavy reuse of `test_executor_harness.zig` pattern), ~30 min per redaction test. One commit per layer.

## Why now / why later

- **Why now:** invariant 6 from M42_001 (`Pub/sub never leaks secrets`) is asserted by code review only. A regression in the redactor (e.g. `runner_progress.Adapter` rewrite, NullClaw observer API change) would land silently.
- **Why later:** M42_001 substrate ships without it; the redactor code already exists and has been sanity-checked manually. The test gates regressions, not initial correctness.
