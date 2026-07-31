# Handoff — M148_001 assigned runner policy (Jul 31, 2026, 09:10 AM)

Ephemeral. The spec is canonical: `docs/v2/done/M148_001_P1_API_INFRA_UI_CONTROL_PLANE_ASSIGNED_RUNNER_POLICY.md`
(already in `done/` — CHORE(close) ran; this is the post-PR babysit arm).
Delete this file before the PR merges.

## Scope/Status

**PR #581 is OPEN** — https://github.com/agentsfleet/agentsfleet/pull/581
(`feat(api,runner,app): the assigned isolation is the applied isolation`).
Docs changelog PR is open too: https://github.com/agentsfleet/docs/pull/167
(worktree `~/Projects/docs-m148`, branch `chore/m148-assigned-policy-changelog`).

The whole M148 arc shipped: policy assigned by the control plane and delivered
with the runner's identity; the runner probes kernel capability and reports it;
the two reconcile into a degraded verdict that gates leases fail-closed on both
sides; the env surface collapsed to the bootstrap pair; `macos_seatbelt` gone.
Everything is committed and pushed. Remaining work = babysit only.

- ✅ Full VERIFY chain, CHORE(close), both PRs opened.
- ✅ CI round 1 (SHA `6074db95a`): 9 failures, ALL diagnosed + fixed in
  `c99f8d3c6` — 8 shared one root cause (`std.os.linux.E.init` does not exist
  in this Zig; the line is Linux-gated so macOS-native lanes never analysed it,
  and my cross-compiles covered the daemon graph but NOT `build_runner.zig`),
  the 9th was `bundle-size-app` (the Edit-policy dialog put react-hook-form +
  zod on the runner detail route's critical path, +33 kB over its 102.4 kB
  budget → now a dynamic island like the sibling add-runner dialog, 44.95 kB).
- ⏳ **UNCOMMITTED, UNPUSHED**: greptile P1 #1's fix (see below). Local lint +
  agentsfleetd units pass; `make test-integration` was still running in the
  background when this was written (log:
  `<session-scratchpad>/itest_patch.log`, task `bvj5piv77`).

## Greptile triage state (PR #581, review id 4825008900)

Two P1 line comments. Neither has been REPLIED to yet — replies are owed via
`greptile-triage.md`'s Tier 1 templates, with the fix SHA.

1. **`runner_patch.zig:151` — "Stale verdict leaves leasing enabled"**:
   VALID & ACTIONABLE, **fix written, not yet committed**. My `reconcileNow`
   wrote the verdict in a separate best-effort statement, so a failed verdict
   write left a tightened assignment beside a stale healthy verdict and the
   lease gate would issue work. Fixed by making it ATOMIC: the caller
   reconciles against the stored capability report BEFORE the write and the
   verdict rides the SAME `UPDATE` (`PATCH_RUNNER_ASSIGNED_POLICY` gained
   `$13`/`$14`); `reconcileNow` and the now-unused `runner_sql` import are
   gone. A capability read failure yields a null report → reconciles degraded
   (fail-closed), never assumed-healthy.
