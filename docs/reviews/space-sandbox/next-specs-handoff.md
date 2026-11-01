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

Two fresh-context Sol Max rounds completed; their findings and completed reads are in [round 1](../development-autonomy/sol-round-1.md) and [round 2](../development-autonomy/sol-round-2.md).
[Astra's completeness review](../development-autonomy/astra-completeness.md) completed after those revisions; the scope is ready for detailed spec drafting.

These are design/source reviews, not runtime proof or the full EGM pipeline. The pending M236 draft still has explicit technical blockers before implementation.

The [owner-prioritized work order](../development-autonomy/README.md) records sequencing without duplicating scenario mechanisms.

## Approved M224 finish plan

Indy replied "Approve this finish plan" on 2026-11-01.
Keep draft #6's regression and confirmed Slack delivery passed.

Finish trace/stopping behavior, actual-model cases and native recall.
After verification, replace the bespoke checker and obsolete commands with concise evidence.
Retain runtime policy tests, resolve current gate failures and publish the verified changes in a PR.

Current evidence and failed observations belong only in the [launch ledger](../../../playbooks/operations/acceptance/baselines/M224_001-launch.md).
The [M224 spec](../../v2/active/M224_001_P0_DOCS_INFRA_FIRST_PRODUCTION_LAUNCH.md) owns milestone closure; [M224_002](../../v2/active/M224_002_P1_DOCS_UI_APPROVAL_AND_FLEET_ACTIVITY_CLARITY.md) owns the requested chat/approval changes and completion amendment.

## Restart prompt

- Read AGENTS.md, the linked M224 specs/ledger and the canonical durable-repair scenario; recheck remote branch and PR state.
- Finish the approved M224 work without claiming incomplete live cases passed or authoring the fleet's demonstration fix in Codex.
- Draft one goal-driven autonomy implementation spec from the reviewed scenario, tracing current source and resolving the explicit design choices before runtime implementation.
- Use Grafana and GitHub in development with the existing scheduler; Elasticsearch and a new sandbox backend are not prerequisites.
- Preserve reviewed setup/grant boundaries, cumulative limits and human merge/deploy authority.
- Keep each decision in its owning architecture page and link rather than duplicate; use the required EGM/spec and gstack/review workflows honestly.
- Push the verified implementation/spec changes and open their scoped PRs; do not merge or deploy production.
