# Development autonomy: decisions and restart

## Accepted direction

- Orly 0.28 ships M15_002 cleanup; M16's gate-language decision is separate. The [broader Orly/EGM/Jev workflow proposal](orly-egm-jev-handoff-prompt.md) has no assigned 0.29–0.30 releases.
- Write one Agentsfleet implementation spec for the goal/Jev loop and durable repair lifecycle. Keep a new Firecracker backend and PR review posting separate.
- Production launch follows written M23x specs and demonstrated development autonomy; this is not release authority.
- M224 retains its reduced manual-first evidence scope. Development rollout identity verification, full Linux acceptance and the Elasticsearch-dependent incident demonstration are descoped.

The [durable repair scenario](../../architecture/scenarios/durable-repair.md) is the canonical proposed flow and combined acceptance owner.
It covers the Codex-opened Linkwarden PR, later CI, worker replacement, human steering, concise progress, soft failures and Grafana/scheduler variant.
The [sandbox evaluation](../../architecture/sandbox_evaluation.md) owns save/restore/backend mechanics; avoid copying those decisions here.

## Review status

Two Sol Max reviews and Astra's completeness review completed.
Their corrections are recorded in the owning scenario and pending M236 draft.
M236 still needs its named technical design decisions before implementation.

## Work order

1. Native memory recall and varied social replies: verified in the launch ledger.
2. Chat and approval polish: implemented in PR #796; the targeted signed-in page and preflight passed ten checks.
3. M224: eight native behavior proofs are complete and the custom evidence checker is removed; final repository results belong to PR #796.
4. Orly 0.28: M15_002 is drafted in Orly PR #75; implementation follows review.
5. M236: finish the unified goal/Jev/durable-repair design before implementation.
6. CLI: cancellation, bounded polling/permanent errors and incomplete approval pagination are fixed in PR #796.
7. Demonstrate Grafana/GitHub monitoring and repair with the existing scheduler in development.

## Approved M224 finish plan

Indy replied "Approve this finish plan" on 2026-11-01.
Keep draft #6's regression and confirmed Slack delivery passed.

Finish trace/stopping behavior, actual-model cases and native recall.
After verification, replace the bespoke checker and obsolete commands with concise evidence.
Retain runtime policy tests, resolve current gate failures and publish the verified changes in a PR.

Current evidence belongs only in the [concise native ledger](../../../playbooks/operations/acceptance/baselines/M224_001-ci-evidence.md); the linked launch record retains earlier observations.
The [M224 spec](../../v2/active/M224_001_P0_DOCS_INFRA_FIRST_PRODUCTION_LAUNCH.md) owns milestone closure; [M224_002](../../v2/active/M224_002_P1_DOCS_UI_APPROVAL_AND_FLEET_ACTIVITY_CLARITY.md) owns the requested chat/approval changes and completion amendment.

## Restart prompt

- Read AGENTS.md, the linked M224 specs/ledger and the canonical durable-repair scenario; recheck remote branch and PR state.
- Recheck PR #796 final verification and CI; M224 native proof and checker cleanup are complete. Codex must not supply the fleet's demonstration fix.
- Draft one goal-driven autonomy implementation spec from the reviewed scenario, tracing current source and resolving the explicit design choices before runtime implementation.
- Use Grafana and GitHub in development with the existing scheduler; Elasticsearch and a new sandbox backend are not prerequisites.
- Preserve reviewed setup/grant boundaries, cumulative limits and human merge/deploy authority.
- Keep each decision in its owning architecture page and link rather than duplicate; use the required EGM/spec and gstack/review workflows honestly.
- Push the verified implementation/spec changes and open their scoped PRs; do not merge or deploy production.
