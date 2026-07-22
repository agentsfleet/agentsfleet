# HANDOFF — M135_004 dev release acceptance gate (parked, harness complete)

**Branch:** `feat/m135-release-readiness` (worktree `../agentsfleet-m135-release-readiness`)
**Spec:** `docs/v2/active/M135_004_P0_INFRA_UI_DEV_RELEASE_ACCEPTANCE_GATE.md` — all Sections/Dimensions DONE, rubric graded, parked on R2/R3/R4.

## State

Harness and workflow work is complete and verified:
- Preflight project gates every journey; passes 13/13 against live dev.
- Grouping (journeys / operator chain / wall chain / fetch-audit-last), retries 0, workers 4, remote-env budgets (60s test / 10s expect).
- Vercel bypass secret traded for its derived cookie via storage state — no loaded secret in retained traces (spec Invariant 3).
- `scripts/dev_release_verdict.sh` + notify wiring: green requires qa + browser + CLI success; skip/cancel = red; always-notify fallback.
- Cache keys on real inputs (root `bun.lock` + resolved Playwright version, hardened resolve steps, no restore-keys, artifact overwrite for re-runs).
- 14 hermetic release-gate tests + full vitest (1736), `make lint-all`, `make harness-verify`, gitleaks: all green.

## Why parked (not PR'd)

R2 cannot grade ✅: the remaining browser reds are **deployed-product defects the gate exists to catch**, all in the session/stream auth family:
1. Live workspace stream fails to connect on app-dev — product shows "Reconnecting to live updates"; `fleet-console`, and intermittently `secrets-lifecycle` / `signup-lifecycle`, fail on it. Same family as Indy's `/streams` `UZ-AUTH-002` report.
2. Operator publish Server Action 307s to `/sign-in` mid-flow (session-token refresh on long flows) — `platform-library-onboarding` publish walk. Trace: `ui/packages/app/playwright-acceptance-results/platform-library-onboardin-5da2e*/trace.zip` (from the graded run).

Both routed to the M133 workspace-stream hardening stream. R3/R4 grade on the first CI run after merge.

## Next steps

1. M133: fix dashboard token refresh for streams + long Server Action flows (evidence above).
2. Re-run `make acceptance-e2e` against healthy dev → R2 should go green; regrade.
3. Merge → first `deploy-dev` run grades R3 (job < 600 s) and R4 (`gh run download --name acceptance-e2e-dev-results`).
4. CHORE(close) fully: move spec to `done/`, delete this handoff, open the PR (`ci(dev): make release acceptance fast and truthful`), then `kishore-babysit-prs`.
5. Burn-in recommendation: ~5-10 CI runs before treating the gate as release-blocking (first-ever parallel execution for ~25 journey specs).

## Session Notes material (for the eventual PR body)

See `docs/v2/active/M135_004_*.md` §Discovery for consult log, adversarial-review outcomes (Claude subagent 12 findings / 8 fixed; Codex 5 findings / 4 fixed, 1 recorded), and the residuals routed to M135_003 (fixture session cookies in traces; cross-job fleet mutations between the CLI and browser lanes).
