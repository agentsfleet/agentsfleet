# HANDOFF — M141_001, bounded runner lease fan-out

**Ephemeral.** Delete this file at CHORE(close); it briefs the next agent and never
ships in the Pull Request (PR).

Written: Jul 25, 2026. Branch `feat/m141-lease-fanout`, 7 commits, **not pushed**,
no PR yet.

---

## YOUR MANDATE

**Make `make test-integration` exit 0 on this branch, then finish M141 to PR.**

It currently does not. You own that outcome regardless of which commit introduced
each failure. Indy's direction, verbatim:

> Indy (2026-07-25): "i am not worried about who broke it, if it break then fi it"

> Indy (2026-07-25): "the agent has to pickup and get moving and ensure
> test-integration succeeds (not tuck under the cover complaining with a blabber
> saying oh i didnt do it - its not my code)."

So: **do not** write an analysis explaining that `tenant_provider_endpoint` is
someone else's suite. Do not file the remaining failures as a follow-up spec and
call this done. Fix them. If a fixture in an unrelated suite is wrong, fix the
fixture. If the shared harness leaks, fix the harness. The bar is a green suite.

The one thing you may **not** do is weaken a test to get green — no skipping, no
loosened assertion, no `catch {}` to swallow a failure. If you believe a test is
asserting the wrong thing, say so with evidence and get Indy's call.

---

## Scope / status

M141 makes an idle runner lease-poll cost one bounded Redis read and **zero**
Postgres round-trips, instead of walking every active fleet on the platform (3
Postgres + 3 Redis calls per fleet, per poll, per runner).

| Piece | State |
|---|---|
| Readiness index `queue/fleet_ready.zig` (mark / peek / clear / depth, UUIDv7 token) | ✅ built, unit-tested |
| Ready-first lease path `fleet/assign.zig` + bounded query in `fleet/sql.zig` | ✅ built |
| Token-guarded clear at the single `acquireFresh` no-work site | ✅ built |
| Consumer-group memo `queue/fleet_group_memo.zig` | ✅ built, unit-tested |
| Producer relocated to `queue/redis_fleet.zig` (+12 call sites) | ✅ built |
| Deliverability probe `queue/redis_fleet_probe.zig` (undelivered + PEL) | ✅ built, unit-tested |
| Sweeper: readiness backstop, keyset cursor, depth sampling | ✅ built |
| Poll-cost metrics (5 families) | ✅ built, render-tested |
| `docs/architecture/{scaling,runner_fleet}.md` corrected | ✅ done |
| 10 integration proofs `fleet/assign_ready_integration_test.zig` | ✅ written; see caveat below |
| **`make test-integration` green** | ❌ **YOUR JOB** |
| Token-verdict auth cache | ❌ approved, not built (see Decisions) |
| VERIFY rubric / review / changelog / CHORE(close) / PR | ❌ not started |

**No Dimension in the spec is marked DONE yet.** Done means tested, and the suite
is not green. Mark them as you verify them.

---

## Working tree / branch

```
## feat/m141-lease-fanout      (clean apart from this file)
7633f4741 test(m141): stop the readiness suite polluting the shared index
ca0baf803 refactor(m141): cut the machinery that outweighed what it bought
0b3fd1c5c test(m141): name the readiness proofs into the integration tier
cfcfc05ae docs(m141): correct the idle-cost model and add the readiness namespace
f791bc983 feat(runner): bound the lease-poll fan-out to ready fleets
9b9cf2756 feat(runner): add the readiness index, group memo, and poll-cost metrics
cf09be36d chore(m141): open the bounded lease fan-out workstream
```

Worktree: `/Users/kishore/Projects/agentsfleet-m141-lease-fanout`. Stay in it.
Spec: `docs/v2/active/M141_001_P0_API_DOCS_OBS_BOUNDED_RUNNER_LEASE_FANOUT.md`.

---

## Running processes / infra

No tmux sessions. Docker test infra is **up** (leave it up, it saves ~2 min/run):

```
agentsfleet-postgres   :5432   healthy
agentsfleet-redis      :6379   healthy
agentsfleet-qstash     :8080   healthy
```

