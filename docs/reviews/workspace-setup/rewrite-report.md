---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Workspace chat rewrite and change report

## What it is

Read this when comparing the reviewed proposal with the rewritten documents and two pending implementation specs.
The [review packet](review-packet.md) gives the reading order; the [resource reference](../../architecture/api_resources.md) owns proposed routes and types.

| Question | Answer |
|---|---|
| What is ready for review? | The rewritten proposal, source-answer ledger, layouts, owning spec, folded website spec, and this change report. |
| What is implemented by this rewrite? | Documentation and planning changes only; backend implementation and fresh application verification remain pending. |
| Which decisions stand? | [L1–L21](decisions.md), the approved interface, one existing worktree, and one product Pull Request (PR). |
| What do the document checks prove? | Structural spec consistency, local links, JSON examples, source-reference bounds, and route counts. Application behavior remains unverified. |

## Why it exists

Earlier review bundles describe superseded resources and machinery.
Reviewers need the current proposal, a document comparison, and clear limits on the evidence.

## How it behaves

### Comparison inputs

The comparison input is `/private/tmp/agentsfleet-chat-review-response-20261014.zip`.
It contains the prior reconciliation and both original reviewer responses.
The earlier packaging receipt recorded that archive's hash and byte comparisons. It remains historical; the current review uses linked Markdown files.

“Rewritten” means the document existed in that response bundle and its bytes changed.
“New” means it was absent there.
These labels compare review snapshots; they do not describe committed files or implemented features.

The inspected source revision remains `ad3ed8f92c739b59df71863dbeec81d621dd7d0a`.
[Five source findings](source-findings.md) preserve the inspected evidence and its qualifications.
Document checks validate references and line bounds; they do not independently prove runtime behavior. The current Astra update adds source checks recorded in the resolution ledger.

### Proposal and design documents

