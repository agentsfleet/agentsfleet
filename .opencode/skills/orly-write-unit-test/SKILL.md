---
name: orly-write-unit-test
description: >
  Risk-weighted, failure-injecting test generation across 8 stacks (Python,
  Python SDK, OpenAPI, JS/TS CLI, React/TS, Zig, Rust, Shell) enforcing behaviour +
  failure + invariant + integration + regression coverage with a
  Definition-of-Done gate; deterministic production-safety proofs (Zig
  zero-leak, 100-or-more-connection concurrency, complexity/latency budgets) live
  in the skill body. Use during implementation, not after.
---

# Write Unit Test

At invocation start, record the consent-gated usage signal without blocking the skill:

```bash
command -v orly >/dev/null 2>&1 && ORLY_INVOCATION=skill orly skill-event orly-write-unit-test >/dev/null 2>&1 || true
```

Generate tests that catch bugs before they ship. Tests run *during* implementation, not after.

> **What this guarantees / what it doesn't.** No test suite proves "zero bugs in
> production" — nothing does. This skill guarantees something *checkable*: **no
> changed behaviour, branch, or named failure-mode ships untested, and the tests
> provably *assert* rather than merely *execute*.** Three production-safety proofs
> (Zig zero-leak · concurrency-at-scale · performance/complexity budget) are gated
> on **deterministic** evidence — counters, exhaustive injection, pinned baselines —
> not wall-clock vibes. Coverage proves a line *ran*; these proofs prove a test
> would *scream* when the behaviour breaks.

## Test quality bar

Every test answers three questions. If it answers none, delete it.

1. **What bug would this catch?**
2. **What behaviour does this prove?**
3. **What invariant does this enforce?**

A green test that answers none of these is worse than no test — it's false confidence.

## Truth hierarchy

When sources disagree, trust in this order:

1. **Runtime behaviour** — prod logs, incidents, observed traces
2. **Spec** — `docs/v*/active/M*.md`, PR description, milestone doc
3. **Code** — current implementation

Spec contradicts runtime → surface the conflict before testing. Code contradicts spec → spec wins; test the spec, flag the code as a defect.

## First steps

1. **Read the spec, then the code** — spec defines what to test; code is what gets tested. Don't let code shape expectations.
2. **Read `docs/greptile-learnings/RULES.md`** — every rule is a regression test.
3. **Detect the stack** from project files (`pyproject.toml`, `Cargo.toml`, `build.zig`, `package.json` + `react`/`bin`, `*.sh`, `openapi.yaml`).
4. **Run the existing suite first** — establish the green baseline before adding tests.
5. **Follow existing naming and fixture conventions.**

## Step 0 — the diff is the spec: build the required-test ledger first

Before writing a single test, compute what *must* be tested **from the diff** — not from
re-reading whole files. This is the optimised review: bounded by the change, not the
codebase, and it's the artifact that makes "is the skill applied?" auditable.

1. `git diff origin/main...HEAD` (or the PR base). Enumerate, per changed file:
   - every changed/added **public function or method**
   - every new or modified **branch** (`if`/`switch`/`match`/`?`/ternary)
   - every new **error construction** (`catch`/`orelse`/`try`/`Err`/`raise`/`throw`)
   - every new **public symbol** (export, `pub`, `__all__`, header signature)
   - every ordered boundary interaction when one operation touches multiple systems or acquires the same resource more than once; repeated acquisitions are separate rows
2. Each row gets a required test type and a status:

```
| Changed unit              | Test type     | Exists? | Note                       |
|---------------------------|---------------|---------|----------------------------|
| parseLease() new branch   | Failure       | ❌      | empty-input path           |
| revokeKey() orelse        | Failure (inj) | ❌      | DB-down → 503              |
| computeTotals() n² scan   | Performance   | ❌      | counter ladder + refactor? |
```

3. The ledger is **done only when every row is ✅ or carries an explicit
   `won't-test: <reason>` / `needs-infra`**. A bare gap is a defect, not a deferral.