2. **`loop.zig:223` — "Worker-count increases remain unapplied"**: VALID BUT
   INTENTIONAL + already disclosed. Spec assumption (6): worker-count changes
   bind at the next loop tick and the pool is spawned once; the Codex review
   raised the same point and the Edit-policy dialog copy now states it
   ("growing the worker count past what the daemon started with takes effect
   after a runner restart"). Reply with that evidence; do NOT build live pool
   growth — that is a separate workstream, not M148 scope.

## Working tree

Branch `feat/m148-assigned-runner-policy` @ `c99f8d3c6` pushed;
worktree `~/Projects/agentsfleet-m148-assigned-runner-policy`.
Uncommitted: `src/agentsfleetd/http/handlers/fleet/{runner_patch,sql}.zig`
(the atomic-verdict fix above) + this handoff file.

## Next steps (in order)

1. Confirm the background `make test-integration` (task `bvj5piv77`) exit 0.
   If red, fix before pushing — the PATCH statement changed shape.
2. Commit the atomic-verdict fix; push.
3. Reply to BOTH greptile comments per `greptile-triage.md` Tier 1 templates
   (fix SHA for #1, the intentional-and-disclosed evidence for #2), then write
   the history lines to BOTH `$HOME/.gstack/projects/<slug>/greptile-history.md`
   and `$HOME/.gstack/greptile-history.md`
   (`fix|already-fixed` + `correctness`/`other`).
4. Re-poll CI on PR #581 (`gh pr checks 581`) — expect the previously failing
   9 to go green on `c99f8d3c6`+. Fix anything caused by our diff; surface
   anything pre-existing to Indy rather than fixing blind.
5. Keep polling per the cadence table until **two consecutive empty polls AND
   ci=green** on #581, and #167 green too. Then post the final BABYSIT REPORT
   and add it to the PR's Session Notes.
6. Delete this handoff file before merge.

## Risks/gotchas

- **Cross-compile BOTH graphs.** `zig build -Dtarget=…` only covers the daemon;
  the runner needs `zig build --build-file build_runner.zig -Dtarget=…`. This
  is exactly how 8 CI lanes died. AGENTS.md says it; I missed it.
- **Linux-gated code is invisible on macOS.** Anything inside
  `if (builtin.os.tag != .linux) return …` is comptime-dead locally. Prefer
  version-independent idioms (the signed syscall return) over std helper
  spellings that move between Zig releases.
- **New client-heavy dialogs need the island pattern** —
  `components/domain/island-dynamic/*Dynamic.tsx` — or they blow a route
  budget. Tests then mock the island, not the component (see
  `RunnersView.test.tsx` and the updated `RunnerHeader.test.tsx`).
- **`bun install` after a rebase.** `origin/main` moved the app toolchain to
  TypeScript 7; the app lane fails to resolve `typescript-jsapi` until you run
  it. That was the pre-push hook's one red.
- **Pipelines eat exit codes** — `make … | tail` reports tail's rc. Use
  `${pipestatus[1]}` or redirect + `echo $?`. Cost three false-greens.
- **`TEST_FILTER` matching nothing exits 0** (false pass). Graded rows use the
  unfiltered lane.
- Indy-acked, in the spec's Discovery: EgressScope enforcement is a separate
  workstream; the heartbeat-vs-PATCH `policy_version` guard is a follow-up
  ("i can live with the race"); 4.1/4.2's e2e tier has no target in this repo.
  Everything else is in-scope — treat a gap as a defect, not a deferral.

## /pickup prompt for the next agent

> /pickup — resume the M148 babysit arm in
> `~/Projects/agentsfleet-m148-assigned-runner-policy` (branch
> `feat/m148-assigned-runner-policy`, PR
> https://github.com/agentsfleet/agentsfleet/pull/581 open; docs PR
> https://github.com/agentsfleet/docs/pull/167 open). Read
> `HANDOFF_Jul_31_09_10.md` at the worktree root first — the spec is already
> in `docs/v2/done/` and all implementation is done; this is `kishore-babysit-prs`
> work only. Do, in order: (1) confirm the backgrounded `make test-integration`
> passed (re-run it if the session lost the task) — it covers an uncommitted
> atomic-verdict fix to `runner_patch.zig` + `fleet/sql.zig`; (2) commit and
> push that fix; (3) reply to BOTH greptile P1 comments on #581 per
> `greptile-triage.md` Tier 1 templates — #1 is fixed (cite the SHA), #2
> (worker-count growth needs a restart) is intentional, spec-documented, and
> already disclosed in the Edit-policy dialog copy, so reply with that evidence
> and do NOT implement live pool growth; write the history lines to both
> greptile-history files; (4) re-poll CI until two consecutive empty polls with
> ci=green on both PRs, fixing only failures our diff caused; (5) post the final
> BABYSIT REPORT into the PR's Session Notes and delete the handoff file.
> Do not re-litigate closed decisions — every Indy quote is in the spec's
> Discovery.
