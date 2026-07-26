# HANDOFF — M141_001, bounded runner lease fan-out

**Ephemeral.** Delete at CHORE(close); it briefs the next agent and never ships in the PR.

Written: Jul 26, 2026. Branch `feat/m141-lease-fanout`, 7 commits, **not pushed**, no PR.

---

## READ THIS FIRST — the last thing I did is the most likely thing to revert

`make test-integration` went from **45 failures to 81** across my own commits. The
prime suspect is my last one, `a7e00b09b`, which clears the whole `fleet:ready`
index at **every** `TestHarness.deinit`.

| run | pass | skip | fail | crash | leaks | at commit |
|---|---|---|---|---|---|---|
| clean baseline | 2439 | 80 | 43 | 2 | 1344 | `6808195dd` (pre-mine) |
| after fixture fixes | 2501 | 19 | **45** | 0 | 63 | `757b62d58` |
| cert-poisoned, ignore | 2209 | **336** | 22 | 0 | 42 | `a7e00b09b` |
| uncontended | 2419 | 67 | **81** | 0 | 1087 | `a7e00b09b` |

**Do not read the "22 fail" row as progress — 336 tests never ran.** I briefly
reported it as an improvement; it was not, and comparing runs without checking
the skip count is how that happened. Always compare `pass + skip + fail`.

**Hypothesised mechanism:** a suite where test A publishes an event and a LATER
test in the same file leases it now loses the mark, because A's harness teardown
wipes the shared index. That fits the new failures landing in publish-then-lease
suites (`registry` 9, `slack` 5+2+2+1, `qstash` 5, `oauth_providers` 4) — none of
which I touched.

**First action:** `git revert a7e00b09b` (or just the `deinit` call, keeping
`expectLease`), run the full suite, compare. If it drops back toward 45, the
deinit reset is wrong and the per-suite `forgetFleet` cleanups in `757b62d58` are
the right granularity. I ran out of runway to test this.

Note `a7e00b09b` bundles two unrelated changes — the deinit reset AND
`expectLease`. **Keep `expectLease`**; it is independently correct (see §Fixed).

---

## Mandate (unchanged)

> Indy: "i am not worried about who broke it, if it break then fi it"
> Indy: "the agent has to pickup and get moving and ensure test-integration
> succeeds (not tuck under the cover complaining with a blabber saying oh i
> didnt do it - its not my code)."

You may **not** weaken a test to get green — no skips, no loosened assertions, no
swallowed errors. If a test asserts the wrong thing, show evidence and ask Indy.

---

## The root cause I did establish (this part is solid)

One mechanism produced ~26 of the original 43 failures. Verified end to end:

```
fleet:ready is ONE Redis key; peek is HRANDFIELD bounded at 64 AND randomized
  │  9 suites publish fleet events; only 1 cleared the index
  ▼
grant test's fleet crowded out of the sample → lease returns null
  │  test did `.?.object` on null → SIGABRT (not a failure — a PANIC)
  ▼
crashed test skips `defer cp.cleanupAll`
  ├─ runner-cp-a survives → token_hash collides with placement's GPU_TOKEN
  │    (both `agt_r` + "a"×64; seeds use ON CONFLICT (id), which does NOT
  │     guard the uq_runners_token_hash UNIQUE constraint) → 6 placement fail
  └─ workspace 2b3e1e0d6011 survives → lowest uuid under the shared test tenant
       → secret_probe.resolvePrimaryWorkspace orders by (created_at,
         workspace_id) over rows every fixture seeds with created_at = 0
       → tenant_provider gets someone else's workspace → 12 fail NotFound
```

That explains the run-to-run drift the previous handoff saw (44→41→38): it was
the **randomized sample**, not vague "shared-state fragility".

---

## Fixed (4 commits, no test weakened)

```
a7e00b09b test(m141): reset readiness at harness teardown + expectLease   ← SUSPECT
757b62d58 test(m141): stop seven suites leaking marks into the shared index
92a37c9f0 fix(m141): clear readiness when a fleet stops being leasable
```

