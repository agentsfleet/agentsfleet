# HANDOFF — M141_001, bounded runner lease fan-out

**Ephemeral.** Delete at CHORE(close); it briefs the next agent and never ships in the PR.

Written: Jul 26, 2026. Branch `feat/m141-lease-fanout`, **21 commits ahead of `origin/main`, not pushed, no PR.**
Rebased cleanly onto `origin/main` (`b2ca2afa9`) this session — zero conflicts.

---

## READ THIS FIRST — the 15 failures are already classified. Do not re-triage.

Everything is committed; the tree is clean. Latest full run: **2654 pass, 16 skip,
15 fail**. Of those 15, **8 fail in every run** (the deterministic core) and ~7
rotate run-to-run. The 8 have been individually re-run in isolation and sorted:

**Category A — fails ALONE. Genuine defects, 3 of them:**

| test | file |
|---|---|
| `consumer identity is stable: repeated idle probes leave one consumer in the group` | `event_lifecycle_reclaim_integration_test.zig` |
| `test_unaccepted_report_never_captures_completion` | `integration_roundtrip_test.zig` |
| `the reclaim chain enforces monotonic token ordering across runners` | `integration_roundtrip_test.zig` |

The two roundtrip ones are in the same file and probably share a root cause.

**`consumer identity is stable` is diagnosed — it asserts pre-M141 behaviour.** It
seeds `FLEET_IDLE` (a fleet with NO events), creates its consumer group, polls 25
times, and expects one consumer. Under ready-first an idle fleet is never in the
readiness index, so the poll never examines it, never issues `XREADGROUP`, and no
consumer is ever created. The test's INTENT — this process uses one stable
consumer name rather than minting `worker-{host}-{ts}` per probe — is still worth
proving; it just cannot be proven by polling a fleet that holds no work. Make the
fleet genuinely ready (publish, poll, repeat) and keep the `== 1` assertion.
**That is a test-vs-design call: show Indy the evidence before rewriting it.**

**Category B — passes ALONE. Cross-suite contamination, 5 of them:**

```
a budget-killed run persists failure_label=budget_breach on the event row
runner control plane — lease assigns across active fleets, sticky-preferred first
approval denial writes the terminal row: gate_blocked + approval_denied + XACK
a failed release degrades to TTL expiry and never masks the original reclaim error
terminal entry re-delivered from the PEL is re-acked, never re-executed
```

These are NOT broken tests and NOT broken code. Something earlier in the binary
leaves state they trip over. The readiness index is no longer it (`HLEN
fleet:ready` is 0 now) — look next at leftover rows on the SHARED runner
(`base.RUNNER_ID`) and at `fleet.runner_leases`, since every Category B failure is
a `pollLease` that returned nothing.

### The tool that made this possible — use it

```
/private/tmp/claude-501/-Users-kishore-Projects-agentsfleet/287b3733-9a6c-4987-bb0f-3392b887b1f9/scratchpad/itest.sh
```

Runs ONE filtered suite against the already-running per-worktree infra in **~2
seconds** instead of ~15 minutes, reading the container ports from `docker port`
so it cannot dial a stale one. Copy it somewhere durable — it is in a session
scratchpad and will vanish.

```bash
./itest.sh -Dtest-filter="consumer identity is stable" --summary all
```

**Check the skip count on every focused run.** `69 pass` means it ran; `68 pass, 1
skip` means it did not and you are reading nothing. If a suite skips, refresh the
cert — the script does it, but confirm `.tmp/redis-ca.crt` is non-empty.

---

## How to read a run — three sessions got this wrong in three different ways

1. **`pass + skip + fail`, always together.** A run with high skips is not a better
   run. Skips have been a steady 16 for the last three runs; a jump means suites
   stopped running, not that they started passing.
2. **`make test-integration; echo $?` does NOT give you make's exit code** — the
   `;` makes it the echo's. Use `make test-integration; RC=$?` or redirect properly.
   A previous message in this session reported "exit 0" on a run with 17 failures
   because of exactly this.
3. **The suite is NOT deterministic even with a pinned seed.** `SEED=` now passes
   through to `zig build --seed` (added this session), which fixes *test order* —
   but `fleet_ready.peek` uses `HRANDFIELD`, which randomises server-side
   regardless. Two runs at seed `0x6e910394` gave 13 identical / 3 gone / 4 new.
   **Do not treat a changed failure list as evidence a fix worked.** Compare the
   COUNT across several runs, and prefer a mechanism you can explain.

Run numbers so far, all post-rebase, all on isolated per-worktree infra:

