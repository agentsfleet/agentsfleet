# HANDOFF — M141_001, bounded runner lease fan-out (close-out session 2)

**Ephemeral.** Delete at CHORE(close); it briefs the next agent and never ships in the PR.

Written: Jul 26, 2026, ~9:05 PM. Branch `feat/m141-lease-fanout`, **30 commits ahead of
`origin/main` (e1ec00be2), not pushed, no PR.** Working tree clean except untracked
`itest.sh`. Docs worktree `/tmp/docs-m141` on `chore/m141-lease-fanout-changelog`,
2 commits, PUSHED, no PR.

---

## READ THIS FIRST — everything is implemented; what remains is gates → rubric → close

The prior session's mandate is complete through item 5 of 6:

1. ✅ **Seven review-demanded test additions** — committed `052fbe570`, each run green
   via the focused runner. New `fleet/assign_ready_faults_integration_test.zig`
   (ceiling / mark-failure / peek-failure / memo-invalidate / heal — now also the
   gate auto-kill pause test, see item 3), registered in `tests.zig`; 100-way HTTP
   single-winner in `concurrency_lease_test.zig`; memo-hit-skips-Postgres in
   `runner_enrollment_integration_test.zig`; token ascending-sort in
   `assign_ready_integration_test.zig`.
2. ✅ **Spec row reconciliations** — same commit (Dim 1.3, 3.6, 1.2 tier, 2.2
   enrichment, regression-row equivalences, R8 note, Files-Changed rows).
3. ✅ **Two recovered Codex findings FIXED** — committed `56bf3d15c` on Indy's
   explicit in-session ruling (he chose fix-in-PR for BOTH when offered
   fix/register/follow-up): the raw Codex log (session scratchpad
   `6b9c028f-…/scratchpad/codex_review.log`, tail) held 5 findings; 3 were in the
   fix commit `565ed027a`, 2 had been dropped from the digest:
   - `cron/FireQueue.zig` (the SECOND stream producer — atomic dedup Lua) never
     marked `fleet:ready` → scheduled fires waited out sweeper reach. Now marks
     after a successful append; duplicate arm proven to mark nothing
     (`fire_queue_integration_test.zig`, whose fleet ids also became minted
     UUIDv7s + teardown drops marks — they'd otherwise leak junk into the shared
     index, the crowding class from the 15-failure saga).
   - `fleet/approval_gate.pauseFleet` bypassed the PATCH-site readiness clear.
     Now takes the redis client (both call sites had it in scope) and
     force-clears after the pause UPDATE commits; skipped on UPDATE failure.
     Proven end to end in the faults suite (paused + mark cleared + no lease).
   - Spec: Files-Changed rows for all three files, §1 one-producer premise
     corrected, Dim 1.1/3.7 rows updated, Discovery entry records the ruling.
4. ✅ **Docs** — the HANDOFF-claimed `UZ-UUIDV7-009` gap was FALSE (row shipped
   Jul 12, docs `ee22c03`); verified, no page change needed. Changelog `<Update>`
   amended instead (scheduled-fires bullet + auto-pause added to the
   stop/delete bullet), committed `9a1e6f0`, pushed. Discovery for later
   (recorded in Session Notes): `zig build gen-error-codes` raw output has
   drifted from the hand-curated page and fails the docs repo's version pin
   (`EXPECTED_VERSION 0.17.0` in `scripts/check-documentation.py`) and DOC-05
   banned words coming from registry prose — docs-tooling reconciliation, NOT
   owned by M141.
5. ✅ **Cheap rubric rows pre-graded on the final tree** (see §Rubric below).
6. ⏳ **Final gate battery** — RUNNING in background at handoff time. See next.

## The gate battery — check it FIRST