If it is down, `make test-integration` brings it up itself via `_ensure-test-infra`
(needs the Docker daemon running: `open -a Docker`).

---

## Tests — what was actually run

```
✅ zig build                            clean
✅ zig build test        (unit tier)    exit 0
✅ make lint                            passed
✅ make harness-verify                  ALL GATES GREEN
✅ make _lint_zig_test_depth            unit=2995  integration=403
                                        (baseline at CHORE(open): unit=2958 integration=393)
❌ make test-integration                MAKE_EXIT=2
```

### Read this carefully before you re-run

The last **full** `make test-integration` (38 fail / 1 crash / 1449 leaks) ran
**before** commit `7633f4741`, which fixed the readiness suite's index pollution.
Its numbers are therefore stale, and in particular the 3 `assign_ready` failures
it shows are already fixed.

After `7633f4741`, a filtered run (`-Dtest-filter="integration: "`) showed
`assign_ready_integration_test` with **zero failures**. Its only remaining report
is a *leak* that resolves to `base.setup()` — the shared `TestHarness` — and is the
identical 21-allocation signature that a dozen unrelated connector suites also
report. That leak is harness-level.

**So: your first action is a fresh full run to get a true current number.** Do not
plan against the 38.

### Latest full-run failure distribution (stale, pre-`7633f4741`)

```
 10  state.tenant_provider_endpoint_test          vault row NotFound
  7  fleet.placement_eligibility_test             ALL of its tests fail
  3  fleet.event_lifecycle_reclaim_integration_test
  3  fleet.control_plane_policy_integration_test
  3  fleet.assign_ready_integration_test          <- FIXED in 7633f4741
  2  state.tenant_provider_test
  2  fleet.event_lifecycle_integration_test
  2  fleet.credit_metric_reconciliation_test
  2  fleet.budget_gate_integration_test
  1  fleet.integration_roundtrip_test
  1  fleet.control_plane_integration_test
  1  events.subscription_hub_test
```

Counts **vary run to run** (44 → 41 → 38), which is the signature of shared state
rather than a fixed ordering bug. Three suites were stable across every run:
`tenant_provider_endpoint` (10), `tenant_provider` (2), `placement_eligibility` (7–8).

---

## Reproduction recipes (copy-paste)

**Full suite:**
```bash
cd /Users/kishore/Projects/agentsfleet-m141-lease-fanout
make test-integration 2>&1 | tee /tmp/integ.log
grep -oE "error: '[^']+' failed" /tmp/integ.log \
  | sed "s/error: '//;s/' failed//" | sed 's/\.test\..*//' | sort | uniq -c | sort -rn
```

**One suite in isolation** — this is the highest-value tool here, because it is how
you separate "broken code" from "shared state". Save as `itest.sh`, `chmod +x`:

```bash
#!/bin/bash
# Replicates _test-integration-full's env WITHOUT the schema reset, so a single
# suite can run in seconds instead of ~12 minutes.
cd /Users/kishore/Projects/agentsfleet-m141-lease-fanout
export ZIG_GLOBAL_CACHE_DIR="$PWD/.tmp/zig-global-cache"
export ZIG_LOCAL_CACHE_DIR="$PWD/.tmp/zig-local-cache"
export AGENTSFLEET_RUNNER_BIN="$PWD/zig-out/bin/agentsfleet-runner"
export LIVE_DB=1
export TEST_DATABASE_URL="postgres://agentsfleet:agentsfleet@localhost:5432/agentsfleetdb?sslmode=disable"
export TEST_REDIS_TLS_URL="rediss://:agentsfleet@localhost:6379"
export REDIS_URL_API="$TEST_REDIS_TLS_URL"
export REDIS_TLS_CA_CERT_FILE="$PWD/.tmp/redis-ca.crt"
exec zig build test "$@"
```

```bash
./itest.sh -Dtest-filter="required tag subset" --summary all   # placement, alone
./itest.sh -Dtest-filter="readiness index"     --summary all   # mine, alone
```

Needs `.tmp/redis-ca.crt`, which `_ensure-test-infra` writes; run `make test-integration`
once first, or `make _ensure-test-infra`.