| Document | Comparison | Change and review purpose |
|---|---|---|
| [chat_authoring.md](../../architecture/high_level.md#fleet-authoring-and-runtime) | Rewritten | Selects one private authoring flow, explicit publication, retained setup identity, and a coordinated cutover. |
| [chat_authoring_decisions.md](decisions.md) | New | Records all locked owner decisions, one-PR packaging, and the separate schedule follow-up. |
| [chat_authoring_source_findings.md](source-findings.md) | New | Answers five backend questions with source evidence, corrections, and stated limits. |
| [chat_authoring_codex_response.md](codex-response.md) | Rewritten | Replaces earlier alternatives with the selected design, disagreement costs, hostile cases, and expected operational costs. |
| [chat_authoring_definition.md](../../architecture/fleet_definitions.md#validated-definition) | Rewritten | Wraps existing typed policy, uses small atomic edits, binds human review, and moves budget readers to nanos. |
| [chat_authoring_api_resources.md](../../architecture/api_resources.md) | Rewritten | Defines the sole target route/type surface, exact preconditions, replay rules, wire strings, private streams, and proposed errors. |
| [chat_authoring_api.md](api.md) | Rewritten | Separates source-backed gaps from reusable behavior and states schedule prerequisites and provider limits. |
| [chat_authoring_creation.md](../../architecture/data_flow.md#fleet-creation-and-setup-recovery) | Rewritten | Uses installing fleets, actual grants, guarded activation, one initial admission, explicit retry/Kill, and command-line interface (CLI) partial outcomes. |
| [chat_authoring_runner.md](../../architecture/runner_fleet.md#authoring-leases) | Rewritten | Shares one claim/capacity limit; bounds tokens, time, authority, spending, uncertain usage, and private copies. |
| [chat_authoring_runtime_baseline.md](runtime-baseline.md) | Rewritten | Preserves source-backed runtime behavior and corrects install, steer recovery, budget renewal, ordering, and delivery claims. |
| [chat_authoring_end_to_end.md](../../architecture/data_flow.md#fleet-setup-and-runtime-sequence) | Rewritten | Shows sixteen detailed entry, review, setup, runtime, recovery, deletion, and release panels. |
| [workspace-chat-options.md](../../designs/workspace-chat-options.md) | Rewritten | Reconciles navigation, contextual drawers, Save choices, setup progress, Library actions, mobile behavior, and recovery copy. |
| [chat-within-a-workspace.md](../../designs/chat-within-a-workspace.md) | Rewritten | Retains the approved appearance while updating ownership, sharing, controls, statuses, and cleanup behavior. |
| [chat_authoring_file_inventory.md](file-inventory.md) | New | Lists discovered rename paths, planned additions, historical exceptions, and carried work without treating planned files as source. |
| [chat_authoring_public_changes.md](public-changes.md) | New | Lists separate public-docs work, the approved Representational State Transfer (REST) guide amendment, and release/recovery requirements. |
| [chat_authoring_review_packet.md](review-packet.md) | Rewritten | Provides current reading order, both specs, this report, direct Markdown delivery, and pending implementation work. |
| [chat_authoring_claude_review_prompt.md](claude-review-prompt.md) | Rewritten | Requests read-only enforcement review, source citations, counterexamples, negative tests, and a qualified verdict. |
| [chat_authoring_chatgpt_review_prompt.md](chatgpt-review-prompt.md) | Rewritten | Uses the same authority and review questions; requires explicit qualification when source cannot be independently read. |
| [chat_authoring_followup_review_prompt.md](followup-review-prompt.md) | Rewritten | Applies the same locked choices and evidence requirements to follow-up review. |

### Related architecture pages

These rows describe the carried branch changes against the recorded source revision.
The Astra update extends the existing report and proposal; it does not edit runtime source or the carried visual implementation.

| Document | Change |
|---|---|
| [README.md](../../architecture/README.md) | Adds questions pointing to the proposal, designs, specs, public handoff, and this report. |
| [fleet_definitions.md](../../architecture/fleet_definitions.md) | Connects typed definitions, shared parsing/export, support-file pins, nanos, and proposed resumable creation. |
| [runner_execution.md](../../architecture/runner_execution.md) | Connects private authoring to shared runner capacity while keeping existing fleet execution authoritative. |
| [web_app.md](../../architecture/web_app.md) | Connects approved chat layouts to existing server/client boundaries and subject-scoped recovery. |
| [billing_and_provider_keys.md](../../architecture/billing_and_provider_keys.md) | Separates proposed bounded authoring admission from funds reservation and preserves key visibility. |
| [data_flow.md](../../architecture/data_flow.md) | Adds the proposed setup sequence and corrects admission identity, secret preflight/classification, and install-versus-work ordering. |

### Implementation specs

| Document | Comparison | Change |
|---|---|---|
| [M227_001](../../v2/active/M227_001_P0_API_CLI_DOCS_UI_WORKSPACE_CHAT_CUTOVER.md) | New | Owns the cutover, seven implementation sections, failure proofs, file scope, prerequisites, and final repository checks. |
| [M227_002](../../v2/done/M227_002_P1_CLI_DOCS_UI_WEBSITE_BRANDING_BATCH.md) | New | Folds carried website, PostHog analytics, branding, shell, email, and readme work into the owning PR. |

Both specs remain `PENDING`; their comparison revision is recorded and their test baseline remains pending.
Packaging adds this report to M227_001's file scope and the expanded inventory.

The fresh reference check also found an out-of-range queue-wait citation.
The runtime map now cites the inspected functions at lines 22–34 of `rustd/crates/afd_fleet/src/lease/deliver/queue_wait.rs`.

### Carried documentation

These pages describe existing carried work.
Their inclusion supplies review context. The website journey diagram now follows L7; its source-copy change and fresh proof remain pending.

| Document | Review context |
|---|---|
| [Root readme](../../../README.md) | Product entry and shared branding. |
| [Branding readme](../../../branding/README.md) | Asset sources, variants, and distribution. |
| [CLI readme](../../../cli/README.md) | CLI entry and shared branding. |
| [Design system](../../DESIGN_SYSTEM.md) | Approved typography, tokens, components, and assets. |
| [Product analytics](../../architecture/product_analytics.md) | Explicit click allowlists and private-input exclusions. |
| [Website entrance](../../architecture/website-job-entrance.md) | Approved website journey and entry behavior. |

### Astra review update

The [required-change ledger](codex-response.md) records all ten supplied findings, source assessment, revised mechanisms, and pending proofs.
The canonical API was updated first; Definition, runner, source/runtime maps, specs, inventory, layouts, diagrams, and public handoff follow it.

| Revised requirement | Reviewable result |
|---|---|
| Scoped model invocation | Fourth machine route with daemon-held keys, issued capability, guarded dispatch receipts, bounded replay and live revocation. Person inventory remains 17/7/2. |
| Combined capacity | Claims and both renewal kinds share the host guard; database-time expiry rejection prevents revival. |
| Candidate edits | Lease and proposal acknowledgement expose immutable candidate ID/revision; exact retry recovers the original accepted version. |
| Fleet recovery | Existing queued message reads include durable pending admissions without receipts; logical identity deduplicates reads and eventual work. |
| Model and failure recovery | Idle versioned model replacement, pinned earlier turns, and durable bounded failure/action fields after reload. |
| Definition validation | Valid slug/version example and full supported metadata/policy/manifest round trips. Display title remains separate. |
| Private spool | Honest logical expiry, decryptability and powered-off retention; supervisor startup checks every content copy before read/upload. |
| Website behavior | Spec and journey diagram separate ordinary fleet Save from explicit Library publication; source correction remains planned. |
| Acceptance evidence | Expected/observed four-build manifest, startup/environment procedure, mismatch rejection before fixture mutation, release and recovery rehearsal. |
| Rust layout | Documented production crate, focused mod.rs/behavior files, local error helpers, and tests split by behavior. Exonum path absent; Arrow/local patterns inspected. |

Both specs remain PENDING. No required implementation proof is marked passed.
The inventory now names the additional model transport, renewal, queued-reader, supervisor spool, modular tests, and acceptance-preflight paths.

### Document verification

Delivery remains individual Markdown files; no new ZIP is created.
Older archives and their receipts describe previous snapshots and are not the current review authority.

Run the structural spec check on both final specs and the focused Markdown reference checker after all edits.
The checker records its evidence in /private/tmp/agentsfleet-chat-cutover-doc-check.json, including document hashes and explicit application-test exclusions.
The review-update file comparison checks that this continuation changed only the authorized Markdown paths.

The final structural checks passed for both M227 specs.
The focused check passed across 23 Markdown files: 179 local links, two JSON examples, and 78 source-reference bounds.
The route inventory is 17 new, seven changed, and two removed person routes, plus four new machine routes.
A full workspace file comparison found 21 changed Markdown files and no changed application source, lockfiles, or assets in this update.

Repository unit/integration tests, browser proofs, provider calls, and coverage remain not run during this update.

## Limits

Document consistency and a spec-template pass do not establish implementation readiness, reviewer approval, or deployed behavior.
Proposed constants and errors still need implementation and meaningful failure tests.

All ten supplied findings are reconciled in the documents. A focused follow-up review of the four P1 mechanisms is the next review step before affected flows are implemented.
Implementation also needs the recorded lifecycle/commit decision resolved and the approved guide amendment applied at its owning source.
Verify the independently owned schedule fix before schedule acceptance; its stronger provider fencing remains the separate L8 follow-up.

## Related pages

- [Review packet and reading order](review-packet.md)
- [Locked owner decisions](decisions.md)
- [Source-backed agreements and qualifications](source-findings.md)
- [Public documentation handoff](public-changes.md)