Script + logs (absolute paths, previous session's scratchpad — readable across sessions):

- Script: `/private/tmp/claude-501/-Users-kishore-Projects-agentsfleet/b9732de8-61f3-4ea0-9f77-e2f2a7b79376/scratchpad/final_gates.sh`
- Results ledger: same dir `…/scratchpad/gates/RESULTS.txt` (one `<lane> RC=<n>` line per lane; ends with `ALL DONE`)
- Per-lane logs: `…/scratchpad/gates/<lane>.log`

Lanes, in order: `test-integration`, `test-unit-all`, `memleak`, `harness-verify`,
`gitleaks`, `xcompile-x86`, `xcompile-aarch64`, `r10-lib` (`make
test-unit-agentsfleet-lib`), `r10-auth` (`zig build test-auth`).

- If `RESULTS.txt` ends `ALL DONE` with every `RC=0` → grade the rubric and close.
- If the battery died mid-run (session end can kill it): re-run
  `bash <script>` — it restarts from the top; or run remaining lanes by hand
  (they're plain `make`/`zig build` commands, see the script).
- If `migrate` refuses with `MigrationSchemaAhead`: `make _reset-test-db` first
  (main retired migration 35; stale local DBs trip it).
- Compare integ counts vs the ledger: integ7 (pre-additions) = 2668 pass / 16 skip.
  The final tree adds 9 integration tests (7 additions incl. the split heal/kill
  cases + 2 cron assertions ride existing tests) — expect pass ≈ 2677±, skips
  pinned at 16. Read pass+skip+fail TOGETHER.

## Rubric state (spec `docs/v2/active/M141_….md` §Acceptance Rubric)

Pre-graded this session on the FINAL tree (evidence in hand, cells not yet edited):

- **R7** ✅ `runner_fleet.md` +30/−2, `scaling.md` +29/−4 (both non-empty).
- **R8** ✅ 69 changed paths; only unnamed = the spec itself (pending/active pair)
  and `HANDOFF.md`, both handled at close. (Verified by scripted diff-vs-table compare.)
- **S6** ✅ `make _zig_line_limit_check` → "All new Zig files within 350-line limit".
- **R10 provenance** (the empty cell that blocks the ship gate): §7 refactor commits
  `0b192aa69`/`8f0c5fc9c` carry ZERO diff on `runner_token_cache_test.zig`.
  IMPORTANT correction to the prior handoff's evidence: `565ed027a` DID touch that
  file — but comment-only (3+/2−, the stale direct-mapped-era comment reworded;
  assertions byte-identical), which still honors Dim 7.2. Grade R10 from
  `r10-lib` + `r10-auth` RC=0 plus exactly this provenance statement.
- Everything else (R1, R3, R4, R5, R6, R9, S1, S3, S4, S5): grade from the
  battery's lane logs — decisive line + RC per row. EVERY row re-earns from the
  final tree; don't carry the old cells' run ids forward.

## CHORE(close) — the remaining mechanics, in order

1. Fill every rubric Graded cell (above), `Status: DONE`, move spec
   `docs/v2/active/` → `docs/v2/done/` (git mv).
2. PR `## Session notes`: the COMPLETE drafted seed is at
   `/private/tmp/claude-501/-Users-kishore-Projects-agentsfleet/b9732de8-61f3-4ea0-9f77-e2f2a7b79376/scratchpad/session_notes.md`
   — one `<!-- FILL FROM gates/RESULTS.txt -->` marker awaits the battery
   numbers. Keep the design-risk register intact (Indy reads it in the PR;
   cross-tenant sample flooding stays at the top).
3. DELETE `HANDOFF.md` (this file) and `itest.sh` (recipe preserved in the notes).
4. `git status -uall` empty post-commit; `make check-version` (VERSION untouched
   this session — should pass trivially).
5. `gh pr create` (title per spec: "feat(runner): bound the lease-poll fan-out to
   ready fleets"), body = intent + Session Notes.
6. Docs PR: `/tmp/docs-m141`, branch `chore/m141-lease-fanout-changelog` (2
   commits, pushed) — `gh pr create` there too.
7. `kishore-babysit-prs` on both.

## Traps (carried + new this session)

- **itest.sh for full runs** — 68 skips = suites not running. Focused only.
- **Focused-run pass counts lie**: the summary counts container `test {}` blocks
  (baseline 47 with a no-match filter; a fleet-file filter reads "69"). Judge by
  RC + count DELTA, not the absolute. Live side-effect sampling
  (`HLEN fleet:ready` during the ceiling test) is the positive-evidence trick.
- **zig's `failed command:` line prints on GREEN runs** — Build Summary + exit
  code are the truth.
- **`-Dtest-filter` accepts ONE value** — repeat invocations, not repeated flags.
- **Warn-level logs are suppressed by the test runner** — absence of `log.warn`
  lines in a green run's output proves nothing.
- **`readiness_lifecycle_integration_test.zig:1` carries a pre-existing
  milestone-ID header comment** (`M141 §3, Dimension 3.7`) — if you edit that
  file for any reason, the MSID gate makes you strip it in the same diff.
  Untouched this session on purpose (no other reason to open it).

## Decisions settled — do not re-litigate (carried + new)

- All prior-session items (metrics-doc trim, inline test SQL, `fleet_set_cache`
  its own struct, sweep bound 100, cold-start deferral with Indy's ack quote in
  spec Discovery).
- **This session (Indy, in-session, via structured ask):** cron P1 → fix in this
  PR; pause P2 → fix in this PR. Both landed (`56bf3d15c`); neither is a deferral.
- Docs `error-codes.mdx`: NO regeneration — page is hand-curated under the docs
  repo's 0.17.0 version pin; the generator's raw output must not replace it.

## Environment right now

- Worktree `/Users/kishore/Projects/agentsfleet-m141-lease-fanout`, clean except
  untracked `itest.sh`. Containers up (per-worktree compose), DB migrated.
- Gate battery possibly still running (started 20:51) — check `RESULTS.txt`.
- Raw Codex review: `…/6b9c028f-e46f-4c81-9faf-6d39ff651a08/scratchpad/codex_review.log`
  (verdict at tail; now fully dispositioned).
- `agentsfleet-m143-read-surfaces` depends on `src/lib/common/cache_table.zig`
  from this branch — landing unblocks them.
