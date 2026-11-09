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
The milestone is incomplete; current compilation, lint and Rust unit evidence is recorded in the handoff.
Native integration execution is paused until code completion.

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
The preceding incorporated tip was Claude's 3e3d8f1b0; b5e971269 fixes the
three unit failures, and 9c2803f91 contains the authoring guard work.
Claude's musl-builder and datagram-socket patch commits are preserved.
Claude reports the r2 image published for both architectures in #818 comment
6743710519. Its native Linux CI reruns/static-build results remain pending;
local amd64 emulation builds were stopped. Check #818 for newer results.

Reuse /workspace/agentsfleet-m243 if present. Otherwise fetch the published
feature branch into an isolated worktree. Preserve the current work.
Fetch the latest feature branch before editing and incorporate newer published work.

Read first:
1. AGENTS.md, .orly/LOCAL.md, .orly/AGENTS.md and the orly-build skill.
2. docs/reviews/m243/session-handoff.md and owner-feedback-audit.md.
3. docs/reviews/m243/implementation-signoff.md, latest Nov 09 corrections.
4. docs/architecture/runner_protocol.md.
5. docs/v2/active/M243_001_P1_API_DOCS_INFRA_OBS_RUNNERS_HOLD_ONE_PUSH_STREAM.md.

M243 is incomplete. Finish code/review first, then perform one combined final
verification stage. The handoff records the current scheduler checkpoint checks.
The preceding authoring checkpoint passed workspace cargo check and make
lint-all. Full Rust units passed 5,380 before its guard refactor; the affected
library shard passed 3,531 afterwards. These are historical scoped results,
not a full repository verdict for the current scheduler/dependency changes.

Indy explicitly authorized compilation, formatting, lint, tests and gates on
Nov 09. That supersedes the earlier verification hold; do not ask again for
routine checks. Indy subsequently paused integration tests until code completion.
Keep that pause; read the handoff for results and cloud native tool setup.

Kernel examples and current caller fixtures are migrated; RUNNER_REPORTS stays removed.
Authoring model calls, proposals, renewals and completion use one native session.
Their HTTP adapters/routes are removed. The shared checked boundary is
afd_authoring/src/lease/guard/connected.rs; model and proposal callers request
one guard. It checks connection authority and fresh time after the host lock.
Accepted provider work stays tracked through disconnect, preserving known usage.
Twelve native authoring integration cases are written but lack a complete run.
Do not claim them verified, and do not redispatch an uncertain physical model call.

Then complete source and tests for listener drain, native child cleanup, reconnect/cancellation,
renewals/folds/report retries, remaining polling fixtures and benchmarks,
failure/fairness/observability sources, full Orly Rust review including
RS-SENTINEL and RS-UNWRAP, docs and generated OpenAPI reconciliation.
Dimensions 4.4 and 4.5 now have bounded pass/probe source and passing focused
unit cases. The scheduler awaits session-owned claims; wakes during a pass
coalesce into one subsequent pass. The handoff records the exact check scope
and the 14 changed first-party Rust files still over the file-length ceiling.
Review queued-reader-frame semantics in SessionSupervisor::drain as a specific
open question; do not describe it as a confirmed runtime bug without evidence.

Reports and decision receipts retry only from caller memory until daemon
acknowledgement. Postgres owns saved results and billing. No report spool,
disk outbox, disk replay, old-format retry or compatibility alias may return.
The temporary authoring/report.rs and lease/tagged.rs were removed.

Preserve HTTP/3 preference with HTTP/2 transport fallback, private Fly routing,
hybrid-only X25519MLKEM768 on both transports and ends, native UUIDv7 bindings
and the shared Bus abstraction. Production path and certificate lifecycle
evidence remain open; do not claim them proved by local source.

Ask Indy before new repository scripts, workflow/deployment edits, unlisted
tables or SQL casts. Preserve Claude's M244 work; coordinate overlaps on
issue 818 and read issue 813 before every push. Push reviewable checkpoints
to existing PR 819 without rebasing or force-pushing its published branch.

Keep the PR draft and hold merge for Indy's listener, certificate and
1Password review. Tell Indy when milestone code is complete, then run the
remaining verification lanes. Integration remains paused until that point.
```

## Limits

This prompt transfers context and instructions; it does not certify a working runner session.
Read the source and detailed handoff before continuing unfinished code.

## Related pages

- [Detailed session handoff](session-handoff.md)
- [Owner feedback audit](owner-feedback-audit.md)
- [M246 parallel preparation](m246-parallel-prompt.md)