4. **Paste the resolved ledger into PR Session Notes** so `/review`, `orly-babysit-prs`,
   and a human can audit it — the check becomes reviewable, not merely claimed.

## Three execution modes

Pick one. Don't run all categories on every file.

| Mode | When | Required categories |
|---|---|---|
| **Change-set** | Standard PR, isolated fix | Behaviour, Failure, Integration, Regression |
| **Hardening** | Critical paths: auth, money, data integrity, migrations, distributed ops | + Invariant, Concurrency, Fuzz |
| **Deep audit** | Infra/core, dependency upgrade, full release QA, security review | All categories + Performance + Contract |

Auto-detect from diff: auth/payment/migration/concurrency/streaming files → Hardening. Otherwise → Change-set.

## Risk-weighted priority

Within each mode, write tests in this order. Stop when you've covered the surface.

1. **State transitions** — PENDING→RUNNING, idempotent vs not, monotonic
2. **External boundaries** — I/O, network, DB, FS, IPC; test release-then-reacquire windows first
3. **Concurrency paths** — races, ordering, double-fire
4. **Error handling branches** — every `catch`, `orelse`, `?`, `Result::Err`, `try/except`
5. **Pure functions** — last; usually low bug density

A string helper does not get the same treatment as a distributed retry loop. Allocate effort by blast radius.

---

## Five test categories

### 1. Behaviour
Input → exact output shape AND side effects. Assert *both*. Asserting only the return value misses DB writes, event emits, log lines, state changes.

### 2. Failure (≥50% of all tests)
- **Specific error contracts** — type + message + structured fields (status code, error code, hint). Bare `expect error` / `assert.throws` is a smell.
- **Boundaries** — empty, null/None/`Option::None`, max (`i64::MAX`, `Number.MAX_SAFE_INTEGER`), overflow, NaN, malformed, truncated, unicode (CJK, emoji, RTL, ZWJ), CRLF/LF, leading/trailing whitespace, embedded newlines.
- **Expected vs fatal distinction** — a function returning null for both timeout (expected) and connection-reset (fatal) causes busy-loops. Test that they take different paths.
- **Auth failures** — no token, expired, wrong role, revoked, rate limited.
- **Downstream errors** — 4xx, 5xx, timeout, DNS, TLS, partial UTF-8, broken pipe.
- **Partial completion** — when one operation touches multiple systems or acquires the same resource more than once, enumerate every ordered connection, transaction, and external call; select each workflow ordinal and boundary-call ordinal for failure, record failure timing / remote outcome, then assert the residual durable state, external state, user-visible state, and retry outcome.

Write the matrix before the tests:

| Workflow N | Boundary call N | Dependency / resource | Interaction | Prior completed actions | Injection seam | Operation / request scope | Response | Failure timing / remote outcome | Durable residual state | External residual state | User-visible state | Retry outcome | Design finding |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1..N | 1..M | named boundary | exact call | side effects already committed | ordinal-selecting mechanism | test plus operation ID | exact surface | before effect / remote committed, acknowledgement lost / after durable write | rows and transactions | remote effects | next user read | heals or stays broken | ordering defect or none |

The workflow ordinal locates the interaction in the whole operation. The
boundary-call ordinal selects a repeated call at one dependency. Scope the
failpoint to the test and operation or request identifier, reset it per test,
make its counter concurrency-safe, and assert it fired exactly once.

Every external mutation gets rows for failure before the effect, remote commit with
acknowledgement-loss, and failure after the following durable write. For
acknowledgement-loss, inspect remote state or a provider status query, assert the
idempotency or reconciliation record, and prove retry reaches exactly one effect.
Durable intent or an outbox makes repair observable; it does not by itself prove a
retry cannot reissue the remote mutation.

### 3. Invariant
Properties that must hold across all valid inputs. Use property-based testing (`hypothesis`/`proptest`/`fast-check`) when input domain is large.