- **`92a37c9f0`** — production. `fleet_ready.forceClear` (unconditional HDEL) +
  `redis_fleet.purgeFleetRedisState` (stream + mark). Called from `delete.zig`
  and `patch.zig` when a fleet leaves `active`. Approved by Indy in-session.
  Also retires `delete.zig`'s inline `fleet:{s}:events` literal — the one
  production match the spec's own Dead Code Sweep expects to be zero.
- **`757b62d58`** — 7 suites now purge stream+mark together; placement's runner
  tokens carry its own node suffix so the `token_hash` collision cannot recur.
- **`expectLease`** (inside `a7e00b09b`) — a null lease returned
  `error.ExpectedLeaseGotNull` instead of panicking the binary. **Keep this**
  even if you revert the rest: the panic is what turned one assertion into ~26
  failures by skipping every remaining `defer`.

Spec updated: Files Changed rows, Dimension 3.7, Failure Modes row for the
orphan residual, Test Specification row 3.7.

---

## NOT done

1. **Dimensions 3.2–3.5 have no tests at all.** Named in the spec's Test
   Specification, absent from `src/`. Verified by grep:
   `test_pending_entry_keeps_fleet_ready`,
   `test_sweeper_recovers_undelivered_without_pel_entry`,
   `test_reclaimed_stray_remarks_readiness`,
   `test_sweeper_scan_advances_across_passes`. **The sweeper backstop — the
   thing that makes losing a mark safe — is entirely unverified.** M141 cannot
   honestly be called done until these exist.
2. **6 `control_plane_policy` failures.** `cp.seedActiveLease` writes
   `event_id = 'evt-seed-1'`, an id **never in the Redis stream** — no PEL
   entry, no mark. Pre-M141 the poll walked every fleet and found the lease in
   Postgres; under ready-first, discovery goes through Redis and finds nothing.
   Options discussed with Indy, recommendation = **seed the state the way
   production makes it** (publish a real event → lease it once → expire it), so
   the mark is a *consequence*, not a fixture input. Indy's own objection to
   hand-marking: it drives mark-loss coverage to zero, which is itself the
   argument against it.
3. **`control_plane_integration_test.zig` and `control_plane_policy_integration_test.zig`
   share EVERY fixture id** (`WORKSPACE_ID`, `RUNNER_A_ID`, `AGENTSFLEET_1_ID`,
   `LEASE_OLD_ID`, `AFFINITY_1_ID`). That is a separate order-dependence bug
   from the reclaim one — the "fresh lease" tests pass alone and in pairs, and
   fail only interleaved. Give one file its own ids.
4. **Token-verdict auth cache** — approved, unbuilt. Without it the milestone
   title is false: an idle poll still costs one Postgres lookup at
   `cmd/serve_runner_lookup.zig:34`. Design settled: in-process
   `sha256(agt_r) → {runner_id, active}` in agentsfleetd, `PUBLISH` on
   admin_state change (`http/handlers/fleet/runner_patch.zig`) and on runner
   delete, per-replica subscriber drops the entry, long TTL only as a backstop
   for a PUBLISH missed during a redial. Never cache the raw token.
   `runner_fleet.md`'s "Prod runs a single agentsfleetd machine today" is stale —
   fix it when you build this. Do NOT mirror `fleet_group_memo.zig`: that is a
   lossy hint, and a stale "active" here would let a deactivated runner lease.
5. VERIFY rubric / review / changelog / CHORE(close) / PR — not started.

---

## Verified facts — do not re-derive

- **`make memleak` PASSES** (exit 0, all four lanes). The `N leaks` in
  test-integration output is a DIFFERENT thing: `std.testing.allocator` reports,
  dominated by the pre-existing 21-allocation `TestHarness` leak.
