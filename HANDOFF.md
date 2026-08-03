# Handoff — M154 schema rebuild

Ephemeral. Delete at CHORE(close); this briefs the next agent, never the Pull
Request (PR).

## ▶ READ THIS FIRST

**Everything is committed and the working tree is clean.** `HEAD` is
`adf52ba04`, 51 commits ahead of `origin/main`. The previous handoff opened by
warning that 26 files sat uncommitted — that is resolved; eleven focused commits
landed this session. Start by confirming `git status -uall` is empty, then work
forward.

**The branch already contains `origin/main` (`31ce4b9c9`)**, including the PR
#586 playbook rebuild. No rebase needed before the PR.

**Still no PR, and CHORE(close) has not started.**

## Gate board

| Row | Verdict |
|---|---|
| R1–R6, S1–S8 | ✅ all fourteen graded verbatim in the spec, from their own output |
| `make test-integration` | ✅ exit 0, zero test failures (re-run this session after the new tests) |
| `make check-playbooks` | ✅ exit 0 |
| Test depth | unit=3415 integration=576, from the CHORE(open) baseline 3344 / 510 |

**S1/S2 were re-run because the previous handoff's greens were partly stale** —
see "Traps" below. Treat the rubric's S-rows as needing one more confirming run
at CHORE(close), because the diff has grown since they were graded.

## What is left — in priority order

1. **Three runtime bugs Indy raised, to be fixed in this PR** (he asked
   explicitly: *"Can we fix them all in your PR?"*). All three are **blocked on
   information only he can supply** — the questions are at the bottom of this
   file.
   - **Billing page** returns "Billing isn't ready yet". Root cause is
     server-side and invisible from here, because
     `app/(dashboard)/settings/billing/page.tsx:58` does
     `getTenantBilling(token).catch(() => null)` and `null` renders that empty
     state. **A 500, a network failure and a genuinely empty tenant are
     indistinguishable to the operator.** That collapse is a real defect on its
     own and should be fixed regardless of the cause — but the cause needs the
     actual API status.
   - **`github-pr-reviewer` fleet** reports "The runner crashed —
     FleetRunFailed", 0 tokens, 5.1s, on PR #586. `FleetRunFailed` is the
     outcome label, not the reason; the reason is behind the `{} Details`
     expander in that row.
   - **Runners leases filter** — no way to filter by workspace then fleet; a
     funnel icon per row beside a truncated workspace id. This is a **design
     change, not a bug**, and needs a target shape before any code.
   - **Hypothesis worth testing before writing code:** Indy plans to teardown
     dev and rebuild the api-dev database after this merges. If that database
     has already drifted from the deployed code, a failing billing read and a
     runner dying in 5.1s with 0 tokens are both what a schema mismatch looks
     like — and the rebuild is the fix, not a patch. This milestone renames
     `core.tenant_billing` → `billing.tenant_wallet` and
     `core.fleet_execution_telemetry` → `billing.usage_ledger`, so the shape
     fits. **Establish this before treating either as a code bug.**

2. **Skill chain — one of three done.**
   - `/write-unit-test` ✅ ran; it produced the six tests described below.
   - `/write-integration-test` ❌ never run.
   - gstack `/review` ❌ never run. Both are mandatory before the PR.

3. **Index review (§5) never happened.** `make down` first so EXPLAIN reads a
   cold stack; seed 10 runners / 100 fleets. First candidate
   `idx_fleet_events_fleet_id_created_at_id`. Also settle `fleet.runner_events`:
   4 indexes plus the primary key, and the planner picks
   `idx_runner_events_type_created_at` over the composite
   `index_usage_integration_test` expects.

4. **Dimension 6.1 is the one Dimension still untested**, and the spec now
   records why in full. Short version: the Dimension asserts a *comment
   convention* the catalogue cannot see, so the honest test is a different
   assertion (literal-vs-constant drift, like `audits/cross-tier-rates.sh`) and
   that substitution is Indy's call. **Do not mark it DONE.**

5. **CHORE(close)** — spec `active/` → `done/`, delete this file, PR Session
   Notes, orphan sweep, `git status` empty.

6. **PR**, then `kishore-babysit-prs`.

## What this session changed

**All 26 uncommitted files landed**, in eleven commits scoped by concern: the
coverage-lane workflow and its guard, the auth session fixes, the OpenAPI route
removal, the catalogue tests, the app coverage tests, the stale source comments,
the architecture docs, the spec grading, the billing-doc money-path corrections,
the five Dimension tests, and the playbook role check.

**Five of the six untested Dimensions now have tests — 2.1, 3.1, 4.2, 4.3, 7.4**
— and the spec is marked accordingly. **None was accepted on a green exit.**
Every one was mutated first and watched go red:

- `ledgered + 1` → `expected 35921, found 35920`
- ledger foreign-key count 3 → 4 → `expected 4, found 3`
- expiry tally `+ 1` → `+ 2` → `expected 2, found 1`

The two worth knowing about. **4.2** pins that the wallet drain equals the ledger
sum; the sibling test that looked like it already covered this reconciles the
emitted *metric* against the ledger, and would still pass if the ledger and the
wallet disagreed with each other. **7.4** deletes the event body so reclaim's
INNER join returns nothing — the only case where a tally written by a second
statement behaves differently from one riding the status flip.

**The Continuous Integration guard was blocking every commit.**
`check_ci_lane_config_test.py` asserted `assertTrue(options, "the coverage job
must still declare container options")` — it required the coverage job to have a
`container:` block, which is an implementation choice, not a property, and which
Indy's approved workflow edit deliberately changed. Deleted. The two assertions
that *do* encode properties now read `docker run` flags as well as
`container.options:`, which also closed a blind spot `memleak.yml:88` had sat in
since the guard was written: a `docker run --privileged` was invisible to the
sweep. Indy approved this edit in session.

