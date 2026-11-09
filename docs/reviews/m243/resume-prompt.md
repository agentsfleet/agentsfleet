---
type: explanation
audience: contributor
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# Resume M243 in a new cloud session

## What it is

Copy the prompt below into the new session after the context-save commit reaches PR 819.
Use the published feature-branch tip containing this file.
The checkpoint is incomplete and has no passing full build or test lane.

## Why it exists

Indy requested source changes, review notes and resume prompts together in the existing draft PR.
The detailed handoff names unfinished interfaces and known source migration gaps.

## How it behaves

```text
Resume M243_001 using the orly-build skill.

Repository: agentsfleet/agentsfleet
Branch: feat/m243-runner-push-stream
Existing draft PR: https://github.com/agentsfleet/agentsfleet/pull/819
Coordination: https://github.com/agentsfleet/agentsfleet/issues/818
Use the latest published context-save commit containing this prompt.
The preceding checkpoint was a0a8cf3b766d74fb7984d332cbe5cc49010bc0a0.

Reuse /workspace/agentsfleet-m243 if present. Otherwise fetch the published
feature branch into an isolated worktree. Preserve the current work.

Read first:
1. AGENTS.md, .orly/LOCAL.md, .orly/AGENTS.md and the orly-build skill.
2. docs/reviews/m243/session-handoff.md and owner-feedback-audit.md.
3. docs/reviews/m243/implementation-signoff.md, latest Nov 09 corrections.
4. docs/architecture/runner_protocol.md.
5. docs/v2/active/M243_001_P1_API_DOCS_INFRA_OBS_RUNNERS_HOLD_ONE_PUSH_STREAM.md.

This is incomplete source with failed compilation attempts. Do not claim code
completion, a green checkpoint or passing tests from earlier revisions.

Indy explicitly authorized compilation, formatting, lint, tests and gates on
Nov 09. That supersedes the earlier verification hold; do not ask again for
routine checks. Read the handoff for exact results and cloud native tool setup.

First migrate the sandbox kernel examples still using RUNNER_REPORTS, then
finish native end-to-end fixtures and compile the complete workspace.
The four authoring daemon handlers, shared domain wiring and current-owner
checks are written, and their four HTTP adapters/routes are removed.
Finish caller migration and native integration evidence. Preserve database
lock ordering, provider dispatch accounting and memory-only result delivery.
Do not redispatch an uncertain physical model call.

Then complete source and tests for listener drain, native child cleanup, reconnect/cancellation,
renewals/folds/report retries, remaining polling fixtures and benchmarks,
failure/fairness/observability sources, full Orly Rust review including
RS-SENTINEL and RS-UNWRAP, docs and generated OpenAPI reconciliation.
The handoff records initial review fixes, unfinished Dimension 4.4 and file-length findings.

Reports and decision receipts retry only from caller memory until daemon
acknowledgement. Postgres owns saved results and billing. No report spool,
disk outbox, disk replay, old-format retry or compatibility alias may return.
The temporary authoring/report.rs and lease/tagged.rs were removed.

Preserve HTTP/3 preference with HTTP/2 transport fallback, private Fly routing,
hybrid-only X25519MLKEM768 on both carriers and ends, native UUIDv7 bindings
and the shared Bus abstraction. Production path and certificate lifecycle
evidence remain open; do not claim them proved by local source.

Ask Indy before new repository scripts, workflow/deployment edits, unlisted
tables or SQL casts. Preserve Claude's M244 work; coordinate overlaps on
issue 818 and read issue 813 before every push. Push reviewable checkpoints
to existing PR 819 without rebasing or force-pushing its published branch.

Keep the PR draft and hold merge for Indy's listener, certificate and
1Password review. Tell Indy when code is complete; verification is authorized.
```

## Limits

This prompt transfers context and instructions; it does not certify a working runner session.
Read the source and detailed handoff before continuing unfinished code.

## Related pages

- [Detailed session handoff](session-handoff.md)
- [Owner feedback audit](owner-feedback-audit.md)
- [M246 parallel preparation](m246-parallel-prompt.md)
