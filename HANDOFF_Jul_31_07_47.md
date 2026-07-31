# Handoff — M148_001 assigned runner policy (Jul 31, 2026, 07:47 AM)

Ephemeral — briefs the next agent; delete before the Pull Request (PR) opens.
The spec is the canonical state: `docs/v2/done/M148_001_P1_API_INFRA_UI_CONTROL_PLANE_ASSIGNED_RUNNER_POLICY.md`
(yes, `done/` — CHORE(close) has run; only push→PR→babysit remain).

## Scope/Status

The full M148 arc is CODE-COMPLETE, REVIEW-HARDENED, and GRADED. Every spec
Dimension is ✅ DONE; every rubric row is graded green (S4 = documented skip:
no `make test-e2e` target exists in the repo).

- ✅ §1–§3 (prior session) + the batched integration suite (8 spec-named
  tests: register → heartbeat → PATCH → degraded → lease-gate-by-contrast →
  recovery, plus the pre-migration-row unbrick arc and the lenient-parse beat).
  Red-green proven (an inverted assertion failed the full lane).
- ✅ §4 server (shared `fleet/runner_row.zig` decode; list+detail carry
  assigned/achievable/degraded) + dashboard (PolicyFields four-field form,
  Edit-policy header action, degraded badge/reason on tile+header).
- ✅ §5 env teardown (playbooks, unit file, deploy.sh, CLI help, R5 sweep as a
  named test — zero matches, no carve-outs) + §6 seatbelt removal (R7 zero).
- ✅ REVIEW chain (unit-test audit → integration audit → gstack /review with
  Claude adversarial + security + testing specialists + Codex): 1 P0 fixed
  (heartbeat reply serialized freed row memory — decoders now `alloc_always`)
  + 12 findings fixed; all dispositions in spec Discovery + the PR body draft.
- ✅ CHORE(close) committed (`2faeaed6f`): both specs in `done/` (M147_001
  moved per Indy), workflow comment line removed per Indy's explicit approval.
- ⏳ IN FLIGHT: `orly gate` running the slow verify suites in background
  (started ~06:55; log buffered → `scratchpad/orlygate.log` under the session
  scratchpad, task id `bfd046ccm`). Its verdict gates the push.

## Working tree

Branch `feat/m148-assigned-runner-policy`, worktree
`~/Projects/agentsfleet-m148-assigned-runner-policy`. **20 local commits, none
pushed; no PR yet.** One uncommitted file: the spec in `done/` carrying
Indy's race-ack quote ("i can live with the race") — commit it as
`docs(m148): record the race-acceptance quote` before or during the push flow.
Docs repo: changelog `<Update>` COMMITTED AND PUSHED on
`chore/m148-assigned-policy-changelog` (worktree `~/Projects/docs-m148`);
its PR is not yet opened — open it alongside the main PR.

## Tests/checks run (all green, evidence in spec rubric + PR body draft)

- `make test-unit-all` exit 0 (all lanes + all package coverage gates; the app
  package's 100% thresholds close over the new surfaces).
- `make test-integration` from clean reset exit 0 (final run post-review-fixes).
- `make lint-all` ✅ · `make memleak` ✅ (incl. boot→drain lifecycle) ·
  cross-compiles ✅ (linux test graph's "unable to execute binaries" = PASS) ·
  `gitleaks` ✅ · `make check-openapi` ✅ · `make check-version` ✅ (0.26.0).
- Test Delta: unit 3266→3296, integration 501→509.

## Next steps (in order)

1. Wait for / check `orly gate` (task `bfd046ccm`). Green → proceed. Red →
   fix the named criterion first (its snapshot predates the uncommitted spec
   quote — docs-only, cannot flip quality gates).
2. Commit the spec-quote file.
3. `git fetch origin && git rebase origin/main` — branch is ~8 behind; m150
   touched the runner activity flush, so expect possible conflicts in
   `src/runner/daemon/loop.zig` / `control_plane_client.zig`. After rebase:
   `make test-unit-agentsfleetd && make test-unit-agentsfleet-runner` minimum;
   full `make test-integration` if the rebase touched anything non-trivial.
4. `git push origin feat/m148-assigned-runner-policy` (non-force).
5. `gh pr create` — title `feat(api,runner,app): the assigned isolation is the
   applied isolation`; body is READY at the session scratchpad
   `pr_body.md` (includes `## Session notes` with all required outputs).
6. Open the docs-repo PR for `chore/m148-assigned-policy-changelog`.
7. Delete THIS handoff file (and commit the deletion) before/with the PR.
8. Run `kishore-babysit-prs` (all three surfaces: CI runs, greptile inline,
   PR-level summary; stop on two consecutive empty polls with CI green).

## Risks/gotchas (hard-won this session)

- **Pipelines eat exit codes.** `make … | tail` reports tail's rc — three
  false-greens happened this way. Capture rc via `${pipestatus[1]}` (zsh) or
  redirect to a file and `echo $?`.
- **`TEST_FILTER` false-passes**: a filter matching nothing exits 0 (the mk
  warns; it bit anyway). Graded integration rows use the UNFILTERED lane only.
- **"failed command: …integration-tests" + warnings `ignored: PG` in the full
  integration output are benign** (a child inside a negative test); the pass
  criteria are make rc=0 + the two ✓ echo lines.
- **`make test` does not exist** — tier 1 is `make test-unit-all`
  (`docs/VERIFY_TIERS.md`); the spec's S1 row records this instance note.
- The pre-commit hook runs harness-verify + scoped lints; UFS flags ANY
  literal repeated 2× per file (incl. import paths — `_ = runner_row;` in a
  `test {}` block dodges the double-`@import` hit).
- The app coverage gate enforces 100% on statements/branches/functions/lines —
  any new TSX branch needs a test in the same commit.
- Indy-acked deferrals (quotes in spec Discovery): EgressScope enforcement
  (separate workstream), the heartbeat-vs-PATCH `policy_version` guard
  ("i can live with the race"), 4.1/4.2 e2e tier (no target; unit+wire cover).
  Everything else is in-scope — treat any other gap as a defect.
- The m151 docs-repo checkout (`~/Projects/docs`) sits on ITS branch — never
  commit there; use the `~/Projects/docs-m148` worktree.

## /pickup prompt for the next agent

> /pickup — resume M148 close-out in
> `~/Projects/agentsfleet-m148-assigned-runner-policy` (branch
> `feat/m148-assigned-runner-policy`, 20 local commits, spec already in
> `docs/v2/done/`). Read `HANDOFF_Jul_31_07_47.md` at the worktree root first.
> Sole remaining work, in order: (1) confirm the backgrounded `orly gate`
> verdict (re-run `orly gate` if the session lost it); (2) commit the
> uncommitted spec file (Indy's race-ack quote); (3) rebase onto
> `origin/main` (watch `src/runner/daemon/loop.zig` for m150 conflicts) and
> re-run the Zig unit lanes; (4) push, `gh pr create` with the body at the
> prior session's scratchpad `pr_body.md` (regenerate from the spec's
> Discovery + rubric if the scratchpad is gone), plus the docs-repo PR for
> `chore/m148-assigned-policy-changelog`; (5) delete the handoff file;
> (6) run `kishore-babysit-prs`. All verification is already green and graded
> in the spec — do not re-litigate closed decisions; the spec's Discovery
> carries every Indy quote.
