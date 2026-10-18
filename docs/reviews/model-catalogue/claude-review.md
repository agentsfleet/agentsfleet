---
type: how-to
audience: contributor
verified: 2026-10-18
product_version: 0.61.0
executable: false
---

# Independent catalogue review

## What it is

The [takeover handoff](../../../HANDOFF_M229_CATALOGUE.md) supersedes the earlier read-only prompt below.
Indy now assigns Claude integration and review after its merge; Codex stops after committing the handoff.
There is no Pull Request: Indy requested local commits only. Codex has not run this review.

## Why it exists

The branch has working transport, catalogue and service changes alongside explicitly unfinished integration.
The reviewer must distinguish defects in the submitted implementation from the documented missing work, and verify both.

## How it behaves

```text
Perform a read-only, adversarial review of issue #771, M229 section 8.

Worktree: /Users/kishore/Projects/agentsfleet-worktrees/m229-model-catalogue
Branch: feat/m229-model-catalogue
Base: 53e3d093ece9ff39fde0290924ff30b7ac70efca

Read AGENTS.md and .orly/LOCAL.md first, then the section 8 Dimensions,
test specification, failure modes and Discovery entries in
docs/v2/active/M229_001_P1_API_DOCS_INFRA_POSTGRES_SCHEMA_CLEAN_SLATE.md.
Discovery's eight catalogue decisions win over conflicting section text.
Read the full issue with gh issue view 771 --repo agentsfleet/agentsfleet.
Read docs/reviews/model-catalogue/{catalogue,handover,test-ledger}.md
and HANDOFF_M229_CATALOGUE.md for measured verification.

Review git diff 53e3d093ece9ff39fde0290924ff30b7ac70efca and all nonignored
untracked source. Include both committed and uncommitted changes. Record HEAD
and worktree status before reviewing; do not substitute main as the base.

Do not edit, commit, push, merge, reset databases or open a Pull Request.
Do not read sibling worktrees, print credentials or make paid model calls.
The protected paths listed in handover.md are another agent's responsibility;
read their callers as necessary, but do not change them.

Prioritize monetary correctness, SQL atomicity and concurrency, exact decimal
storage, prompt-tier and UTC date boundaries, provider context provenance,
curated identity pruning, advanced filtering and cursor consistency, native
Gemini thought signatures, Ollama endpoint security and unknown prices,
streamed tools and continuation, typed rows, cause-preserving errors, and
every removed or relaxed test/gate. Check the seeder's actual empty-database
apply and retry behavior, not just its conversion helper.

Validate the verification report against its decisive command output. Separate
mock transport tests from real datastore tests and live provider metadata
reads. Require 99% whole-file coverage for each touched measured Rust file and
100% TypeScript coverage. Do not infer complete billing from lookup tests.

Known open items must remain visible: per-request reporting and settlement
design; local Ollama customer-runner ownership; authoring model eligibility,
token counting and provider dispatch; administrator metadata/tier editor;
protected API/query/report/OpenAPI integration; and twelve missing exact
provider context proofs. Only six identities are currently seeded. The app
at localhost:3000 belongs to the separate #754 worktree and uses api-dev;
it is not a preview of this branch.

For each substantiated finding give severity, file:line, a concrete trigger,
user or data impact, supporting evidence, and the smallest proposed repair.
Group duplicate symptoms by cause. Distinguish new regressions, known
unfinished requirements and pre-existing base failures. List checks you could
not run. End with a clear merge recommendation and its decisive blockers.
```

## Limits

This earlier prompt describes read-only review. Indy's later takeover instructions govern Claude's work.
The takeover handoff records prior verification and unresolved decisions; Codex runs no further review or test commands.

## Related pages

- [Catalogue and source matrix](catalogue.md)
- [Integration handover](handover.md)
- [Test ledger](test-ledger.md)
- [Verification](../../../HANDOFF_M229_CATALOGUE.md)
