---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Workspace chat cutover review packet

## What it is

The amended backend proposal,21 locked owner decisions, source answers, detailed flow/layouts, and two linked implementation specs for one product PR on `feat/website-completion`. It supersedes the earlier route set. Backend implementation remains pending.

## Why it exists

Chief Technology Officer (CTO) AIWA reviewers should assess one consistent design, its pessimistic failure cases, and missing proof requirements. The interface remains approved. The same current worktree holds website/PostHog/branding and chat cutover work.

## How it behaves

### Reading order

| Order | Document | Review purpose |
|---|---|---|
|1|[Owner decisions](decisions.md)|Do not reopen L1–L21; one-PR packaging and L8 follow-up.|
|2|[Five source findings](source-findings.md)|Agree/rebut each remaining backend question with source.|
|3|[Resolution ledger](codex-response.md)|Agreement, evidence, costs; dark/pessimistic/optimistic reasoning.|
|4|[Overview](../../architecture/high_level.md#fleet-authoring-and-runtime)|User goal and selected architecture.|
|5|[Definition](../../architecture/fleet_definitions.md#validated-definition)|Existing typed policy, small edits, nanos, review/manifest/marker.|
|6|[Resource reference](../../architecture/api_resources.md)|Exact17 new/7 changed/2 removed person routes; one claim plus 4 machine routes.|
|7|[API gaps](api.md)|What exists and what is missing, source-backed.|
|8|[Creation](../../architecture/data_flow.md#fleet-creation-and-setup-recovery)|Retained installing identity, approvals, activation, initial run, retry/Kill/CLI.|
|9|[Runner](../../architecture/runner_fleet.md#authoring-leases)|Capacity, authority, tokens, spend, unknown usage, Stop/delete/copies.|
|10|[Runtime map](runtime-baseline.md)|Preserved inspected runtime and corrected claims.|
|11|[ASCII flow](../../architecture/data_flow.md#fleet-setup-and-runtime-sequence)|Sixteen complete journey/failure panels.|
|12|[Layouts/options](../../designs/workspace-chat-options.md)|Navigation, drawers, models, review, Save, mobile, recovery.|
|13|[Approved interface](../../designs/chat-within-a-workspace.md)|Visual direction and necessary copy changes.|
|14|[Owning M227_001 spec](../../v2/done/M227_001_P0_API_CLI_DOCS_UI_WORKSPACE_CHAT_CUTOVER.md)|Sections, failure assumptions, proof mapping and final gates.|
|15|[Folded M227_002 spec](../../v2/done/M227_002_P1_CLI_DOCS_UI_WEBSITE_BRANDING_BATCH.md)|Carried website/PostHog/branding in the same PR.|
|16|[Exact file inventory](file-inventory.md)|Whole-repo rename hits, planned files, historical exceptions, carried batch.|
|17|[Public handoff](public-changes.md)|Separate docs-repo branch and approved guide amendment.|
|18|[Rewrite report](rewrite-report.md)|Per-document changes, comparison inputs, and pending evidence.|

### Review prompts

[Claude prompt](claude-review-prompt.md), [ChatGPT prompt](chatgpt-review-prompt.md), and [focused follow-up prompt](followup-review-prompt.md) all point to this same route/type authority. They request read-only review; no agents are launched by preparing these prompts.

### Markdown review files

Use the linked Markdown files directly, as Indy requested. Both M227 specs, prompts, and this packet remain individual .md files; no new archive is produced. Previous ZIP snapshots are historical and omit these Astra revisions.

The [Astra required-change ledger](codex-response.md) maps all ten findings to revised requirements and pending proofs. The [rewrite report](rewrite-report.md) records document-check evidence and its limits.

### Pending implementation punchlist

1. Review the ten Astra dispositions and revised canonical API, especially scoped dispatch, renewal, candidate version handoff, and receipt-less recovery. Document repairs do not establish new reviewer approval.
2. Resolve implementation lifecycle activation versus the recorded deferred-commit instruction; apply the approved conversation clause in its owning source. Confirm schedule-fix merge/test evidence before schedule acceptance.
3. Implement M227_001 sections: shared typed definition/nanos/schema, private messages/review, runner safeguards, resumable setup/CLI, Library/revisions/Restore, workspace chat/recovery, runtime/release proof.
4. Complete the carried M227_002 batch and actual required unit/integration/browser/coverage checks.
5. Prepare matching published-docs pages on their own branch from the handoff.
6. Review the final diff, commit, and open one product PR when the deferred implementation and gate work are complete.

No backend code, commit, push, deploy, or outside-repository documentation edit is performed by this rewrite. Neither a template pass nor existing test files establishes fresh coverage. L8 content-revision provider fencing stays a separate follow-up.

## Limits

Evidence is source inspection at `ad3ed8f92c739b59df71863dbeec81d621dd7d0a`. The separate schedule branch has an independent owner; its live provider gate numbers are not claimed here. Provider/backups erasure and zero overdraft are not promised.

## Related pages

- [Claude review prompt](claude-review-prompt.md)
- [ChatGPT review prompt](chatgpt-review-prompt.md)
- [Resource authority](../../architecture/api_resources.md)