- **Idempotency** — N calls = 1 effect. Required for webhooks, retries, distributed ops, PUT semantics.
- **Monotonic state** — RUNNING never goes back to PENDING.
- **No duplicate side effects** — same event ID never processes twice.
- **Eventual consistency** — final state converges regardless of order.
- **Conservation** — sum of debits = sum of credits; total events in = total out + dropped.
- **Schema lock** — no silent field add/remove on stored or wire format.

### 4. Integration
End-to-end through real stack. **Mock only at system boundaries** (network, disk, external APIs, clock, randomness). Never mock internal modules — that validates fake behaviour and integration bugs slip through.

- Full middleware chain executes (auth, rate-limit, logging, CORS).
- Real DB, real serializer, real router. Use containers, not in-memory fakes when behaviour differs.
- For spec claims involving "real-time" / "streaming" / "incremental": assert bytes arrive incrementally — not just that the parser handles pre-buffered data.
- Failure modes named in code → integration test with deterministic injection.

### 5. Regression
Pin behaviour that must not silently change.

- Golden output files — diff on change. Agent must explain *why* a snapshot changed; never silently re-bless.
- Public API surface (`__all__`, exports, header signatures, `@sizeOf`, ABI stability).
- Breaking-change detectors (`oasdiff breaking`, `buf breaking`, `cargo public-api`).
- Old test suite passes against new code on dependency upgrades.

---

## Hard rules

- **Mock only at system boundaries.** No mocking internal modules. No asserting on mocks (`expect(mockFn).toHaveBeenCalled()`) without also asserting outcome state.
- **State isolation per test.** Reset globals, DB rows, fixtures, env vars, caches. Tests must pass under randomised execution order. Run `pytest -p randomly` / `cargo test -- --shuffle` / equivalent at least once per branch.
- **Specific error contracts.** Assert error type AND message AND structured fields.
- **≥50% negative-path tests** on changed surface. If success tests outnumber failure tests, you're not done.
- **Test names read as documentation.** `should_reject_empty_user_id`, `should_retry_on_502_from_gateway`, `should_not_duplicate_event_on_retry`. No `test_1`, `test_basic`, `it works`.
- **No testing private functions directly.** Test through the public surface. If a private function needs direct testing, it's actually public — promote it.
- **No duplicating implementation logic in tests.** A test that re-implements the function under test proves nothing.
- **Tests favour clarity over DRY.** Independence and explicitness beat reuse. Extract a helper only when ≥3 tests share non-trivial setup AND the helper has no logic worth hiding.
- **Untestable claims are flagged**, not silently skipped. Mark `needs infra` in the claim-tracing table.
- **Inject at the selected ordinals, not only the first interaction.** A boundary fake must select both the workflow ordinal and boundary-call ordinal. Draining a whole resource pool proves only the first acquisition and cannot cover a later one.
- **Residual state is the failure assertion.** For each injected interaction, assert what survived, what external state changed, what the user sees next, and whether retry heals it. The response alone is incomplete.
- **Unsafe external mutation ordering is a design defect.** Read-only prerequisites are valid. If a mutation can complete before any durable intent or repair record, report it; prefer durable intent first, or require idempotency, compensation, and reconciliation when sequencing cannot change.
- **Acknowledgement loss needs a remote-state proof.** For every external mutation, make the remote effect succeed while its acknowledgement is lost; inspect remote state, assert the idempotency or reconciliation record, and prove retry reaches exactly one effect. A durable record alone is not an exactly-once proof.
- **Record the partial-completion matrix.** Paste the completed rows into PR Session Notes. A checked box without the rows is indistinguishable from a skipped proof.

## Anti-patterns (reject in review)