- **The 21-allocation leak is NOT M141's.** `git diff origin/main..7633f4741 --
  src/agentsfleetd/http/test_harness.zig src/agentsfleetd/db/test_fixtures.zig`
  is EMPTY, and it fires in suites M141 never touches (`concurrency_renew`,
  `renewal_integration`, `renewal_malformed`, a dozen connector suites).
- **Infra collision is SOLVED by M143_001, which is unmerged.** Their branch
  removes `container_name` from postgres/redis/qstash and assigns per-worktree
  ports via `scripts/test-infra-ports.sh`; their `_ensure-test-infra` deleted the
  stale-container sweep and added a hard cert-staleness check. **This branch
  still has the OLD recipe** that force-removes `agentsfleet-{postgres,redis,qstash}`
  by fixed name and squats 5432/6379/8080. Until M143_001 merges, THIS worktree
  is the one that can destroy a sibling's run — including via `make memleak`,
  which also calls `_ensure-test-infra`.

---

## Dead ends — falsified, do not redo

1. **`text[]` → `uuid[]` cast** in the candidate query. Verified against the live
   DB; works. (Carried from the previous handoff, still true.)
2. **Suites overwriting each other's encryption key.** `setTestEncryptionKey()`
   is idempotent, same key everywhere.
3. **`uq_runners_token_hash` — the previous handoff called this a falsified dead
   end. IT IS REAL.** They tested it against an empty table, where it looks
   innocent. Under the actual crash sequence it fires. Fixed in `757b62d58`.
4. **Clearing the readiness index at harness START.** Looks equivalent to
   teardown-side and is not — it erases marks a test publishes before standing up
   another harness. Cost 10 failures. (Teardown-side may ALSO be wrong; see the
   top of this file.)
5. **"Mark on lease issue" as a fix for the control_plane_policy tests.** Cannot
   work: the fixture writes the lease row with raw SQL and never travels the
   lease path.

---

## Reproduction

```bash
cd /Users/kishore/Projects/agentsfleet-m141-lease-fanout
make test-integration 2>&1 | tee /tmp/integ.log
grep -oE "error: '[^']+' (failed|terminated)" /tmp/integ.log \
  | sed "s/error: '//" | sed 's/\.test\..*//' | sort | uniq -c | sort -rn
grep -E "pass,|Build Summary" /tmp/integ.log | tail -2   # ALWAYS check skips
```

`itest.sh` (untracked, at repo root — delete at CHORE(close)) runs ONE suite in
~20s instead of ~12min. **Before every use**, refresh the cert or harness tests
silently skip:

```bash
docker cp agentsfleet-redis:/tls/server.crt .tmp/redis-ca.crt
./itest.sh -Dtest-filter="required tag subset" --summary all
```

A "skip" in an `itest.sh` run almost always means a stale CA cert, not a
self-skipping test. Check `pass/skip/fail` on every run — that is the lesson
that cost me a false progress report.

---

## Decisions settled with Indy — do not re-litigate

- **UUIDv7 token**, not a counter. Reasoning in `queue/fleet_ready.zig`.
- **Sweep batch bound stays at 100.** Ack quote in the spec's Discovery. The
  keyset cursor is NOT part of that deferral.
- **Auth cache invalidates on change, not a TTL.**
- **Three scheduler-adjacent knobs are deliberate placeholders** — randomized
  sampling, `MAX_READY_CANDIDATES_PER_POLL = 64`, the polled repair cadence. Do
  not tune them.
- **Orphan prune stays out of the sweeper.** Below 64 entries `HRANDFIELD`
  returns every field so orphans cost nothing; the depth gauge climbs months
  before latency moves; recovery is a `DEL` or a surgical `HDEL` cron. Indy
  raised a durable tombstone queue so a failed `forceClear` is retryable —
  recorded in the spec's Failure Modes as scheduler scope.

---

## What I got wrong — so it does not cost you time

1. **I reported "22 failures" as progress without checking the skip count.** 336
   tests had not run. Always compare `pass + skip + fail`.
2. **I declared the previous handoff's Leads A and B "ghosts"** after ONE run —
   a run that was poisoned by a sibling worktree. They were real and reproduced
   exactly. One contaminated data point is not a refutation.
3. **I diagnosed the infra collision correctly at 22:45, then asserted it as
   present-tense fact for hours** without re-checking, and asked Indy to pause a
   session on that basis. M143_001 had already fixed it. Check `docker inspect`
   labels and `ps` at the moment you claim interference.
4. **I claimed "it's not normal to have Redis calls in API handlers."** Wrong —
   `create.zig:180` creates the stream, five ingress handlers call
   `xaddFleetEvent`. The real defect was a duplicated key literal, not layering.
5. **I put the index reset at harness start before thinking through
   publish-then-start ordering.** Cost 10 failures and a cycle.

The pattern in all five: asserting from a stale or single observation instead of
checking. The commands were always available.