| run | pass | skip | fail | note |
|---|---|---|---|---|
| integ_full | 2652 | 16 | 17 | first trustworthy run |
| integ2 | 2653 | 16 | 16 | after 3 invalidation/reset fixes |
| integ3_seeded | 2652 | 16 | 17 | same seed as integ2 — set still differed |
| integ4 | 2654 | 16 | 15 | after the mark-leak fix; `HLEN fleet:ready` now **0** |

---

## The infra blocker is GONE — do not re-diagnose it

M143_001 merged. `make test-integration` now brings up **per-worktree** containers
on dedicated ports (`agentsfleet-m141-lease-fanout-{postgres,redis,qstash}` on
28025/28026/28027). Verified with `docker ps`. The old fixed-name containers that
made three sessions' numbers meaningless are gone.

**Ignore the previous handoff's instruction to check `docker --context colima ps`.**
Indy has explicitly dropped that line of investigation. Do not stop work on it.

Redis for manual poking:
```bash
docker exec agentsfleet-m141-lease-fanout-redis-1 sh -c \
  'redis-cli --tls --insecure -a agentsfleet --no-auth-warning HKEYS fleet:ready'
```
**`HLEN fleet:ready` should be 0 after a clean run.** It was 5 before this
session's fix. Any non-zero value names a suite that leaks a readiness mark — the
field IS the fleet id, so `grep -rn "<id>" src/` finds the owner immediately. That
is the single highest-value diagnostic in this workstream.

---

## Fixed this session (all committed except the four files above)

- **`common.CacheTable`** (`src/lib/common/cache_table.zig`) — set-associative,
  allocator-free, per-entry expiry, unsynchronised so each consumer picks its own
  lock. Adapted from ghostty's `datastruct/cache_table.zig` (MIT) plus bun's
  digest-as-bucket-index (MIT). Replaces the two hand-rolled direct-mapped tables.
  `runner_token_cache_test.zig` carries **zero diff** and still passes — that is
  the evidence the auth refactor is behaviour-preserving.
- **`common.RwLock`** in `sync.zig`, so `fleet_group_memo.isEnsured` holds the lock
  SHARED on the publish path.
- **`liveness_sweeper` invalidates the token memo.** It is a THIRD writer of
  `admin_state` (`markDrainedIfIdle`, `draining → drained`) and was leaving its own
  machine's cache stale. Production fix.
- **`test_harness.zig` resets the group memo per start.** It always reset the token
  cache; the group memo had no reset anywhere. Redis is flushed once per binary
  while the memo lives for the whole binary.
- **`runner_enrollment` gate fixture invalidates like production.** It flipped
  `admin_state` with raw SQL, so the memo kept answering `active`. Assertions
  untouched. This failure is deterministically gone.
- **Eviction-hook leak in `CacheTable`** — `removeAt` (called by `get`-expiry,
  `remove`, `removeMatching`) and `put`'s reuse path never fired `Context.evicted`.
  Found because **`agentsfleet-m143-read-surfaces` already consumes this table**
  with a heap-owned `[]u8` value and a byte tally; it would have leaked on every
  refresh and every expiry. All drops now route through one `release()`.
- **`SEED=` passthrough** in `make/test-integration.mk`.

---

## NOT done

1. **The full suite is not green.** 16 failures as of integ2, in
   `event_lifecycle_reclaim`, `control_plane_*`, `integration_roundtrip`,
   `budget_gate`, `event_lifecycle`. **Indy has ruled these IN SCOPE for this PR**
   — verbatim: *"well the failures of the CI steps must be fixed in the PR, since
   PR get blocked on merge."* There is no deferral option; do not ask for one.
2. **Rubric rows are graded from per-suite runs, not a full one.** 13 rows carry ✅
   that a green full run must re-earn. R10 (§7) is ungraded.
3. **VERIFY tiers not run since the rebase**: `make test-unit-all`, `make memleak`,
   `make harness-verify`, `gitleaks`. Cross-compile IS green (both linux targets).
4. **REVIEW not started** — gstack `/review` on the branch diff, local, pre-commit.
5. **Docs PR not opened.** `/tmp/docs-m141`, branch
   `chore/m141-lease-fanout-changelog`, commit `4de4347`, **pushed, no PR**. Its
   changelog entry is accurate and unaffected by §7 (internal storage only).
6. **CHORE(close)** — spec `active/` → `done/`, `Status: DONE`, PR Session Notes,
   version sync, orphan sweep, delete this file.

---

## Verified facts — do not re-derive