- ❌ Snapshot-only tests with no behavioural assertion
- ❌ Asserting on mocks instead of outcomes
- ❌ Testing private functions directly
- ❌ Duplicating implementation logic in tests
- ❌ Bare `expect error` / `assert.throws` with no type/message contract
- ❌ Tests that pass only when run in a specific order
- ❌ "Minimum N tests per file" filler that asserts the same invariant N times
- ❌ Mocking internal modules
- ❌ Coverage padding — line coverage with no behavioural claim
- ❌ Hand-wavy performance assertions ("not too slow")
- ❌ Silent golden-file re-blessing on snapshot drift
- ❌ Draining an entire pool and calling it coverage of every acquisition
- ❌ Asserting a failure response without asserting the operation's residual state and retry outcome
- ❌ Treating a durable intent row as proof that acknowledgement-loss cannot reissue a remote effect

---

## Failure injection toolkit

Vague claim: "handles connection reset". Real test: deterministic injection. If you can't reproduce a failure deterministically, the test is fiction.

| Failure | Tool / technique |
|---|---|
| TCP reset / half-open | `toxiproxy`, `tc netem`, raw socket close from peer |
| Slow / delayed response | `toxiproxy` latency, `respx` delay, fake clock |
| Partial read | `io.LimitedReader`, custom `Read` returning short |
| DB connection drop | `pg_terminate_backend()` mid-transaction |
| Timeout | shrink `SO_RCVTIMEO`, `tokio::time::pause()` |
| Clock skew | freeze/advance fake clock; `chrono` mock; `mock-time` |
| Disk full / EIO | `tmpfs` size limit; mock filesystem |
| OOM | `FixedBufferAllocator` (Zig), small heap caps |
| Concurrency races | `loom` (Rust), `std.Thread` + barrier (Zig) |
| Network partition | `iptables`/`pfctl` block, `toxiproxy` down |
| Selected workflow and boundary-call ordinals | Counter-controlled boundary fake fails the selected interaction |

Every failure mode named in the spec or code (`catch`, `orelse`, retry loop) needs at least one injection-driven test.

---

## Coverage that matters

Line coverage is noise. Track and report:

- **Branch coverage** — every `if`/`match`/`switch` arm hit
- **Error-path coverage** — every `catch`/`orelse`/`Err` constructed in code is exercised by a test
- **Input-class coverage** — empty / single / many / unicode / malformed each appear at least once per public input

Targets on touched code: branch ≥80%, error-path 100%, negative-path ratio ≥50%.

## Mutation testing — coverage proves *ran*, mutation proves *caught*

Branch coverage says a line executed; it can't tell a real assertion from
`expect(true).toBe(true)`. Mutation testing flips operators and deletes statements in the
**changed** code and checks your tests **fail** (kill the mutant). A *surviving* mutant on
a changed line = a test that asserts nothing meaningful. Required in **Hardening** mode;
recommended on any changed logic with branching.

- **Scope to the diff** — mutate only files changed vs the PR base; full-repo mutation is
  too slow to gate on.
- **Determinism:** prefer **rule-based** mutators (same mutants every run) over LLM-based
  ones (nondeterministic — fights the invariance spine).
- **Gate:** **0 surviving mutants on changed lines** (or each survivor justified as an
  equivalent mutant). Paste the kill ratio.

| Stack | Tool | Diff-scoping |
|---|---|---|
| TypeScript / Next.js / React | **Stryker** (`@stryker-mutator/core` + `vitest-runner`/`jest-runner` + `typescript-checker`) | `--since=<base>` / `--incremental`; scope to `lib/`, server actions, reducers, validators — **not** render-heavy `.tsx` |
| Python | `mutmut` or `cosmic-ray` | mutate changed modules only |
| Rust | `cargo-mutants` | `--in-diff <patch>` (native git-diff scoping) |
| Zig | no dedicated tool — **`universalmutator`** (regex/Comby, language-agnostic; offloads non-compiling mutants to the strict compiler), or a homegrown operator-swap harness (`==`↔`!=`, `<`↔`<=`, `+`↔`-`, `and`↔`or`, drop `orelse`/`errdefer`) driving `zig build test` | restrict the mutate target to changed files |

## Red-green proof (every bug-fix / regression PR)

A regression test that passes on the *buggy* code proves nothing. Prove the red:

