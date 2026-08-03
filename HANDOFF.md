# Handoff — M154 schema rebuild + the coverage basis it grew

Ephemeral. Delete at CHORE(close); this briefs the next agent, never the Pull
Request (PR).

## ▶ READ THIS FIRST

**The previous handoff claimed "§7 COMPLETE". It was wrong.** Dimensions 7.3 and
7.4 carry no DONE marker in the spec and their named tests
(`test_reclaim_redelivers_event_without_lease_payload_copy`,
`test_reclaim_tally_stays_in_the_status_flip_statement`) do not exist in the
tree. §7.3's *code* did land — `schema/610_runner_leases.sql:12` documents the
lease carrying no body copy — but the tests the spec names are absent. Treat
7.3/7.4 as **in scope**, not deferred.

**Only 8 of the spec's 31 named tests exist.** They are §7.1, §7.2 (×2) and §8
(×5) — exactly the Dimensions marked DONE, so the spec is self-consistent. But
§1, §2 and §5 ARE substantively tested under *different* names —
`schema_migration_test.zig`'s `"core key schemas: the retired identity twins are
gone and stay gone"` is Dimension 2.1 verbatim, and the two `index_usage_*`
suites cover §5. This is a **table-to-reality reconciliation**, not 23 missing
tests. It still has to happen: that table is what a reviewer reads.

**🚨 STILL NO CONTINUOUS INTEGRATION (CI) HAS EVER RUN ON THIS BRANCH.** Verified
again: `gh run list --branch feat/m154-schema-rebuild` returns only `smoke
(post-deploy)` entries fired by `deployment_status`, all **skipped**. Every real
workflow is `pull_request`-only; none carries `workflow_dispatch`. There is no
manual escape hatch. Indy's decision stands: **no PR until CHORE(close)**.

**🚨 STALE ARTIFACTS ARE THE RECURRING TRAP IN THIS REPO.** It bit three separate
ways this session. Always check an mtime before believing a result:
`stat -f "%Sm %N" -t "%H:%M" zig-out/bin/agentsfleetd-integration-tests`

**Without `make _reset-test-db` + migrate, integration results are garbage.**
Leftover fixture rows produce phantom failures in unrelated suites. Two runs
this session were misread before this was re-learned.

## Status

- ✅ Branch **pushed** through `1d28bd827`; a fifth commit lands after it.
- ✅ Integration **793 passed / 7 skipped / 0 failed**. Runner lane 405. Logging 36.
- ✅ Test depth grew **unit 3344 → 3403, integration 510 → 566**.
- ✅ Merged production coverage **86.15%**, gate at 83.
- 🔶 Three decisions are open and blocking (below). M154's close-out has not started.

## Open decisions — nothing below moves without these

1. **The Continuous Integration workflow edit is BLOCKED.** The harness classifier
   refused two attempts to edit `.github/workflows/test.yml`. The full proposed
   job is written out and reviewed but unapplied. Until it lands, the coverage
   lane gates nothing in CI. Indy must either grant the permission or apply it.
   The change: drop the job-level `container:`, add `timeout-minutes: 30`, boot
   postgres+redis on the host, and run `make test-coverage-zig` inside
   `docker run --network host --security-opt seccomp=unconfined
   --cap-add=SYS_PTRACE` with the socket and workspace mounted. Same image, same
   digest pin — `ci-zig-ubuntu` already carries kcov v43 and docker-cli. This
   mirrors `test-integration.yml`, which documents why a job `container:` cannot
   work here (GitHub forces its own network and rejects `--network host`).

2. **90% is unreachable on the current basis — Indy must pick.** See the
   arithmetic below. Either exclude process-lifetime entry points from the
   denominator (90% then costs +782 lines, which is achievable), or set the gate
   at **88%**, which is the honest ceiling with reachable tests. The denominator
   has already moved twice today; a third change is not an agent call.