**The billing architecture doc described a deleted table as live in nine
places**, not the seven the previous handoff counted. The two extra were
`slice_seq` (the `meter_slice_seq` counter is gone from the affinity slot) and
the budget gate's rolling-window filter (`recorded_at` → `last_charged_at`).
`renewal.zig`'s own header had the same defect.

**`playbooks/.../teardown/database/03_verify.sh` verified four of eight roles.**
Its hardcoded list named `ops_readonly_agent` — retired with the old product
noun, creatable by nothing — while missing `billing_runtime`,
`metering_runtime`, `vault_runtime` and `ops_readonly_fleet`, and printed a
success criterion of "5-6". The two it silently stopped reporting own the money
tables. Now catalog-derived, the way `teardown.sql`'s schema loop already is.

**The rest of the teardown → rebuild path was checked and is sound for this
milestone**, which matters because landing it forces a from-empty bootstrap:
`teardown.sql` enumerates schemas from `information_schema` so `billing` and
`audit` drop without being named; the migration ledger is
`audit.schema_migrations` and goes with them, so the next boot replays from
zero; role creation is guarded on `pg_roles` and schemas are `IF NOT EXISTS`, so
a second bootstrap over surviving roles is idempotent; and no file under
`playbooks/` names a table this milestone renamed.

## Traps this session re-learned the hard way

- **`zig build test-integration` run directly SKIPS the database-and-Redis tests
  SILENTLY and reports a green `68/69`.** A deliberately broken assertion still
  passed there. Only `make test-integration` provisions the suite, and only
  under it does the mutation go red. **Any claim about an integration test must
  come from the make target.** This is the single most expensive trap in this
  repository and it has now bitten twice.
- **A filter that matches nothing passes — and so does one that matches a
  skipped test.** Counting is not enough; mutate the assertion and watch it go
  red. That is the only proof.
- **The previous handoff's S2 `make lint-all ✅` was stale.** `lint-all` →
  `lint-zig` → the Continuous Integration guard test, which was red the whole
  time. What had actually been verified after the workflow edit was
  `make check-gh-actions-valid`, a much narrower target (actionlint only).
  Re-grade S-rows after the diff grows; do not carry a green forward.
- **The pre-commit hook takes longer than two minutes** on Zig-touching commits
  and is scope-aware — doc-only commits skip the heavy lanes entirely. Budget
  for it; a two-minute timeout kills the commit mid-hook.
- Everything the previous handoff listed still holds: a background task's exit
  code is the wrapper's, not `make`'s; a degraded coverage run reports a
  confident wrong number, so check file counts not just percentages;
  `public/openapi.json` is generated, edit `public/openapi/**`;
  `make test-unit-all` reaches the live internet via `http_pin_test`.

## Decisions Indy made this session — do not relitigate

1. **The Continuous Integration guard fix is approved** — he reviewed the
   analysis and said *"yes continue"*.
2. **Cache-prune cleanup is deferred, explicitly.** *"let fix the cache-prune
   later."* The analysis stands for whoever picks it up: the `closed-pr` rule
   fixes a real Least Recently Used inversion; the `superseded` rule is largely
   redundant with LRU and the 7-day sweep; the four workflow-text assertions are
   the most brittle part. **Out of M154's scope — its own spec, not this PR.**
3. **The three runtime bugs are to be fixed in this PR** — *"Can we fix them all
   in your PR?"*
4. Carried from before: **no `changelog.mdx` entry** (asked twice, refused
   twice, both quotes in the spec's Discovery); **no PR until CHORE(close)**;
   the `.github/workflows/` edit is approved; horizontal sharding is "measure,
   then decide"; `M155_001` exists so three deferrals stop pointing at nothing.

## Open, needing Indy — all three block work

1. **The billing API error.** `curl -H "Authorization: Bearer <token>"
   https://<api-dev>/v1/tenants/me/billing`, or the `agentsfleetd` log line. The
   status code alone narrows it a long way.
2. **The runner crash detail** — the contents of the `{} Details` expander on
   that lease row, or the runner log.
3. **The Runners filter's target shape** — a workspace picker that scopes a
   fleet picker in the toolbar? Free-text search? Click a row's workspace to
   filter to it?

Carried, still open:

- **`http_pin_test` makes a P0 gate depend on the developer's network.** Options:
  accept S1 as Continuous Integration-only, guard the sweep with
  `error.SkipZigTest` on an unreachable-network errno, or move it to a lane
  allowed to need the internet.
- **`ERR_APIKEY_INVALID` is declared with no live consumer** — dead registry
  entry; removal needs an orphan sweep.
- **`dashboard-workspace`'s WorkspaceSwitcher test** exceeds the default 1s
  `waitFor` under parallel load. Passes 28/28 alone. Makes S1 non-deterministic.
- **`schema/710_usage_ledger.sql:2`** says "At most three rows per event — one
  per charge type"; `ChargeType` has two members (`receive`, `stage`). Harmless
  today, wrong as written. Noticed while grounding the billing doc; not fixed
  because it is a comment on a file outside the session's touched set.

## Environment

Docker compose project `agentsfleet-m154-schema-rebuild`, ports
25832/25833/25834, database `agentsfleetdb`. Redis TLS cert at
`.tmp/redis-ca.crt` — `make` provisions it, a direct `zig build` does not, which
is the skip trap above. Always `docker ps` first. ❌ Never run
`playbooks/operations/teardown/database/02_teardown.sh`; `make _reset-test-db`
is fine.
