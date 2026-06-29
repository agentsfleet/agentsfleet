# M102_004: Amortize test-integration harness setup to cut suite wall-clock

**Prototype:** v2.0.0
**Milestone:** M102
**Workstream:** 004
**Date:** Jun 29, 2026
**Status:** PENDING
**Priority:** P2 — developer/CI velocity (the lane brushes its timeout); no user-facing surface
**Categories:** API
**Batch:** B1 — standalone; no concurrent workstream
**Branch:** {feat/m102-004-name — added when work begins}
**Depends on:** M102_002 (the CI lane that runs this suite; its container/overlap work already landed on PR #461 — this attacks the suite *run*, which that lane left untouched)
**Provenance:** LLM-drafted (Claude Opus 4.8, Jun 29 2026), from a per-test timing profile captured this session

> **Provenance is load-bearing.** This spec is LLM-drafted from a measured profile — cross-check the cited numbers against a fresh timed run before trusting any single figure; the *shape* (uniform per-test setup floor) is the robust finding, the exact ms are arm64-local and indicative.

**Canonical architecture:** none — test-infra has no `docs/architecture/` doc. The shape to mirror/refactor is `src/agentsfleetd/http/test_harness.zig` (the existing per-test harness). The shared-harness pattern is greenfield in this repo (no `beforeAll`/shared-singleton precedent in tests).

---

## Implementing agent — read these first

1. `src/agentsfleetd/http/test_harness.zig` — the harness every integration test builds per test (`init` → `openHandlerTestConn` + `bringUpServer` + `connectRedis`). This is the object whose setup we amortize.
2. `src/agentsfleetd/http/handlers/common_authz.zig` (`openHandlerTestConn`) — opens a fresh `pg.Pool` (size 4, eager-connect) per test; §2 shrinks this.
3. `src/agentsfleetd/db/pool.zig` — `POOL_SIZE_DEFAULT = 256/64 = 4` and `pg.Pool.init` eager-connects `size` conns; the multiplier behind the floor.
4. Vendored `zig-pkg/pg-*/test_runner.zig` — shows the Zig 0.16 custom-runner API **only as reference**; this spec uses the **default** runner (no `beforeAll` hook), so sharing must be a lazily-initialized file-scoped singleton, not a runner callback.

Greenfield: the shared-harness lifecycle (lazy init, between-test isolation, leak-clean teardown) has no in-repo precedent — design it here.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Amortize integration-test harness setup to cut suite wall-clock
- **Intent (one sentence):** Make `make test-integration` finish materially faster by paying the httpz-server + DB-pool + Redis-connect setup **once per test file** instead of once per test, without weakening any test's isolation.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; a mismatch with the Intent → STOP and reconcile before editing.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — a developer (or CI) runs `make test-integration`; the suite that took ~7.5 min on CI now finishes well under that, and a re-run produces identical pass/fail with no flakiness from shared state.
2. **Preserved user behaviour** — every existing integration test keeps passing, asserts the same behaviour, and still passes when run alone (`-Dtest-filter`) and in any order. Test count does not drop.
3. **Optimal-way check** — the unconstrained optimum is one shared server+pool+Redis for the whole process; the acceptable gap is **per-file** sharing (each file registers its own route/handler set, so a single process-wide server can't serve all of them) plus a small shared datastore layer where safe.
4. **Rebuild-vs-iterate** — not a rewrite: the harness API stays; only its *lifecycle* (who owns it, how long it lives) changes. A parallel test runner is the bigger refactor and is explicitly Out of Scope (determinism/leak-audit risk).
5. **What we build** — a reusable shared-harness lifecycle + between-test isolation reset; a size-1 lazy test pool; three outlier-test fixes.
6. **What we do NOT build** — a custom/parallel test runner; per-test transaction-rollback isolation (the server thread holds its own pool conns, so a test-side txn can't wrap server queries); CI-lane changes (those are M102_002 / PR #461).
7. **Fit with existing features** — compounds with the M102_002 CI lane (faster lane + faster suite); must not destabilize the global `_reset-test-db` (the once-per-run schema reset stays the isolation backstop).
8. **Surface order** — N/A (internal test infra; no user surface).
9. **Dashboard restraint** — N/A.
10. **Confused-user next step** — a flaky/order-dependent failure after sharing → the failing test names the shared resource; the self-serve move is `-Dtest-filter="<test>"` to confirm it passes in isolation, pointing at a missing between-test reset.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal; specifically **NDC** (no dead code — remove any superseded per-test setup), **NLR** (touch-it-fix-it on harness files), **UFS** (the shared-harness sentinels / pool-size test constant must be named, not literal).
- **`dispatch/write_zig.md`** — the diff is `*.zig`: pg-drain lifecycle (`conn.query` → `.drain()` before `deinit`), tagged-union/`errdefer` placement on the shared-harness init/deinit, file ≤350 / fn ≤50 / method ≤70, cross-compile both linux targets.
- **`dispatch/verify.md`** — the Test-Delta + tiered VERIFY at close.

Not greenfield on rules — standard set applies in full.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all edits are `*.zig` | cross-compile `x86_64-linux` + `aarch64-linux`; pg-drain audit; read `dispatch/write_zig.md` |
| LIFECYCLE | yes — shared harness adds an init/deinit lifecycle | `errdefer` on each acquired resource; shared state freed exactly once (leak-clean) |
| PUB / Struct-Shape | yes — new shared-harness accessor surface | shape verdict per new `pub fn`/struct in the harness module |
| File & Function Length (≤350/≤50/≤70) | maybe — `test_harness.zig` may grow | split the shared-lifecycle helper into its own file if the cap nears |
| UFS | yes — sentinel keys / size-1 test constant | named constants (e.g. a test pool-size const), shared verbatim if cross-referenced |
| LOGGING / ERROR REGISTRY / SCHEMA / UI / DESIGN TOKEN | no | no logging-scope, no `UZ-` errors, no schema, no UI touched |

---

## Overview

**Goal (testable):** the integration suite boots each test file's httpz server + DB pool + Redis connection **once** (lazily, reused across that file's tests) and resets cross-test state between tests, so the measured ~1.25s/test setup floor collapses and the suite's CI wall-clock drops well below its current ~463s — with every test still passing in isolation and in any order.

**Problem:** `make test-integration` takes ~7.5 min on CI and has been **cancelled at the 600s timeout**. A per-test timing profile (1815 tests) shows **275 tests ≥500ms account for 95%** of the time, with a near-uniform **~1240–1260ms/test floor** across unrelated files — the signature of fixed per-test *setup*, not test logic. The setup is: a fresh 4-connection Postgres pool (eager-connect), a fresh Redis TLS connection, and a real httpz server, all built then torn down for **every** test.

**Solution summary:** Change the harness *lifecycle* (not its API) so the expensive setup is amortized per file, add between-test state isolation so sharing is safe, shrink the per-test DB pool to a single lazily-connected conn, and remove three fixed-duration waits in outlier tests. Net: the dominant 95% setup cost is paid ~58 times (once per file) instead of ~275+ times (once per test).

---

## Prior-Art / Reference Implementations

- **API/test-infra** → `src/agentsfleetd/http/test_harness.zig` is the reference to refactor (not replace). No in-repo shared-harness precedent — the lazy-singleton + between-test-reset lifecycle is defined here.
- **Pool** → `src/agentsfleetd/db/pool.zig` `parseUrl` is where the size/eager-connect defaults live; §2 mirrors its existing `Opts` shape with a test-scoped size.
- Divergence note: the vendored `pg` custom test_runner is **reference only** — we stay on Zig's default runner, so no `beforeAll`/`afterAll`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/test_harness.zig` | EDIT | add the shared-per-file lifecycle (lazy init, reuse, leak-clean teardown) + a between-test reset entry point |
| `src/agentsfleetd/http/handlers/common_authz.zig` | EDIT | `openHandlerTestConn` → size-1, lazy-connect test pool |
| `src/agentsfleetd/http/test_harness_server.zig` | EDIT (maybe) | allow server reuse across a file's tests if `bringUpServer` ownership needs it |
| `src/agentsfleetd/http/handlers/fleets/patch_concurrent_integration_test.zig` | EDIT | outlier: signal the lock-holder to release once the contending PATCH returns, instead of a fixed 7s sleep |
| `src/agentsfleetd/events/subscription_hub_test.zig` | EDIT | outlier: shorten/conditionalize the 5s bounded-stop wait |
| `src/agentsfleetd/queue/redis_pool_test.zig` | EDIT | outlier: shorten the 3.8s restart-reconnect probe window where safe |
| `src/agentsfleetd/http/**_integration_test.zig` (the ~58 harness files) | EDIT | adopt the shared-harness accessor + between-test reset; only as needed per file |

> Blast radius is wide-but-shallow: most edits are the mechanical swap from "init a harness" to "get the shared harness". The depth is in `test_harness.zig`.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections ordered by risk — §2 (trivial pool shrink) lands first as a measurable, isolated win; §1 (the shared-harness refactor, the 95%) is the prize; §3 (outliers) is independent cleanup. This lets the cheap win quantify the pool's share of the floor before the big refactor.
- **Alternatives considered:** (a) **process-wide single harness** — rejected: files register different route/handler sets, so one server can't serve all; (b) **per-test transaction rollback for isolation** — rejected: the httpz server thread issues queries on its *own* pool connections, outside any test-side transaction, so rollback can't isolate server-side writes; (c) **parallel test runner** — rejected: breaks the leak audit's determinism and the suite's serial-shared-datastore assumptions (separate, larger effort).
- **Patch-vs-refactor verdict:** **targeted refactor** of the harness lifecycle. It is not a mud-patch (it changes ownership/lifetime, the root cause) and not a rewrite (the harness API and every test's assertions are preserved).

---

## Sections (implementation slices)

### §1 — Shared-per-file harness with between-test isolation

Boot the server+pool+Redis once per test file via a lazily-initialized file-scoped accessor; reuse it across that file's tests; reset cross-test state between tests so order-independence holds; tear down leak-clean at process exit. **Implementation default:** per-file shared harness (one `getHarness()`-style accessor per file, lazy on first call) because a process-wide server can't host divergent route sets; shared long-lived state uses a non-`std.testing` allocator so the per-test leak detector stays clean. Between-test isolation default: **targeted truncation/reset of the tables a file touches** (or per-test unique identifiers where a file already does that), the agent picking per file — NOT a global re-migrate per test.

- **Dimension 1.1** — a file's second test reuses the first test's server+pool+Redis (no re-`init`) → Test `test_harness_reused_within_file`
- **Dimension 1.2** — every harness test passes when run alone via `-Dtest-filter` (isolation preserved) → Test `test_each_integration_test_passes_in_isolation`
- **Dimension 1.3** — shared harness frees its resources exactly once; no leak flagged by the default runner → Test `test_shared_harness_leak_clean`
- **Dimension 1.4** — between-test reset clears the prior test's rows so a fixed-id fixture does not collide → Test `test_between_test_reset_isolates_fixtures`

### §2 — Size-1, lazy-connect test DB pool

`openHandlerTestConn` builds a pool of **one** connection, connected lazily (not eager 4), since a test acquires exactly one. **Implementation default:** a named test pool-size constant + `connect_on_init_count = 0` (or the pg-zig equivalent), reusing the existing `Opts` shape. Independent of §1 and lands first to quantify the pool's share of the floor.

- **Dimension 2.1** — `openHandlerTestConn` opens ≤1 Postgres connection (not 4) → Test `test_handler_test_conn_opens_single_connection`
- **Dimension 2.2** — every DB-backed integration test still passes with the size-1 pool → Test (regression: existing DB suite green)

### §3 — Eliminate fixed-duration waits in outlier tests

Replace the three measured fixed waits with signalled/bounded coordination that returns as soon as the asserted condition holds.

- **Dimension 3.1** — `patch_concurrent` lock-contention test releases the holder once the contending PATCH has returned 503 (no fixed 7s) and still asserts fast-fail < the handler lock timeout → Test `test_patch_against_held_lock_503_fast` (existing, retimed)
- **Dimension 3.2** — `subscription_hub` bounded-stop asserts the bound without sleeping the full 5s where the condition is observable sooner → Test (existing, retimed)
- **Dimension 3.3** — `redis_pool` restart-reconnect proves reconnection without burning the full 3.8s window when reconnect is observable sooner → Test (existing, retimed)

---

## Interfaces

```
Test-only Zig surface (no HTTP/API change):

  // Per-file shared harness accessor (shape is the agent's call; contract:)
  getSharedHarness(file-scope) -> *TestHarness   // lazy init on first call, reused after
  resetBetweenTests(*TestHarness) -> void        // clears cross-test state; called per test

  openHandlerTestConn(alloc) -> ?{ pool: *db.Pool, conn: *pg.Conn }
    // unchanged signature; pool now size 1, lazy-connect
```

No production interface, endpoint, or schema changes. The agent picks names to match local style; the contract is "lazy, reused, reset-able, leak-clean."

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Cross-test state bleed | shared harness retains a prior test's rows/keys | between-test reset clears them; a missed table → the failing test names it; CI fails loudly, not flakily |
| Order-dependent pass | a test relies on another running first | each test must pass under `-Dtest-filter` in isolation (Dimension 1.2); violation fails CI |
| Leak on shared state | shared harness freed twice / never | freed exactly once at process exit via the lifecycle; default runner's per-test leak check stays green (Dimension 1.3) |
| Server reuse after crash | a file's shared server died mid-file | the accessor detects a dead server and re-inits, or the file fails fast with a clear error (no silent hang) |
| Pool starvation under size 1 | a test acquires 2 conns concurrently | such a test opts out to a larger explicit pool; the size-1 default documents the constraint |
| Outlier retime masks a real regression | shortened wait now passes a broken path | the retimed test still asserts the *behaviour* (fast-fail / reconnect / bounded-stop), only the wait shrinks |

---

## Invariants

1. **Test count is non-decreasing** — the suite runs ≥ the pre-change number of tests (no silent disable). Enforced: VERIFY Test-Delta row + `SUMMARY pass=` count compared to baseline.
2. **Order-independence** — every integration test passes when run in isolation (`-Dtest-filter`). Enforced: a CI/VERIFY spot-check running a sample of filters, plus the existing whole-suite run.
3. **Leak-clean** — the default Zig test runner reports zero leaks. Enforced: the runner's per-test leak detector (already active) + `make memleak`.
4. **No production behaviour change** — only `*_test.zig`, the harness, and `openHandlerTestConn` (test path) are touched; no handler/route/schema diff. Enforced: Files-Changed scope + diff review.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_harness_reused_within_file` | two tests in one file share one server/pool/Redis; the second performs no new `init` (instrument or observe a single bind) |
| 1.2 | integration | `test_each_integration_test_passes_in_isolation` | a representative harness test passes under `-Dtest-filter="<name>"` with no sibling tests |
| 1.3 | integration | `test_shared_harness_leak_clean` | running a file's tests reports 0 leaks from the default runner |
| 1.4 | integration | `test_between_test_reset_isolates_fixtures` | a fixed-id fixture inserted in test A is absent in test B after reset |
| 2.1 | unit/integration | `test_handler_test_conn_opens_single_connection` | `openHandlerTestConn` yields a pool reporting size 1 / ≤1 established conn |
| 2.2 | integration | regression: DB suite | the full DB-backed suite stays green with size-1 pool |
| 3.1 | integration | `test_patch_against_held_lock_503_fast` | contending PATCH returns 503 in < handler lock timeout; total test wall-clock no longer pinned to a fixed 7s |
| 3.2 | integration | `subscription_hub` bounded-stop (retimed) | stop returns within its bound; assertion unchanged, fixed 5s removed where observable sooner |
| 3.3 | integration | `redis_pool` restart-reconnect (retimed) | client reconnects after restart; proof no longer burns the full 3.8s window |

**Regression:** the entire pre-existing integration suite must stay green (this is a lifecycle refactor, not a behaviour change). **Idempotency/replay:** re-running the suite back-to-back yields identical results (the between-test reset + global `_reset-test-db` make it deterministic).

---

## Acceptance Criteria

- [ ] `make test-integration` passes with the same test count (±0 disabled) — verify: `make test-integration`
- [ ] Suite wall-clock drops materially vs baseline on CI — verify: compare the `test-integration` job's run step before/after on a PR
- [ ] A sampled set of harness tests passes in isolation — verify: `zig build test -Dtest-filter="<name>"` for ≥5 representative tests
- [ ] `make lint` clean · `make test` passes
- [ ] `make memleak` clean (shared-harness lifecycle touches allocator ownership)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: suite green + count — make test-integration 2>&1 | tail -5
# E2: Build — zig build 2>&1 | tail -3
# E3: Tests — make test 2>&1 | grep -E "passed|failed"
# E4: Lint  — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate — git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1>350{print "OVER: "$2": "$1}'
# E8: isolation spot-check — for t in <name1> <name2>; do zig build test -Dtest-filter="$t" 2>&1 | tail -1; done
```

---

## Dead Code Sweep

N/A at authoring — no files deleted; superseded per-test setup code is removed in-place under RULE NDC (the implementing agent lists any newly-dead helper before removing it, per VERIFY).

---

## Discovery (consult log)

> Empty at creation. Append consults, skill-chain outcomes, and any Indy-acked deferral quotes as work proceeds.

- Profile basis (this session): per-test timing of 1815 tests — 275 ≥500ms = 371.7s/95%; uniform ~1.25s/test floor; outliers 8.3s/5.0s/3.8s. Re-capture before trusting exact ms.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification | Clean; iteration count + coverage in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs spec, `dispatch/write_zig.md`, Failure Modes, Invariants | Clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff | Comments addressed before merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {paste} | |
| Integration tests + count | `make test-integration` | {paste} | |
| Isolation spot-check | `zig build test -Dtest-filter=<name>` | {paste} | |
| Lint | `make lint` | {paste} | |
| Memleak | `make memleak` | {paste} | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux` | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |
| Suite wall-clock delta | CI `test-integration` run step before/after | {paste} | |

---

## Out of Scope

- CI-lane changes (container/image, overlap, timeout) — that is M102_002 / PR #461, already landed.
- A custom or parallel test runner — separate, larger effort; breaks the leak-audit determinism assumed here.
- Per-test transaction-rollback isolation — incompatible with the server thread's own pool connections.
- Reducing the irreducible test *logic* time (HTTP + DB round-trips) below the per-test body cost — this spec targets setup amortization, not the work each test legitimately does.