- **`agentsfleet-m143-read-surfaces` is unrelated to the failures.** Not merged
  into main; its diff touches zero files under `fleet/`, `queue/`, or `auth/`; the
  failing suites were failing before any M143 work merged. It DOES depend on
  `common.CacheTable`, so landing this branch unblocks them.
- **Dimensions 3.2–3.5 ARE tested.** The previous handoff called the sweeper
  backstop "entirely unverified"; `2b6d586b7` added
  `reclaim_sweeper_readiness_integration_test.zig` (380 lines) covering all four
  plus the depth sample. A grep for `test_pending_entry_keeps_fleet_ready` returns
  nothing because the spec labels tests in snake_case while the Zig tests carry
  prose names. Coverage is real; the naming mismatch is a spec tidiness issue.
- **Test delta is healthily positive.** Baseline `unit=2958 integration=393`;
  now `unit=3137 integration=426`. Satisfies the VERIFY Test Delta row.
- **`stream_registry_test.zig` shares two UUIDs with `messages_integration_test.zig`**
  (`ZID_A`/`FLEET_IDLE`, `ZID_B`/`AGENTSFLEET_ACTIVE`). **Harmless — left alone
  deliberately.** stream_registry only uses them against an in-memory
  `reg.tryRegister`; it never touches Redis or Postgres.
- **`make test` does not exist.** The dispatch doc `write_zig.md` names it; the real
  targets are `test-unit-all`, `test-unit-agentsfleetd`, `test-unit-agentsfleet-lib`.

---

## Dead ends — falsified, do not redo

1. **`text[]` → `uuid[]` cast** in the candidate query. Verified against the live DB.
2. **Suites overwriting each other's encryption key.** `setTestEncryptionKey()` is
   idempotent, same key everywhere.
3. **Pinning the test seed makes the suite deterministic.** It does not — see
   §"How to read a run" item 3. `HRANDFIELD` is the remaining source.
4. **Readiness index overflowing its 64-entry sample.** `HLEN fleet:ready` was 5 at
   end of run, far under `MAX_READY_CANDIDATES_PER_POLL = 64`. The problem was
   leaked marks changing WHICH fleet a poll draws, not the bound being exceeded.
5. **Clearing the readiness index at harness start OR teardown.** Both tried, both
   wrong — start erases marks a test publishes before standing up another harness
   (cost 10 failures); teardown wipes marks a later test in the same file needs.
   Reverted in `61afe6738`. The per-suite `forgetFleet` purge is the right
   granularity.
6. **Adopting ghostty's threaded-sweeper model.** Investigated at Indy's request
   and **explicitly dropped by him** — verbatim: *"Oksy lets skip the ghosttty
   style thread sweeper. its not needed now."* The analysis: our three sweepers
   have different cadences and different failure domains (liveness is
   Postgres-only), so bun's single-timer-heap model would couple them, and
   ghostty's model buys only ~1s of shutdown latency while requiring a new
   signalable wait primitive in the shutdown path — the one place a missed wakeup
   hangs rather than degrades. Do not reopen.

---

## Decisions settled with Indy — do not re-litigate

- **CI failures are fixed in the PR, never deferred** (quote in §NOT done item 1).
- **Auth cache shape**: in-process, `sha256(agt_r) → {runner_id, active}`, expiry
  pinned to `HEARTBEAT_INTERVAL_MS` (10s), invalidated by every writer of
  `admin_state`. Never caches the raw token; misses are never memoised.
- **Prod runs 2–3 agentsfleetd machines**, not one. `runner_fleet.md` corrected.
- **`fleet_group_memo` converts to the shared table**, accepting the loss of
  lock-free atomics for an `RwLock` — Indy chose this with the cost stated.
- **The shared cache is built for other repos**, so `cache_table.zig` is std-only,
  MIT-attributed, and liftable into a package without edits.
- **UUIDv7 token**, sweep bound stays 100, orphan prune stays out of the sweeper,
  three scheduler knobs are deliberate placeholders — do not tune.

---

## What I got wrong — so it does not cost you time

1. **Reported "exit 0" on a run with 17 failures.** My own `make …; echo $?`
   captured the echo. Corrected within the same turn, but check your redirection.
2. **Predicted a pinned seed would reproduce the failure set.** It did not. Order
   was never the only source of nondeterminism.
3. **Bundled a commit by accident** — files staged from a hook-blocked attempt got
   swept into a spec-only commit. `git commit` commits everything staged, not just
   what you just added. Split it before pushing.
4. **Guessed twice at the leak mechanism before measuring** (cross-file
   `FLEET_IDLE` collision; index overflow). Both wrong. `HKEYS fleet:ready` +
   `grep` answered it in two commands. Measure first.