3. **The malformed-session-id 500.** `GET /v1/auth/sessions/{id}` with a
   non-UUID identifier answers 500; a well-formed-but-unknown one correctly
   answers 404. `innerPollAuthSession` maps every store error to
   `internalOperationError`, and `formatSessionKey` rejects an oversized id
   before Redis is consulted. Unauthenticated and reachable. Fix here or file?

## What M154 still owes (untouched this session)

1. **Spec DONE markers** — §1–§6 Dimensions and 7.3/7.4 carry none. Verify each
   Dimension's behaviour is tested (under whatever name) before marking.
2. **The Acceptance Rubric is entirely ungraded** — 14 empty cells, and the ship
   gate requires every row graded. Three criteria **fail as written** and need
   their commands fixed rather than the code:
   - **R3** — 14 hits, all *comments* saying the accrual table is gone. Zero live
     references. The grep cannot tell prose from code.
   - **R6** — hits are comments, *negative* tests asserting `grant-approval` does
     NOT match, and `fleet_keyset.zig` (gallery pagination — substring collision).
     Zero references to `core.fleet_keys`.
   - **R5** — 381 files changed; `cli/` (20) is Indy's knowingly-accepted gap, but
     `scripts/` (3) and `samples/` (1) are NOT covered by that decision.
   - **S1** names `make test`, **which is not a target in this repo** and was
     already broken before this branch. Verified against a clean tree.
3. **`docs/architecture/data_flow.md`** is listed in Files Changed as EDIT and was
   never touched. §7's list/detail split is exactly a data-flow change.
4. **Index review** — `make down` first so EXPLAIN reads a cold stack; seed 10
   runners / 100 fleets. First candidate `idx_fleet_events_fleet_id_created_at_id`.
   Also settle `fleet.runner_events`: 4 indexes plus the primary key, and the
   planner picks `idx_runner_events_type_created_at` over the composite
   `index_usage_integration_test` expects.