**Inspect the live datastores:**
```bash
docker exec agentsfleet-postgres psql -U agentsfleet -d agentsfleetdb -c "\d fleet.runners"
docker exec agentsfleet-redis redis-cli --no-auth-warning -a agentsfleet HGETALL fleet:ready
```

---

## Dead ends — I falsified these, do not redo them

1. **`text[]` → `uuid[]` cast in the new candidate query.** Suspected as the cause
   of every lease failure. Verified directly against the live database: the cast
   works. Not it.

2. **`uq_runners_token_hash` vs the fixture's `ON CONFLICT (id)`.** `fleet.runners`
   really does have a UNIQUE constraint on `token_hash` that the
   `placement_eligibility` fixture does not guard — but the tokens it seeds are all
   distinct (`"a"*64`, `"b"*64`, `"c"*64`) and the table was empty. Plausible in
   theory, not what is firing. **I flagged this to Indy as "the concrete lead"
   before verifying it. That was wrong of me — verify before you assert.**

3. **Suites overwriting each other's encryption key.** `setTestEncryptionKey()` →
   `crypto_primitives.setTestKek()` is idempotent and every suite uses the same
   test key. Not it.

---

## Live leads (unverified — verify before acting)

**Lead A — the vault row is absent, not undecryptable.** `tenant_provider_endpoint`
fails at `crypto_store.zig:92` returning `SecretError.NotFound`, reached via
`vault.loadJson` ← `secret_probe.loadSelfManagedJson` ← `probeSelfManagedSecret` ←
`tenant_provider.upsertSelfManaged` ← the test's own line 29. `NotFound` means no
row at `(workspace_id, key_name)`. Since the key is shared and idempotent, the
likely mechanism is a **teardown in some other suite deleting a workspace or
tenant row, cascading away `vault.secrets`** that this suite's secret hangs off.
Grep for `DELETE FROM core.workspaces`, `DELETE FROM tenants`, and any
`ON DELETE CASCADE` reaching `vault.secrets`, then check which suites run before it.

**Lead B — all 8 `placement_eligibility` tests fail together.** Every test in that
file failing (not a subset) points at something in shared setup —
`base.setup()` / `seedPlatformProvider` / `seedRunnerWithLabels` — rather than at
per-test logic. `seedPlatformProvider` writes a vault credential, so **Lead A and
Lead B may be one root cause.** Chase that connection first; it would collapse ~19
of the failures into one fix.

**Lead C — the 21-allocation harness leak.** `base.setup()` leaks 21 allocations,
attributed to whichever test called it, across at least a dozen suites. Pre-dates
this branch. It does not fail a test on its own but it pollutes every leak report,
including the ones you will need to read. Worth fixing early just to get a clean
signal.

**Lead D — `subscription_hub` (1–4, varies).** SSE pub/sub timing. Lowest priority;
most likely genuinely flaky rather than state-dependent.

---

## Ordered next steps