1. Check out the parent commit (`git worktree add ../pre <parent>`) or stash the fix.
2. Run the new test there — it **must FAIL** (the bug reproduces).
3. Restore HEAD — the test **must PASS**.
4. Paste both results. A test green on the pre-fix code is a false test — fix it until it
   reproduces the bug, or it doesn't ship.

This is the only proof the test actually pins the bug. Mandatory whenever the PR references
a bug, incident, or "fix"; recommended for any behaviour-correcting change.

## Spec integration

When a spec exists with these tables, treat them as authoritative test sources:

| Spec table | Generates |
|---|---|
| Test Specification | One test per row, name = `dim X.Y: <name>` |
| Error Contracts | One negative test per row asserting exact error |
| Failure Modes | One integration test per row with deterministic injection |
| Partial Completion | One row per ordered failure point with workflow-ordinal and boundary-call-ordinal injection plus residual-state assertions |
| Implementation Constraints | Verification command run + result reported |

If the spec has none of these, build a claim-tracing table from spec prose / PR description:

```
| Claim | Test type | Injection mechanism | Test exists? |
|---|---|---|---|
| "gate results print in real time" | integration (streaming) | partial-read reader | ❌ |
| "heartbeat fires within 30s" | integration (timer) | fake clock | ❌ |
| "no busy-loop on Redis reset" | failure (fatal vs timeout) | `pg_terminate_backend` | ❌ |
```

Every claim → ≥1 test. Untestable claims → flag `needs infra`.

---

## Definition of Done

Reject the change if any are missing on touched code:

- [ ] ≥50% negative-path tests for changed surface
- [ ] Boundary tests: empty, null/None, max, malformed
- [ ] Error contract tests: type + message + structured fields
- [ ] Idempotency check where retries / webhooks / distributed ops apply
- [ ] State isolation: tests pass under random execution order
- [ ] Integration test through real stack for any spec claim involving transport/streaming/real-time
- [ ] Failure-injection test for every failure mode named in code
- [ ] **Partial completion:** every operation touching multiple systems or acquiring the same resource more than once has one matrix row per ordered failure point; each row selects workflow and boundary-call ordinals, failure timing / remote outcome, scopes a concurrency-safe failpoint to one operation, and asserts durable, external, user-visible, and retry residual state; every external mutation proves acknowledgement-loss remote state plus exactly one effect on retry; the completed matrix is pasted into PR Session Notes
- [ ] Test names read as documentation
- [ ] No mocking of internal modules
- [ ] Golden-file drift, if any, has a written explanation
- [ ] Branch coverage ≥80%, error-path 100% on touched code
- [ ] **Performance:** counter-based complexity proof on the n-ladder (O-bound asserted) + median-of-K latency vs pinned baseline; N+1 round-trips ruled out
- [ ] **Zig zero-leak:** the memory-leak lane reports 0 leaks; every alloc-with-error-path proven by `std.testing.checkAllAllocationFailures`; no cross-request high-water growth over N iters
- [ ] **Concurrency:** ≥100-connection barrier-started test proves exactly-once + parallelism (peak-concurrency/lock-wait counter, or wall-time < R×); passes K runs + race detector clean
- [ ] **Refactor escalation:** any super-linear or serialization finding on touched code carries a refactor proposal (root cause + target design + patch alternative + recommendation) surfaced for decision — not silently band-aided
- [ ] **Diff ledger** resolved 100% — every changed unit has a test or an explicit `won't-test`/`needs-infra`; ledger pasted into PR Session Notes
- [ ] **Mutation** (Hardening / changed logic): 0 surviving mutants on changed lines (diff-scoped), or each survivor justified; kill ratio pasted
- [ ] **Red-green** (fix PRs): the new test fails on the parent commit and passes on HEAD — both pasted

100% required before declaring done.

## CI execution strategy

