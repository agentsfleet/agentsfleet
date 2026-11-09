---
type: explanation
audience: contributor
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# Prepare M246 in a separate worktree

## What it is

This prompt prepares tenant-owned runners separately from unfinished M243.
The M246 spec names M243 and M241 as dependencies.
Its final integration depends on M243's authentication, periodic recheck and revoke-driven connection closure.

## Why it exists

Indy asked whether M246 could start in another worktree.
Tenant ownership, enrollment and management work can prepare while M243 settles its stream interface.
Do not implement unfinished M243 transport again inside M246.

## How it behaves

```text
Prepare M246_001 for parallel work using the repository's Orly skills.

Repository: agentsfleet/agentsfleet
Use a separate worktree such as /workspace/agentsfleet-m246.
Use its own branch such as feat/m246-tenant-owned-runners from current
origin/main. Never edit through /workspace/agentsfleet-m243.
No M246 branch or worktree was created by the M243 context save.

Read AGENTS.md, .orly/LOCAL.md, .orly/AGENTS.md and the applicable skills.
Read docs/v2/pending/M246_001_P1_API_CLI_DOCS_UI_TENANT_OWNED_RUNNERS.md,
docs/AUTH.md and docs/architecture/push_scheduler.md.
Read the latest M243 handoff and runner protocol from draft PR 819 without
merging the whole incomplete checkpoint into this branch.

M243/PR 819 supplies the authenticated runner stream, interval recheck,
revoke-driven closure and confirmed connection authority. M246 extends it
with tenant ownership, protected model access and token rotation/reload.
M245 is not an explicit dependency in the M246 spec.

Prepare the independent tenant enrollment/management surfaces, ownership
filter and placement policy. Coordinate shared schema, runner auth, claim,
issue, renewal, policy and Cargo changes before touching overlapping work.
Record the concrete interface needed from M243 for final integration.

The pending spec still lists two Discovery decisions: the own_then_platform
fallback trigger/threshold, and proxied platform inference versus tenant
keys only. Check session and issue evidence before asking Indy; do not invent
answers or mark the spec approved without resolving these decisions.
Prepare independent work while the decisions remain open.

Indy's latest M243 correction moves the four active authoring runner calls
onto the native stream. Browser setup chat remains its browser API.
Do not restore old authoring HTTP calls or create a compatibility path.
Do not expose a platform model key to a tenant-controlled runner.

Keep the design small. Postgres owns saved results and billing; add no report
spool, disk outbox, report replay files or trace-based billing recovery.
Preserve HTTP/3 preference with HTTP/2 fallback, hybrid-only X25519MLKEM768,
private routing, shared Bus and native UUIDv7 bindings without SQL casts.

Ask Indy before verification commands, new repository scripts, workflow or
deployment edits, unlisted tables or SQL casts. Coordinate M243/M244 overlap
on issue 818 and read issue 813 before pushing. Use a separate PR for M246;
final stream integration and merge require M243's completed interface.
Do not claim M243 is code complete or verified from its source checkpoint.
```

## Limits

This context save does not create another worktree, start a second agent or approve M246's unresolved product decisions.
M246's spec currently says to start after M243 merges; parallel preparation must preserve that final integration boundary.

## Related pages

- [M243 handoff](session-handoff.md)
- [Resume M243](resume-prompt.md)
- [M246 specification](../../v2/pending/M246_001_P1_API_CLI_DOCS_UI_TENANT_OWNED_RUNNERS.md)
