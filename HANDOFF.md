# HANDOFF — M141_001, bounded runner lease fan-out

**Ephemeral.** Delete at CHORE(close); it briefs the next agent and never ships in the PR.

Written: Jul 26, 2026, ~8:00 PM. Branch `feat/m141-lease-fanout`, **27 commits ahead of
`origin/main` (e1ec00be2), not pushed, no PR.** Rebased onto current main this session —
zero conflicts, version synced at 0.22.1, `make check-version` green.

---

## READ THIS FIRST — the suite is GREEN; what remains is tests-the-spec-demands + close

The 15-failure saga is OVER. Three root causes, all test-side, all fixed and committed:

1. **Index crowding** — the GitHub App ingress suite published events for ~100 fleets
   (each marks `fleet:ready`) and tore down streams only. ~100 leaked marks crowded the
   bounded (64) randomized peek for every seed-shuffled test that ran after it
   (~37% miss per poll at depth ~101). That was the ENTIRE rotating/passes-alone family.
   Fixed: teardown purges via `purgeFleetRedisState` (commit `a012d7e15`).
   Measured live with a 2s `HKEYS` sampler — pre-fix 101 foreign marks resident ~75s;
   post-fix zero. The "mystery janitor" that sometimes hid the leak at run end is
   `assign_ready_integration_test.zig`'s `clearWholeIndex` (DELs the whole key at each
   of its tests' starts — position seed-dependent, hence the wandering counts).
2. **Pre-M141 discovery assumptions** — roundtrip pair + consumer-identity seeded
   Postgres state no poll can discover under ready-first. Rebuilt production-shaped
   (publish → lease → affinity-expiry → reclaim), assertions kept (`faf7eb70d`).
3. **subscription_hub flake** (`expected 10, found 9`) — publish raced the viewer's
   SUBSCRIBE registration; every sibling test settles with `expectNumsub(ch,1)` first,
   this one didn't. Fixed (`a642f5dcf`). Pre-existing (failed in integ3, prior session).

**Run ledger** (full `make test-integration`, per-worktree infra):
integ5 = 2659/16/10 (pre-fix baseline, sampler on) → integ6 = 2668/16/1 (hub flake) →
**integ7 = MAKE_RC=0** (canonical green, pre-rebase tree). Read pass+skip+fail together;
skips steady at 16. integ8 does not exist as evidence — see §What I got wrong.

## The review army ran; its fixes are committed (`565ed027a`)