1. **Fresh full `make test-integration`** for a true baseline. The 38 is stale.
2. **Chase Lead A + Lead B as one root cause** (vault row cascaded away by another
   suite's teardown). Highest expected payoff: ~19 of the failures.
3. **Re-run.** Use `itest.sh` per suite to confirm each fix in seconds rather than
   re-running the full 12 minutes.
4. **Fix the residual** (`event_lifecycle_reclaim`, `control_plane_policy`,
   `credit_metric`, `budget_gate`, `integration_roundtrip`, `control_plane`) once
   the shared-state cause is out of the way — several are probably knock-ons.
5. **Lead C** (harness leak) and **Lead D** (hub timing).
6. **Build the token-verdict auth cache.** See Decisions — design is settled, code
   is not written. Without it the milestone title is literally false: an idle poll
   still costs one Postgres lookup for runner-token auth
   (`cmd/serve_runner_lookup.zig:34`).
7. **Mark spec Dimensions DONE** as each is verified by a green test.
8. **VERIFY** the Acceptance Rubric verbatim, then `/write-unit-test`, then gstack
   `/review`, then changelog `<Update>` in `~/Projects/docs/`, then CHORE(close)
   (spec `active/` → `done/`, delete this file), then push + `gh pr create`, then
   `kishore-babysit-prs`.

---

## Decisions already made — do not re-litigate

**Token = minted UUIDv7**, not a counter. Indy's own suggestion. Random enough
never to collide, time-ordered so the index is debuggable, allocation-free, and it
needs no second Redis key — which is what kills the eviction/reset/clock residual
that every counter shape carried. `queue/fleet_ready.zig` documents the reasoning.
The archived greptile R1–R6 record in the spec's Discovery is annotated as
superseded.

**Sweep batch bound stays at 100.** Cold start and post-eviction discovery both
scale with fleet count (`min_idle + ceil(active_fleets / 100) × interval`; ≈50 min
at 5 000 fleets). A boot-time reconciliation pass and a raised bound were both
offered and declined:

> Indy (2026-07-25): "I dont like both approaches we will need a scheduler later
> on, so i think go with 100 for now." — context: cold-start / post-eviction
> discovery window; both fixes are discovery scaffolding the future scheduler
> subsumes.

The keyset cursor is **retained** and is not part of that deferral — without it the
fleets past the first batch are never reached at all, not merely reached late.

**Auth verdict cache: invalidate on change, not on a timer.** Indy corrected an
earlier TTL-first proposal. Production **will** run multiple `agentsfleetd`
replicas (so `runner_fleet.md`'s "Prod runs a single agentsfleetd machine today" is
stale — fix it when you build this). Shape: in-process cache of
`sha256(agt_r) → {runner_id, active}` in `agentsfleetd`; `PUBLISH` on admin_state
change (`http/handlers/fleet/runner_patch.zig`) and on runner delete; a subscriber
per replica drops the entry; a long TTL only as a backstop for a `PUBLISH` missed
during a redial (Redis pub/sub has no replay). Never cache the raw token — key on
the hash. Nothing is cached on the runner: `build_runner.zig` links no `pg`/`redis`
and a lint gate enforces it.

**Scheduler boundary.** M141 built the data plane a scheduler needs. Three things
in it are deliberately dumb **placeholders**, not considered policy — do not tune
them, and do not let them calcify: (a) randomized sampling of the ready slice,
(b) `MAX_READY_CANDIDATES_PER_POLL = 64`, (c) the polled repair cadence. Priority
lanes, interrupting a running fleet, capacity-aware placement, and surfacing queue
position are all scheduler scope and explicitly out of M141.

---

## Risks / gotchas

- **The readiness index is ONE global Redis key** (`fleet:ready`) shared by every
  suite in the test binary. A test that `DEL`s it, or that leaves synthetic fleets
  in it, breaks *other* suites invisibly — because `peek` is bounded **and**
  randomized, leftover entries can crowd out the one fleet a sibling just marked.
  That was a real bug in my own suite (fixed in `7633f4741`); the reasoning is
  written at `clearWholeIndex` in `assign_ready_integration_test.zig`. Read it
  before adding a test there.
- **`fleet/sql.zig` is at 339 of the 350-line cap.** Any new statement forces a
  split first. This is why `reclaim_sweeper`'s query stayed inline against the
  SQL-module rule — surfaced deliberately, not hidden.
- **`redis_fleet.zig` was at exactly 350** and had to shed its decoders to
  `redis_fleet_decode.zig` before it could be edited at all.
- **Integration tests are classified by test-NAME prefix**, `test "integration: …"`,
  not by filename. Omit the prefix and your test silently lands in the unit tier,
  where it self-skips without datastores and therefore never runs anywhere. I hit
  this; it cost a commit to fix.
- **zlint bans `undefined` as a struct field default** and flags unused imports —
  both bit me mid-pass.
- **A5 discipline** wants an ownership phrase on owned-slice-returning `pub fn`s.
  For a function returning a borrow into a caller buffer, the annotation is
  `// discipline: ok — returns a borrowed view into `buf``, matching
  `events/activity_channel.zig`.
- **`make test-integration` takes ~12 minutes.** Use `itest.sh` with
  `-Dtest-filter` while iterating, and full runs only to confirm.
- **Do not use bare `git stash`** — the stash stack is shared across worktrees and
  other sessions.