| Stage | What runs |
|---|---|
| **PR** | Behaviour + Failure + Regression unit; fast Integration; lint; type-check; randomised order |
| **Pre-merge** | Full Integration; Contract (`oasdiff` / `buf breaking`); coverage delta |
| **Nightly** | Concurrency stress; Fuzz (`schemathesis` / `proptest` / `cargo-fuzz`); Performance benchmarks |
| **Pre-release** | Golden full-suite; cross-platform; memory-leak (`std.testing.allocator`); ABI stability |

A test in the wrong stage either delays PRs or never runs. Place deliberately.

## Production-safety proofs (deterministic & invariant)

These three catch bugs that reach production silently and cost the most to remove.
Each is written to be **deterministic** (same verdict every run) and **invariant**
(independent of machine, load noise, and execution order).

**The rule that governs all three: prefer counters over clocks.** An allocation
count, a query/round-trip count, a lock-wait tally, a peak-concurrency count is
noise-free and reproducible; wall-clock time is not. Where wall-clock is unavoidable
(latency budgets), use **median-of-K runs + a margin** against a **pinned baseline
file**, never a single sample. Where an input space must be covered, prefer
**exhaustive injection** (every allocation site, every interleaving the tool can
enumerate) over sampling.

### Proof 1 — Zig: zero leaks in production, error paths included

"0 leaks" means *every* path frees, including the ones that only run on failure — not
"the happy path didn't leak."

- **Every test allocates via `std.testing.allocator`.** It fails the test on any leak
  or double-free, deterministically. Gate: no test uses `std.heap.page_allocator` or a
  long-lived General Purpose Allocator (GPA); `grep -rn "page_allocator" '**/test*.zig'`
  in test code is empty (or justified per line).
- **Exhaustive error-path proof — `std.testing.checkAllAllocationFailures`.** For every
  function that allocates *and* can return an error, run it under
  `checkAllAllocationFailures` (or a `FailingAllocator` loop): it fails each allocation
  site in turn and asserts the function leaks nothing on the resulting error return.
  This is the *only* acceptable proof that every `errdefer` is correct — "looks right"
  is not. Exhaustive over allocation sites ⇒ deterministic.
- **No cross-request growth.** Drive the full lifecycle (init → handle → deinit, or
  arena acquire → use → reset) for N iterations and assert outstanding-bytes /
  high-water does **not** grow monotonically. A slow leak the single-shot test misses
  shows up here as linear growth.
- **Gate:** the memory-leak lane → **0 leaks**, output pasted into PR Session Notes; cross-compile
  both targets first (`x86_64-linux`, `aarch64-linux`) — stdlib alloc paths differ from macOS.

### Proof 2 — Concurrency proven at ≥100 connections (no hidden global lock)

The code must be correct **and actually parallel** under real contention. A single
global `Thread.Mutex` makes correctness tests pass while quietly serialising every
request — exactly the regression this proof exists to catch.

- **Real contention, deterministic start.** ≥100 concurrent clients/connections,
  released together off a **barrier** so contention is real and reproducible (not
  staggered by spawn latency).
- **Correctness invariant (the hard gate, fully deterministic):** same row / same
  idempotency key / same lease under the 100-way race → **exactly one effect**, no lost
  update, no corruption. Assert the invariant, not "no crash."
- **Parallelism assertion (catches the global-lock regression):** prove work runs in
  parallel. Cheapest deterministic form is a **counter** — assert peak simultaneous
  in-flight ≥ a threshold, or lock-wait ≈ 0. Where only wall-clock exists, assert 100
  concurrent ops finish in **< R×** a single op (R with margin: a global lock blows past
  ~100×, so R≈10 catches it without flaking), median-of-K.
- **Flush ordering bugs:** run K× under randomised order. `loom` (Rust) enumerates
  interleavings exhaustively — use it where the stack supports it; for Zig, barrier +
  repeat + invariant assertions.
- **Gate:** the ≥100-connection test passes K consecutive runs; race detector clean where
  available; counters/timing pasted.