gstack `/review` (4 specialists + Claude adversarial + Codex adversarial + Codex
structured) is DONE. Everything that survived verification is fixed and committed:
CacheTable.put duplicate-entry, decodePeek canonical-id hardening + peek self-heal,
purge ordering, mark-before-dupe, decode free-before-assign leak, lookup double-mutex,
metrics-doc trim (two documented families didn't exist), stale comments, wall-clock +
rotation-obligation docs. All focused suites + lib lanes green after.

## NOT done — in order for the next session

1. **Seven test additions the review demands (spec ship-gate items).** Shapes are
   settled; implement:
   - NEW `src/agentsfleetd/fleet/assign_ready_faults_integration_test.zig` (register in
     `tests.zig`, add Files-Changed row): (a) Dim 2.2 ceiling — `fleet_ready.mark` 74
     active fleets (`fixtures.seedFleet`, NO sessions needed — they never lease; ids
     `0195c9da-1e2a-7f13-8abc-2b3e1e0f{d:0>4}`), one `pollLease`, assert
     `snap.lease_poll_candidates_scanned_total == MAX_READY_CANDIDATES_PER_POLL` and
     HLEN == 10 after (64 examined-empty get cleared); defer whole-index DEL.
     (b) Dim 1.2 mark-failure — `SET fleet:ready junk` → `publishEvent` returns id +
     `fleet_ready_write_failures_total` delta 1 → DEL key. (c) peek-failure — same
     SET → `pollLease` false (200, lease null), db_roundtrips delta 0 → DEL.
     (d) Dim 4.3 memo — publish (group memoized) → `XGROUP DESTROY` → poll false
     (invalidate) → publish ev2 → poll true. (e) heal — HSET junk field + publish real
     fleet → poll leases it + junk field HDEL'd by peek.
   - `concurrency_lease_test.zig` (129 lines, room): 100-way SINGLE-WINNER through the
     full HTTP lease path (mirror roundtrip's registry wiring + github ingress test's
     100-thread barrier pattern; token `"c" ** 64`; needs tenant/workspace/provider/
     balance/fleet+session/publish; count exactly 1 non-null lease, 100× status 200).
   - `runner_enrollment_integration_test.zig` (318 lines): memo-hit-skips-Postgres —
     authenticated poll (memoized) → `DELETE FROM fleet.runners` row → poll again
     inside TTL → still 200 (a PG read would 401). Deterministic, no counters needed.
   - `assign_ready_integration_test.zig` token test: add ascending-sort assertion
     (sleep 2ms between marks, `std.mem.order(u8, first, second) == .lt`).
2. **Spec row amendments** (reconciliations, not deferrals — say so in Session Notes):
   Dim 1.3 row (concurrent-marks distinctness is unobservable through the public API —
   mark mints internally and the hash keeps only the last write; sequential
   distinct + canonical + ascending is what's provable), Dim 3.6 row (proof is
   two-layer: module-level stale-token no-op test + `clearReadiness` threading the
   peeked token — a poll-level interleave needs a production seam; note the option),
   regression rows (`test_reclaim_and_fencing_unchanged` ≙ the rebuilt roundtrip
   monotonic test; `test_concurrent_runners_single_winner` ≙ claim-layer 100-way +
   the new HTTP 100-way), R8's "two unnamed" note (now: spec + HANDOFF, both at close).
3. **Final gates on the FINAL tree** (the review commit touched production code, so
   everything re-earns): `make test-integration` (Indy's post-review sequencing; run
   once, count-compare vs integ7), `make test-unit-all`, `make memleak`,
   `make harness-verify`, `gitleaks detect --no-banner`, both cross-compiles,
   R10 verbatim (`make test-unit-agentsfleet-lib && zig build test-auth`).
   **Reset the DB first if migrate refuses**: main retired migration 35, so a stale
   local DB triggers `MigrationSchemaAhead` — `make _reset-test-db` fixes it (memleak
   hit this; CI never does because the canonical lane resets).
4. **Rubric re-grade — every row, from the final tree's runs.** R10's Graded cell is
   EMPTY (ship gate blocks on it). Evidence already in hand for R10: both commands
   exit 0 + `runner_token_cache_test.zig` touched only by §6 commits (`d83ac6a85`,
   `29e7ddbf0`), not by the §7 refactor (`0b192aa69`, `8f0c5fc9c`). The env-caveat
   paragraph and Dead-Code-Sweep row are already rewritten for per-worktree infra.
5. **Docs**: changelog `<Update>` exists and is accurate (docs repo `/tmp/docs-m141`,
   branch `chore/m141-lease-fanout-changelog`, commit 4de4347, pushed, NO PR).
   **Gap found: `api-reference/error-codes.mdx` needs the new `UZ-UUIDV7-009` row** —
   the branch added it to the registry; regenerate via `zig build gen-error-codes`
   into the docs worktree and amend/commit on that branch before opening the docs PR.
6. **CHORE(close)**: spec `active/` → `done/` + `Status: DONE`, PR `## Session notes`
   (seed content below), DELETE this file and `itest.sh` (technique preserved in
   Session Notes), orphan sweep, `git status` empty post-commit, then `gh pr create`,
   docs PR, `kishore-babysit-prs`.

## Session Notes seed (paste-and-trim into the PR)

- Root-cause narrative + run ledger: §READ THIS FIRST above, verbatim.
- Review army disposition: fixed-in-`565ed027a` list above, plus this
  **design-risk register** (surfaced, not silently accepted — Indy should eyeball):
  - **Cross-tenant sample flooding (Codex, critical-rated)**: a tenant minting many
    active fleets with unmatchable `required_tags` + one event each dominates the
    64-field random sample; victim pickup probability decays with attacker fleet
    count. Spec's known-limitation entry + Failure-Modes row cover the benign skew
    case; the ADVERSARIAL case is scheduler-scope (per-tenant mark caps / stratified
    peek). Pre-M141 had no starvation (unbounded scan) but O(fleets) cost — this is
    the trade the milestone made, now named.
  - **Redis-brownout PG-conn pinning (Codex)**: poll holds one PG conn across ≤64
    candidates' Redis ops; brownout = 64 serial timeout windows per conn. Strictly
    better than pre-M141 (unbounded), same shape. Follow-up: consecutive-Redis-error
    bailout in the candidate loop.
  - **Wall-clock revocation expiry** (documented in code): NTP step extends a revoked
    runner's window on sibling machines. Monotonic-clock swap is the hardening.
  - **Resume-to-active doesn't re-mark** (spec-deliberate): retained work waits for
    sweeper reach; cheap option if wanted: unconditional mark on resume (false
    positive costs one wasted check).
  - **Probe fails closed silently** (Codex): sweeper's undelivered probe errors →
    `false` + warn, no counter; a metric needs a semantic-registry name (M139).
  - **Sweeper keyset scan unindexed** (perf specialist): `(status, updated_at, id)`
    btree in a future schema slot; pre-existing shape, background cadence.
  - **forgetFleet ×9 DRY** (maintainability): hoist onto TestHarness — follow-up.
  - Declined: EVALSHA for clear (call frequency doesn't justify untested fallback);
    `FLEET_ID_MAX_LEN` dedupe (locality + symmetry with the auth twin).
- **Test-vs-design call made under the no-deferral mandate**: consumer-identity test
  rewritten to a parked-gated-event probe loop (== 1 assertion kept, strictly
  stronger). Recorded in spec Discovery with rationale.
- **/write-unit-test ledger**: session delta was test-files-only pre-review; the
  review commit's production edits are each pinned (cache duplicate-entry test,
  decodePeek skip test) or covered by unchanged suites (lookup reorder → test-auth
  63/63; purge reorder + mark reorder → readiness/ready suites 82+116 green).
  Red-green: the three discovery tests failed on the pre-fix tree in real runs
  (integ4/5 logs) and pass post-fix; the hub guard's red is integ3/integ6.
- **Focused-runner recipe** (itest.sh, deleted at close): ports from
  `docker port agentsfleet-m141-lease-fanout-{postgres,redis,qstash}-1`, then
  `TEST_DATABASE_URL=... TEST_REDIS_TLS_URL=... REDIS_URL_API=...
  REDIS_TLS_CA_CERT_FILE=$PWD/.tmp/redis-ca.crt AGENTSFLEET_QSTASH_LIVE_URL=...
  zig build test -Dtest-filter="..."` — cert refreshed from the container first.
  **Never use it for full-suite evidence** (no LIVE_DB → 68 skips).
- The single highest-value diagnostic remains:
  `docker exec agentsfleet-m141-lease-fanout-redis-1 sh -c 'redis-cli --tls --insecure -a agentsfleet --no-auth-warning HKEYS fleet:ready'` — empty after a clean run.

## What I got wrong this session — so it doesn't cost you

1. **Ran a full-suite question through the per-suite tool** (integ8 via itest.sh):
   no `LIVE_DB=1` → 68 skips (= suites not running), and I rebased MID-COMPILE so it
   built a mixed tree against an unmigrated DB. Discarded. `make test-integration`
   is the only full-run lane.
2. **Chased `MigrationSchemaAhead` as corruption** — it was main retiring migration
   35 (canonical set {1..34,36,37,38}) vs a stale local DB holding 35. Reset fixes.
3. **Treated zig's `failed command:` stderr line as a failure signal** — it prints on
   fully-green runs too (observed on a 116/116 pass). Build Summary + exit code are
   the truth.

## Decisions settled — do not re-litigate (carried + new)

- CI failures fixed in-PR (Indy, verbatim in spec Discovery); auth cache shape +
  10s heartbeat TTL; prod 2–3 machines; RwLock trade; cache_table std-only/liftable;
  UUIDv7 token; sweep bound 100; scheduler knobs are placeholders; ghostty threaded
  sweeper dropped; colima duplicate-stack investigation dropped (per-worktree infra).
- **This session**: metrics doc trimmed to shipped families (no minting names outside
  M139's registry); test-file teardown SQL stays inline (RULE SQLMOD is
  production-scoped — Indy asked, answered, accepted); `fleet_set_cache` stays its
  own struct (refcount+single-flight+versioning don't fit CacheTable; "set" = the
  noun, M133 surface).

## Environment right now

- Worktree `/Users/kishore/Projects/agentsfleet-m141-lease-fanout`, clean except
  untracked `itest.sh`. Containers up (ports 28025/26/27), DB reset+migrated at
  memleak re-run, `HLEN fleet:ready` = 0.
- Docs worktree `/tmp/docs-m141` on `chore/m141-lease-fanout-changelog`, pushed, no PR.
- `agentsfleet-m143-read-surfaces` depends on `src/lib/common/cache_table.zig` from
  this branch — landing unblocks them; the put-priority fix this session is one their
  heap-owned consumer needed.