5. **Skill chain** then CHORE(close). PR Session Notes MUST record that the CLI
   fleet-key surface was deleted without amending the spec's Files Changed table,
   which is therefore knowingly incomplete (Indy's decision).

## Coverage — what was actually wrong, and what is left

**The reported number was never real.** Three independent defects, all fixed in
`9143b13c2`:

- **Test bodies sat in the denominator.** `*_test.zig` is ~23k measured lines at
  ~90% covered, inflating the figure ~7 points and making the gate satisfiable by
  writing more test files.
- **The integration binary was never measured.** Only the five unit binaries ran
  under kcov, so every store and Hypertext Transfer Protocol handler read as
  uncovered — they are unreachable without a live Postgres and Redis.
- **Stale kcov output rejoined every merge.** kcov names its output directory
  after a hash of the binary; `--clean` only resets the directory for the hash it
  is writing, so a rebuilt binary landed *beside* its predecessor and
  `kcov --merge` was handed the parent. An Aug 2 run whose suite never executed
  kept being folded back in for days.

The historical **62.20% was computed over contaminated input**. The honest
starting figure was **84.91%**.

**🚨 Rank targets by REACHABILITY, not by uncovered-line count.** This is the
single most useful thing learned here:

| File | Uncovered before | Gained | Why |
|---|---|---|---|
| `runner/engine/tool_builders.zig` | 99 | **+97** | flat list of reachable builders |
| `http/handlers/auth/sessions.zig` | 132 | **+179** | request paths, spilled into helpers |
| `http/handlers/webhooks/approval.zig` | 94 | **+76** | request paths |
| `lib/logging/mod.zig` | 71 | **+4** | non-test arm of `emit`, process-lifetime |

**The arithmetic for 90%** (denominator 28,741; currently 24,760 covered):

- Mostly-uncovered, non-boot: **398** winnable
- Partially covered, non-boot: 968 uncovered, realistic capture ~45% ≈ **430**
- Boot / process-lifetime: **469 unreachable by construction**
- Needed for 90%: **+1,107**

Realistic yield ≈ 750 → about **88.4%**. Excluding the seven process-lifetime
entry points (`cmd/serve*.zig`, `runner/daemon/startup.zig`, `lease_run.zig`,
`cmd/migrate.zig`, `cmd/backfill.zig`, `pool_migration_state.zig`) drops the
denominator to 28,272 and makes 90% cost +782 — inside budget. That is decision 2.

**Next tier-1 targets:** `runner/engine/runner.zig` (66 unc, 32.7%),
`fleet_runtime/approval_gate_sweeper.zig` (53, 0%), `runner/child_supervisor.zig`
(45, 10%), `fleet/service_activity.zig` (38, 0%), `lib/s3/r2.zig` (34, 0%),
`handlers/library/pipeline.zig` (33, 10.8%), `handlers/approvals/detail.zig` (30, 26.8%).

## Traps worth keeping

- **A webhook route has TWO signature checks and a test must arm BOTH.** The
  `webhookHmac` middleware verifies before the handler and the handler
  re-verifies as defence in depth. Arming only `h.ctx.approval_signing_secret`
  leaves the middleware refusing everything. Set `reg.webhook_hmac_mw.secret` in
  `configureRegistry` too.
- **Assert exact statuses, never `>= 400`.** A broken signing helper made five
  tests pass while never reaching the code they claimed to test — 401 satisfies
  `>= 400`. Only the assertion naming a specific status caught it.
- **`std.fmt.bytesToHex(bytes, .lower)`**, not a `{x}` specifier, to hex a byte
  array in this Zig version.
- **The harness arms secret-gated paths by field assignment**, never `setenv` —
  the 0.16 environment snapshot ignores a late setenv. Documented on
  `test_harness.zig` as the Option-C convention.
- **A make rule's PREREQUISITES expand when the rule is READ**; recipe bodies
  expand at run time. A prerequisite referencing a variable from a later-included
  fragment resolves to nothing, silently.
- **Cancel on a device-login session authorizes on the session's stored Clerk
  subject**, which it acquires only at approve — so a still-pending session
  cannot be cancelled by anyone. Cancel answers **204**, not 200.
- **Never rebuild a Zig line when scripting SQL edits.** Substitute WITHIN a line
  only; a whole-statement rewrite strips `\\` multiline markers.
- **`git commit` runs the full gate suite and exceeds two minutes.** Background it
  and read `git log` — its exit code lies through a pipe. It caught a real design
  flaw twice this session; do not bypass it.
- **The constraint-name sweep expects FOUR benign hits**, not three:
  `ck_test_reclaim_fail`, `ck_test_release_fail`, `uq_workspaces_other`, and
  `uq_workspaces_tenant_id_name`. The fourth is a partial unique INDEX and the
  sweep only queries `pg_constraint`, which does not carry bare indexes.
- **Two audit tools, use them — do not grep.** `scripts/audit_sql.py --all` and
  the constraint-name sweep found four production bugs grep could not.
- **Docker.** Compose project `agentsfleet-m154-schema-rebuild`, ports
  25832/25833/25834, database `agentsfleetdb`. Always `docker ps` first.
- ❌ Never run `playbooks/operations/teardown/database/02_teardown.sh` — Indy calls
  it manually. `make _reset-test-db` (teardown.sql) is fine.

## Decisions Indy made — do not relitigate

1. **No Pull Request until CHORE(close)**, accepting that CI arrives last.
2. **CLI fleet-key surface deleted, Files Changed NOT amended** — record in PR
   Session Notes at CHORE(close).
3. **The events table's prose cell reads `No result recorded`.**
4. **The transcript re-reads its turns as details**, rather than degrading.
5. **Dimension 8.1's wording amended** rather than adding a bundle reason field.
6. **Every fleet-key mention deleted from `docs/AUTH.md`**, including the v2.1
   first-class-principal roadmap item, and from `docs/architecture/roadmap.md`.