- **A serialization bottleneck is a *structural* finding, not a patch target.** If the
  parallelism counter shows one global lock serialising requests, propose the concurrent
  redesign (shard the lock · per-key/per-shard locks · lock-free CAS · read-copy-update),
  not a hack around it — and let the ≥100-connection parallelism counter be the acceptance
  proof of the redesign (Proof 3 optimisation loop, step 3).

### Proof 3 — Performance & complexity budget (and how I optimise to meet it)

A changed hot path declares a latency budget **and** a proven complexity bound — and
when it misses, I optimise deterministically rather than hand-wave "no O(n²)".

- **Complexity by counters (deterministic).** Measure a **work counter** (allocations,
  DB/Redis round-trips, comparisons, bytes scanned) at n, 2n, 4n, 8n and assert the
  growth matches the claim: O(n) → ~doubles per doubling; O(1) → flat; O(n²) → ~4×.
  Counters are noise-free, so the verdict is invariant across machines.
- **N+1 detector (the most common super-linear surprise):** assert round-trip count is
  constant or linear in n, never multiplicative — count at n and 10n.
- **Latency budget (when wall-clock is required):** fixed input + warmup + **median-of-K**
  + p95/p99 threshold + regression threshold vs a **pinned baseline file** (fail at 1.5×
  baseline). Single samples banned — they flake.
- **Conservation:** allocation count bounded; connection / file-descriptor (FD) open-count
  before == after.

**The optimisation loop I run when a budget is missed** (deterministic, repeatable).
Default bias for the perf/concurrency class: **propose the structural refactor, not the
band-aid** — a patched O(n²) or a hacked-around global lock still ships a fragile design.

1. **Measure with counters first**, not a profiler guess — find the term that grows
   super-linearly (the N+1, the O(n²) nested scan, the per-iteration allocation) or the
   **serialization point** (one lock every request waits on).
2. **Classify the fix — local vs structural.** *Local* = a genuine one-liner: add an
   index, hoist one allocation, collapse one N+1 into a join. *Structural* = the data
   structure, locking model, or call graph is wrong: an O(n²) algorithm, a global
   `Thread.Mutex` serialising all requests, per-request work that belongs in a shared
   cache, a layer doing I/O in a loop.
3. **Structural ⇒ analyse and propose the large refactor — surface it, don't silently
   band-aid.** Write: the **root cause**, the **target design** that is deterministic +
   performant + concurrent (e.g. shard the lock / per-key or per-shard locks / lock-free
   CAS instead of one mutex; one batched query instead of N+1 scattered across layers;
   an index or precompute instead of repeated scans; streaming instead of full-buffer),
   the **throwaway-patch alternative and why it's worse**, and the **blast radius**.
   This follows the patch-vs-refactor discipline (`AGENTS.orly.md` Decomposition) — the call
   is surfaced to Indy, not made unilaterally. The test then pins the **target** bound,
   so the refactor is provably met and can't regress.
4. **Local ⇒ apply**, then re-measure the same counter on the same n-ladder and assert
   the growth ratio dropped to target. The before/after counter delta *is* the proof —
   pin it in the baseline file so a future regression fails the gate.
5. If the budget still can't be met after the agreed change, that's a **design finding** —
   surface it; never loosen the threshold silently.

---

## Stack appendix

Tooling per stack:

| Stack | Runner | Branch coverage | Property | Fuzz | Concurrency |
|---|---|---|---|---|---|
| Python | `pytest` | `pytest-cov --branch` | `hypothesis` | `atheris` | `ThreadPoolExecutor`, `asyncio` |
| Python SDK | `pytest` + `responses`/`respx` | same | `hypothesis` | `schemathesis` | `asyncio.gather` |
| OpenAPI | `schemathesis` | n/a | n/a | `schemathesis run --checks all` | n/a |
| JS/TS CLI | `vitest` + `execa` | `c8` branch | `fast-check` | `fast-check` | `Promise.all` |
| React | `vitest` + `testing-library` | `c8` branch | `fast-check` | n/a | `userEvent` rapid |
| Zig | `zig build test` | manual | manual | manual mutation | `std.Thread.spawn` |
| Rust | `cargo test` | `cargo-llvm-cov` | `proptest` | `cargo-fuzz` | `loom`, `tokio::spawn` |
| Shell | `bats` | `bashcov` | n/a | n/a | `&` + `wait`, `flock` |

