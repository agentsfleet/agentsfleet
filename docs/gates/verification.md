# 🚧 Verification Gate

**Family:** Verification & ship-readiness. **Source:** `AGENTS.md` (project-side guard). Tier definitions and `make` target ownership live in this file.

**Triggers** — fires before any user-facing message asserting work is verified: "tests pass", "ready to merge", "shipping", "ready for review", "CHORE(close) ready", or any equivalent.

**Override:** `VERIFY GATE: <target> skipped per environment constraint (reason: ...)`. Only when a target is genuinely unrunnable (e.g. Docker missing for integration tests). Call out the limitation in the done message — not as "tests pass".

## Why `make` is canonical

Package-scoped runners (`bun run test`, `vitest <file>`, `zig build test` without integration tier) are **not** verification — they skip cross-package lint, cross-compile, pg-drain, and integration. `make` targets are the canonical gates.

## Required runs

| Target | When |
|---|---|
| `make lint` | Always. |
| `make test` | Always (tier 1). |
| `make test-integration` | Tier 2: when diff touches HTTP handlers, schema, DB code, Redis code, or any `_integration_test.zig` file. Use `make test-integration-db` / `make test-integration-redis` for focused subsets. |
| `make test-integration` | Tier 3: at least once per branch before declaring ship-ready. From a clean state (e.g. after `make down`) when tier 2 is intermittent — fresh DB proves no state carry-over. |
| `make memleak` | Server lifecycle (`src/http/**`, `src/cmd/serve.zig`), allocator wiring, cross-thread heap ownership. |
| `make bench` (local) | When the diff touches request-path code, allocator wiring, or startup/shutdown sequencing. |
| `API_BENCH_URL=https://api-dev.usezombie.com/healthz make bench` | After branch deploys to dev. |
| Cross-compile `x86_64-linux` + `aarch64-linux` | Whenever `*.zig` touched. |
| `make check-pg-drain` | Whenever `*.zig` touched. |

`make test` is unit-only by definition; never substitutes for tier 2/3.

Tier 2 passing but tier 3 failing means state pollution — fix isolation before shipping.

## Memleak evidence rule

Before CHORE(close) reports green, paste the final `make memleak` result line into the PR Session Notes block OR cite the CI memleak job URL. Branches touching `src/http/**`, `src/cmd/serve.zig`, or allocator wiring MUST include the last 3 lines verbatim. No "I ran it, trust me."

## Bench knobs

`make/test-bench.mk` env vars: `API_BENCH_METHOD`, `API_BENCH_DURATION_SEC`, `API_BENCH_CONCURRENCY`, `API_BENCH_TIMEOUT_MS`, `API_BENCH_MAX_ERROR_RATE`, `API_BENCH_MAX_P95_MS`, `API_BENCH_MAX_RSS_GROWTH_MB`.

## Required output (done-message)

**Success:**

```
✅ Verified: 🧪 lint ✓ · 🧪 test ✓ <N>p/<M>s · 🧩 test-integration ✓ (or N/A — no handler/schema/redis) · 🎯 cross-compile ✓ (zig only)
```

**Failure (any required target failed):**

```
🔴 NOT VERIFIED: <target> ✗ — <one-line reason>
```

**Skipped (environment constraint, not a pass):**

```
⚠️ <target> skipped per environment constraint (reason: ...)
```

A skipped target MUST be surfaced — never dressed up as "tests pass".

## Emoji legend

| Glyph | Meaning |
|---|---|
| ✅ | Verified — all required targets passed |
| 🔴 | Verification failed — at least one target failed |
| ⚠️ | Skipped per environment constraint — read the reason |
| 🧪 | Lint / unit / integration test |
| 🧩 | Integration test (cross-process) |
| 🎯 | Cross-compile target |
| 🔆 | Informational note (does not affect verdict) |
