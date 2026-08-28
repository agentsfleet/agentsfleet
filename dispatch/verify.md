# write — verify.md — verification dispatch (LATENT façade)

This is the prose the AGENT reads **before emitting any message that asserts work
is verified**. Unlike `write_zig` / `write_any`, `verify` has **no deterministic
`.sh` half** — no script can detect the moment an agent is *about to claim done*.
It is a pure **🤔 judgment** dispatch: the agent reads this, runs the canonical
`make` targets, and emits the verdict block. The trigger is a *claim*, not a file
edit. (This is the former Verification gate absorbed into the dispatch model.)

**Signal legend:**

- 🤔 DECIDE — judgment-only; the agent must run the targets below and report
  honestly. No script gates this — it blocks the *turn*, not a commit.
- 🟣 delegated — the `make` targets themselves live in the product repository;
  this pack carries only the discipline of *which* targets are canonical and when.

## Trigger

Fires before any user-facing message asserting verification: *"tests pass",
"ready to merge", "shipping", "ready for review", "CHORE(close) ready"* — or any
equivalent.

**Override:** `VERIFY GATE: <target> skipped per environment constraint (reason: ...)`.
Only when a target is genuinely unrunnable (e.g. Docker missing for integration
tests). Surface the limitation in the done message — never dress a skip as "tests
pass".

## Why `make` is canonical

Package-scoped runners (`bun run test`, `vitest <file>`, `cargo test -p <crate>`)
are **not** verification — they skip every other package's lint and tests and the
cross-language gates. The declared `make` targets are the canonical gates.

## Two cadences, one boundary

"Always" is not a cadence. A Section closing and a milestone closing are
different claims, and running the whole declared set at both spends the
wall-clock of the comprehensive tier to answer a question the scoped lane
already answered.

The gates match those cadences: `orly gate work` runs the declared `conform`
alone (every commit, seconds), `orly gate verify` runs the fast `verify.*` set
(every push), and `orly gate pr` runs the whole-branch criteria and the slow
suites (the close). Each tier runs once per cadence.

**Section lane (per Section, and per done-claim inside one).** `make
harness-verify` — the declared `conform`, seconds — plus the lane that covers
the surface the Section touched (`make test-unit-rustd`, `make test-unit-app`,
…). This proves a Section. It is not a repository claim and never satisfies a
VERIFY row on its own.

**Milestone boundary (once, at CHORE(close), before the PR).** Every command
`.oracle/orly.json` declares, in one pass: `conform`, `verify.lint`,
`verify.unit`, `verify.version`, and the slow tier (`verify.integration`,
`verify.memory`). This is the repository claim, and it is what the done-message
below reports.

Run `orly gate pr` at the close. It does not re-run the fast tier, and it does
not need to: its own `git.pushed` criterion proves HEAD is exactly the commit
the pre-push `orly gate verify` already graded. A bare `orly gate` chains all
three gates — use it on a branch whose earlier gates never ran.

| Target | When |
|---|---|
| `make harness-verify` | Section lane and boundary. The declared `conform` — the deterministic gate audit. |
| `make lint-all` | Boundary. The declared `verify.lint`. |
| `make test-unit-all` | Boundary. The declared `verify.unit` — the cargo workspace plus every package coverage gate. |
| `make check-version` | Boundary. The declared `verify.version`. |
| `make test-integration-rustd` | Boundary. The declared `verify.integration` — live Postgres and Redis; `orly gate pr` skips it on a branch carrying no code. |
| `make wire-fixtures` | The `/v1/runners` wire types changed. Regenerate, then review the diff — a changed fixture IS the wire change. |
| `make bench` (local) | Diff touches request-path code, allocator wiring, or startup/shutdown sequencing. |
| `API_BENCH_URL=https://api-dev.agentsfleet.net/healthz make bench` | After branch deploys to dev. |
| `/orly-write-integration-test` (skill) | With `/orly-write-unit-test` at VERIFY when the diff crosses module boundaries with real I/O; otherwise record `N/A — <reason>`. |
| Acceptance e2e (live tier) | Diff touches a surface the live/acceptance tier covers — relevant suites green, or their opt-in skip matrix recorded. |

The declared set in `.oracle/orly.json` is the source of truth for what exists,
and a lane listed there is a lane that runs at the boundary — including the slow
tier. A package-scoped runner (`cargo test -p afd_wire`, `bun run test` inside a
package) proves that package and never the repository, at either cadence.

## Wire-fixture evidence rule

A diff that regenerates `samples/fixtures/wire-v2/` carries the regenerated
fixtures in the same commit as the type change, and the PR Session Notes say what
moved. A fixture diff with no type change beside it means someone hand-edited
generated output; a type change with no fixture diff means the emitter never ran.

## Coverage discipline

- **Branch coverage is the goal; line coverage is the floor.** One input "covers" a multi-clause condition while leaving its logic untested (`trimmed === "" || === "y" || === "yes"` passes line coverage with a single `"y"`). Feed varied inputs across the equivalence classes — each OR clause independently, success-retry AND fail-retry paths, every early-return guard, empty/casing/whitespace/garbage for normalizers. bun's lcov emits no branch records, so this is test-design discipline, not a number to chase.
- **Do not chase per-file 97% on declaration-heavy files.** bun marks compiler-erased lines (`import type`, `interface`) as 0-hit — no test can execute them, and restructuring to lift the number backfires (inlined type literals get instrumented as 0-hit too). The enforced gates are **aggregates** (`enforce-coverage.mjs` global row; codecov patch across uploaded packages); a few erased lines dilute to noise there. No codecov `ignore` entries either — gate on the aggregate.

## Bench knobs

`make/test-bench.mk` env vars: `API_BENCH_METHOD`, `API_BENCH_DURATION_SEC`,
`API_BENCH_CONCURRENCY`, `API_BENCH_TIMEOUT_MS`, `API_BENCH_MAX_ERROR_RATE`,
`API_BENCH_MAX_P95_MS`, `API_BENCH_MAX_RSS_GROWTH_MB`.

## Required output (done-message)

**Success (boundary):**

```
✅ Verified: 🧪 lint-all ✓ · 🧪 test-unit-all ✓ <N>p/<M>s · 🧩 test-integration-rustd ✓ · 🔆 harness-verify ✓ · 🔆 check-version ✓
```

**Success (Section lane)** — names the lane it ran, never the repository:

```
✅ Section verified: 🔆 harness-verify ✓ · 🧪 test-unit-<surface> ✓ <N>p/<M>s
```

**Failure (any required target failed):**

```
🔴 NOT VERIFIED: <target> ✗ — <one-line reason>
```

**Skipped (environment constraint, not a pass):**

```
🟠 <target> skipped per environment constraint (reason: ...)
```

A skipped target MUST be surfaced — never dressed up as "tests pass".

## PR description results table

Before opening/updating the PR at CHORE(close), the PR/MR description carries a
✅/❌ results table covering the skill chain (`/orly-write-unit-test` ·
`/orly-write-integration-test` or its recorded N/A · `/review`) and the verification
lanes above — **all ✅ (or an explicit recorded N/A/skip) required**. A lane
missing from the table counts as ❌.

## Emoji legend

| Glyph | Meaning |
|---|---|
| ✅ | Verified — all required targets passed |
| 🔴 | Verification failed — at least one target failed |
| 🟠 | Skipped per environment constraint — read the reason |
| 🧪 | Lint / unit / integration test |
| 🧩 | Integration test (cross-process) |
| 🔆 | Informational note (does not affect verdict) |