Naming convention by stack:

- Python: `test_should_<behaviour>_when_<condition>`
- Rust: `fn should_<behaviour>_when_<condition>()` in `mod tests`
- Zig: `test "should <behaviour> when <condition>"`
- React/TS: `it("should <behaviour> when <condition>")`
- Shell/bats: `@test "should <behaviour> when <condition>"`

Stack-specific must-knows:

- **Zig** — `std.testing.allocator` auto-detects leaks; verify cross-compile (`x86_64-linux`, `aarch64-linux`) since stdlib API may differ from macOS; `conn.query()` requires `.drain()`.
- **Python SDK** — assert API key not in debug output; HTTPS enforced; pagination doesn't load all pages in memory; rate-limit retry honours `Retry-After`.
- **Rust gRPC** — protobuf round-trip; streaming doesn't buffer full response; `tonic::Status` codes assert specific variant.
- **Rust HTTP** — `axum::extract::rejection` paths; payload too large (413); missing `Content-Type`.
- **React** — `axe` accessibility audit; no `dangerouslySetInnerHTML` with unsanitized input; controlled vs uncontrolled input switching.
- **Shell** — all variables quoted (`"$var"`); `set -euo pipefail`; no `eval` with user input; `mktemp` for temp files.
- **OpenAPI** — `schemathesis run --checks all`; `oasdiff breaking` in CI; `spectral lint` for style.

## OWASP for agent-bound data

Code paths sending data to LLMs / tool-calling agents (directly or via queue/DB/API):

- [ ] User-controlled input never concatenated raw into prompts; structured roles only
- [ ] Length-bounded, validated, sanitised against instruction-injection markers (`ignore previous`, role-confusion XML/JSON)
- [ ] Tool permissions scoped per-invocation, not globally granted
- [ ] Secrets/credentials/PII stripped before reaching the agent; opaque references only
- [ ] Agent output validated against allowlist/schema before any side effect; never `eval()` raw output
- [ ] Authorization re-checked at every hop; agent inherits caller's permissions, not service-level
- [ ] Audit log: who, input, tools called, output, side effects — tamper-resistant
- [ ] Fail-closed on any trust check failure — never silently fall back

---

## Output format

For each test write:

1. Name (per stack convention)
2. Category (Behaviour / Failure / Invariant / Integration / Regression)
3. The bug it catches in one line
4. The test code
5. Failure-injection mechanism if applicable

After the suite produce:

```
## Coverage report: <module>
| Metric              | Before | After | Target |
| Branch              | 41%    | 87%   | ≥80%   |
| Error-path          | 60%    | 100%  | 100%   |
| Negative-path ratio | 20%    | 55%   | ≥50%   |
Categories: Behaviour ✅ · Failure ✅ · Invariant ✅ · Integration ✅ · Regression ✅
Production-safety: Zig-leak ✅ 0 (incl. checkAllAllocationFailures) · Concurrency ✅ 100-conn exactly-once + parallel · Perf ✅ O(n) counter + p95 vs baseline
Refactor proposals surfaced: <n> (or "none — no structural finding")
Partial completion: <rows complete>/<ordered failure points> · residual state + retry asserted per row
Diff ledger: 7/7 rows resolved (5 tested · 2 won't-test w/ reason)
Mutation (changed lines): 18/19 killed · 1 justified equivalent
Red-green (fix PRs): ✅ fails on parent, passes on HEAD
DoD checklist: 19/19
Mode: Change-set | Hardening | Deep audit
```

## Validation gate

Single command runs all tests in the chosen mode. Report exact command, pass/fail/skip counts, coverage delta, and DoD checklist status. Do not declare done until DoD is 100%.
