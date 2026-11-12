# M249_002: Fleet commands and scoped discovery

**Prototype:** v2.0.0
**Milestone:** M249
**Workstream:** 002
**Date:** Nov 12, 2026
**Status:** PENDING
**Priority:** P1, make the approved resource vocabulary and operator capabilities discoverable
**Categories:** API, CLI, DOCS, UI
**Batch:** B2, independent command work may start alongside the UI refresh
**Depends on:** M249_001 must merge before this workstream merges; it does not block starting CLI work
**Provenance:** Agent-generated from Indy's approved Flow 1, CLI grouping and permission-discovery decisions
**Canonical architecture:** `docs/architecture/api_resources.md`; `docs/architecture/context_lifecycle.md`

## Overview

**Goal (testable):** Users find coherent fleet commands and only the platform operations their effective permissions advertise, while every operation retains backend authorization.

**Problem:** Fleet verbs are scattered across root help. Fleet library terminology differs from the approved Plugins interface. Platform help advertises operations an ordinary account cannot use.

**Solution summary:** Organize commands by resource, rename the complete-setup library surface to Plugins, and construct permission-aware help and completions using Effect's public command APIs. Keep identity-provider details behind the existing backend identity contract.

**Design flow:** Existing argv/config resolution → existing credential and API-target selection → bounded effective-scope discovery → fresh command factories → Effect parsing/help/completions → existing handler and backend authorization. Plugin selection still starts a private setup from an immutable complete definition.

**Delivery order:** Start now from a fresh worktree at current `origin/main`. Work on the CLI independently. The UI refresh PR merges first. Fetch and rebase onto that merge before changing the refreshed UI's API callers and before final verification or merge. Do not copy older UI layouts into the refreshed application.

## PR Intent & comprehension handshake

- **PR title (eventual):** Unify fleet commands and filter platform discovery by scope
- **Intent:** Make the approved command vocabulary usable without weakening permissions or changing what a Plugin contains.
- **Authoring handshake:** This combines the previously discussed CLI/API cleanup and scope-aware discovery into one follow-up. It is included as a pending spec in the UI refresh PR; its implementation belongs to a separate branch and PR.
- **Implementer handshake:** Record the fetched base, UI PR merge status, command/API inventory, chosen invocation lifecycle and affected callers before editing.

## Implementing agent — read these first

1. `cli/src/cli.ts`, `cli/src/program/entry/argv-scan.ts` and `cli/src/program/tree/root.command.ts` — current credential/target resolution, tree composition and execution.
2. `cli/src/lib/me-ping.ts`, `cli/src/services/config.ts` and `cli/src/program/auth-guard.ts` — identity decoding, environment-key precedence and current refusals.
3. `cli/src/program/tree/platform.command.ts`, `platform-keys.command.ts`, `platform-models.command.ts` and `platform-runners.command.ts` — actual independent operator families.
4. `docs/DESIGN_SYSTEM.md` and the M249_001 spec — refreshed UI terminology, ownership and recovery behavior to preserve after rebasing.
5. Installed `effect/cli` public `Command`, `GlobalFlag` and `CliConfig` APIs, especially `Command.unlisted` — inspect the pinned version rather than introducing a second parser.

## Files Changed (blast radius)

| File or family | Action | Why |
|---|---|---|
| This pending spec and its eventual `done/` path | EDIT/MOVE | Grade the delivered follow-up, not the UI PR |
| `cli/src/cli.ts` | EDIT | Construct the invocation tree and enable completions |
| `cli/src/program/tree/root.command.ts` | DELETE | Remove the mixed assembly/leaf-handler module |
| `cli/src/program/tree/index.ts` | CREATE | Small composition entry point; no leaf handlers |
| `cli/src/program/tree/auth.command.ts` | CREATE | Own login, logout and identity definitions |
| `cli/src/program/tree/diagnostics.command.ts` | CREATE | Own diagnostics definitions |
| `cli/src/program/tree/global-flags.ts` | CREATE | Shared global flag definitions only |
| Existing fleet/chat/access/workspace/platform tree modules | EDIT | Own complete domain groups and scoped visibility |
| `cli/src/lib/me-ping.ts` and invocation discovery module | EDIT/CREATE | Strict scope projection with bounded reads |
| `cli/src/commands/` and `cli/test/` affected domains | EDIT | Rename entry points while retaining handlers and negative proofs |
| `rustd/crates/daemon/afd_http/src/route/` affected routes | EDIT | Plugin vocabulary and platform catalogue removal |
| `rustd/crates/daemon/afd_api_wire/src/library/` and affected wire types | EDIT | Public Plugin names without changing Definition semantics |
| Affected library, draft, lifecycle, permission and SQL modules | EDIT | Update authoritative names and fresh-development schema |
| `ui/packages/app/lib/api/draft-library.ts` and affected API adapters/types | EDIT | Update callers after the UI refresh merges |
| `public/openapi.json` | GENERATE | Reflect the implemented API; never hand-edit |
| Affected canonical docs, playbooks and changelog | EDIT | Commands, scopes, examples and removed spellings |
| Separate `agentsfleet/docs` branch | EDIT | Public reference and changelog under that repository's workflow |

Before implementation, replace family-level inventory with the exact files found in the fetched code. Do not move this entire table into a newly named monolith.

## Applicable Rules

- Repository `AGENTS.md`, `.orly/LOCAL.md`, `.orly/AGENTS.md`, and nested instructions.
- `.orly/standards/spec.md`, `pr.md`, `tests.md`, and the applicable CLI, HTTP, TypeScript and Rust standards.
- `docs/RUST_ERROR_STANDARD.md` before changing fallible Rust signatures.
- No committed helper scripts without the repository's required sign-off; temporary helpers stay outside the checkout.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| `orly doctor`, `orly gate work`, `orly gate verify`, `orly gate pr` | Yes | Use the installed repository pin and retain evidence |
| `make lint-all` | Yes | Include source, CLI, app and Rust checks |
| `make test-unit-all` | Yes | Keep coverage floors and existing tests |
| `make test-integration-rustd` | Yes | Exercise fresh-schema routes and actual authorization |
| `make check-version` | Yes | Keep generated consumers consistent |
| `make dry-app` | Yes for caller changes | Recheck setup, Plugin publication and lifecycle flows |
| File/function limits | Yes | Domain factories and shared readers; avoid a replacement root monolith |
| `gitleaks detect --redact --no-banner` | Yes | No credential bytes in code, fixtures or examples |

## Prior-Art / Reference Implementations

- Existing platform-key handlers already support operator model defaults. Retain their behavior and partial-success recovery.
- Existing immutable library definitions, draft source pinning and publication ETags provide Flow 1; do not replace them with a capability system.
- Existing argv scans already resolve `--api`, JSON mode and `--`; extend that lifecycle, not an independent parser.
- `Command.unlisted` hides discovery while preserving exact-name parsing. Use the installed implementation as the source of truth.

## Sections (implementation slices)

### §1 — Coherent command composition

Create domain factories and a small `tree/index.ts`. Remove `root.command.ts` after both help and execution consume the new composition. Root remains a parser concept, not a mixed implementation module.

| Current | Approved destination |
|---|---|
| `list`, `status`, `stop`, `resume` | `fleet list`, `fleet status`, `fleet stop`, `fleet resume` |
| `kill`, `delete` | `fleet archive`, `fleet delete` |
| `steer`, `logs`, `events` | `fleet steer`, `fleet logs`, `fleet events` |
| `approvals`, `schedule`, `memory` | `fleet approvals`, `fleet schedule`, `fleet memory` |
| `grant list`, `grant delete` | `fleet grants list`, `fleet grants revoke` |
| `fleet show` | Keep `fleet show` |
| `library` | `plugins` with list/show/delete and immutable revision inspection |
| `models`, `tenant provider` | One `models` surface for catalogue/configured models/active choice |
| `secret`, duplicate workspace secrets entry | One `secrets` group |
| `connector` | `connections` |
| `api-key` | `api-keys`, separate at root |
| `billing show` | `usage show` |

Keep `chat`, workspace selection, authentication, diagnostics and the existing `platform` operator family. Preserve argument payloads, cancellation, ETags, exit codes, redaction and JSON behavior. `chat --fleet` edits setup; `fleet steer` sends a runtime instruction. Never replay setup text as a runtime message.

- **Dimension 1.1** — Domain help and execution agree → Test `canonical_groups_share_help_and_handlers` (CLI): each command reaches its existing handler under its approved group.
- **Dimension 1.2** — Removed spellings have no parser aliases → Test `removed_root_verbs_and_library_flags_are_rejected` (CLI): old forms fail with current suggestions; `--` still protects message text.
- **Dimension 1.3** — Setup and runtime messages remain distinct → Test `setup_chat_never_calls_runtime_steer` (CLI): editing saves a candidate; runtime instructions reach only steer admission.

### §2 — Plugins retain complete-setup semantics

A Plugin contains a complete fleet Definition with immutable revisions. Selecting a revision creates a private setup; saving creates an independent fleet. Editing/deleting the Plugin does not mutate existing fleets. Preserve current account/workspace ownership and permissions; do not add person-only ownership, sharing or a new owner column.

Rename `chat --library` to `--plugin`; retain `--revision` and `--publish`; retain `--publish-to` as the explicit publication target and rename `--library-etag` to `--plugin-etag`. Publication and fleet creation remain separate writes and user decisions.

| Public surface | Result |
|---|---|
| `/v1/workspaces/{workspace_id}/fleet-libraries` | `/v1/workspaces/{workspace_id}/plugins` |
| Item and revision paths | Matching `/plugins/{plugin_id}/revisions[/{revision_id}]` paths |
| `library` source kind / `library_id` references | `plugin` / `plugin_id`, retaining `revision_id` |
| `library:write` | `plugins:write`, same authority and narrowing semantics |
| Redundant `/fleet-libraries/owned` | Remove; ordinary Plugin collection lists the retained account-owned entries |
| `/v1/admin/fleet-libraries` and platform catalogue CLI | Remove |

Retain immutable Definition content, pinned versions, review digests and optimistic publication checks. No install/apply/invoke/binding API is introduced. Backend identifiers and installed-copy semantics do not change merely because names change.

This is development-only work. Indy permits resetting the development DB. Update canonical schema files and generated contracts together. Add no compatibility aliases, backfill, dual reads, rollout switch or guarded cutover. Reset only task-owned test databases automatically; do not delete an unrelated database.

- **Dimension 2.1** — Plugin reuse pins a full revision → Test `plugin_revision_creates_independent_private_setup` (CLI/API/UI): IDs and review digest stay pinned through adaptation and save.
- **Dimension 2.2** — Scope renaming cannot broaden access → Test `plugin_permission_preserves_narrowed_authority` (integration): omitted or unrelated rights cannot write, publish or delete; normal readers retain only current read rights.
- **Dimension 2.3** — Revisions remain immutable → Test `plugin_revision_and_installed_copy_do_not_follow_updates` (integration): later publication/deletion leaves earlier copies intact.
- **Dimension 2.4** — Removed platform catalogue stays absent → Test `platform_catalogue_routes_and_commands_are_removed` (CLI/API): no advertised entry, parser alias or functioning old route.

### §3 — Archive, deletion and operator defaults

Archive permanently stops a fleet and retains read-only history. There is no Restore. Stop/Resume remain temporary controls. Permanently Delete is directly available on an active or archived fleet; the CLI composes the existing stop prerequisite internally without a second user confirmation.

Retain a confirmed stop if deletion fails. Retry deletion without repeating the stop. Reconcile an uncertain response before claiming success; denied, unavailable or malformed reads never prove deletion. Billing attribution and separately retained settled records are not promised erased.

Rename public terminal status `killed` to `archived` across wire types, filters, CLI, UI callers and the fresh-development schema. No additional lifecycle state is introduced. Existing PATCH/DELETE semantics can support the client flow; do not invent an atomic-delete endpoint as part of a label change.

Keep `agentsfleet platform keys list/set/delete` and `/v1/admin/platform-keys` unchanged. Selecting an existing provider key does not read or replace its bytes. `--api-key-stdin` and explicit replacement preserve current workspace ownership and secret-write requirements. No platform-default UI returns.

Model credentials remain backed by the workspace chosen in the existing API. There is no primary vault, workspace enumeration to choose storage, key copying, or new vault-selection layer.

- **Dimension 3.1** — Direct deletion uses one confirmation → Test `fleet_delete_retains_confirmed_archive_checkpoint` (CLI/integration): active and archived cases, failed stop, partial delete, uncertain responses and scope loss remain recoverable.
- **Dimension 3.2** — Archive is terminal → Test `archived_fleet_has_history_without_restore` (CLI/API/UI): Stop/Resume do not revive an archived fleet.
- **Dimension 3.3** — Platform defaults retain their authorization → Test `platform_default_requires_existing_admin_and_workspace_authority` (integration): an existing reference and piped replacement retain their distinct permission requirements.

### §4 — Scope-aware help and completions

Build a fresh invocation tree with public Effect combinators, including `Command.unlisted` and `Command.withSubcommands`. Do not mutate a shared module-global command, filter rendered help strings, or maintain a second parser.

Resolve the endpoint and credentials using the current entry lifecycle: `--api`/environment/default endpoint rules, environment API-key precedence and stored-login target protections. Resolve effective scopes through existing `GET /v1/users/me`; never decode an opaque credential as a Clerk JWT or call Clerk from the CLI.

Use one invocation-scoped read result; bound discovery to two seconds and add no retry loop or persistent permission cache. The initial implementation may resolve once for a credentialed invocation before building its tree; share the result with identity reporting rather than issuing duplicate identity reads. Authentication/version/signed-out help remain usable without discovery. A failed discovery read hides operator advertising but does not authorize or block an explicitly typed operation; existing guards/backend checks still decide it. Report unknown discovery only in the appropriate help/error channel and preserve JSON stdout discipline.

For visibility, `scopes` must be an array containing only strings. A missing/non-array value or any malformed member makes discovery unknown. Do not reuse the current decoder's permissive bad-member filtering. Unknown well-formed scope strings grant nothing. Apply only the backend's published effective-scope closure and wildcard semantics; never infer unrelated permissions.

| Effective permission | Advertised children |
|---|---|
| None / unknown | No Platform entry in root help, suggestions or newly generated completions |
| `platform-key:read` | `platform keys list` |
| `platform-key:admin` | Key list/set/delete |
| `model:read` | `platform models list/show` |
| `model:admin` | Model reads plus create/update/delete |
| `runner:read` | Runner list/show/events/leases |
| `runner:enroll` | Enroll and only reads included by the existing server closure |
| `runner:write` | Runner mutations and only reads included by that closure |

Advertise a parent only if it has a retained child. Independent families do not imply one another. Visibility cannot prove ownership of a workspace argument supplied later; retain those flag explanations and API checks.

An exact hidden command still parses. Its own help can explain its permission requirement; do not pretend `unlisted` is authorization or that it hides exact-command help. A stale local completion script never bypasses server checks.

Enable Effect's `GlobalFlag.Completions` alongside existing Help/Version/LogLevel support. Support `agentsfleet --completions <shell>` for Bash, Zsh, Fish and PowerShell. Generated scripts are static scoped snapshots: document regeneration after login, target or permission changes. Live dynamic completion is not required.

Preserve the backend's current capability refresh policy: one-minute refresh, with up to fifteen minutes of stale authority during provider outage. Do not claim immediate revocation or extend that policy with a CLI cache.

- **Dimension 4.1** — Help reflects independent permissions → Test `platform_discovery_matches_effective_scope_matrix` (CLI): ordinary, read-only and each admin family expose only allowed children; parents follow children.
- **Dimension 4.2** — Unknown permissions fail closed for discovery → Test `discovery_refuses_malformed_expired_and_unavailable_identity` (CLI): missing/mixed scopes, timeouts, offline, unauthorized and signed-out cases remain usable and unprivileged.
- **Dimension 4.3** — Invocation ownership is retained → Test `discovery_does_not_reuse_another_identity_or_target` (CLI): environment key wins; account/API switches cannot reuse an elevated result; only one bounded identity read is made.
- **Dimension 4.4** — Exact operations retain backend refusal → Test `hidden_command_and_stale_completion_cannot_bypass_authorization` (CLI/integration): directly typed commands still reach existing checks and revoked operations fail under the server's current cache policy.
- **Dimension 4.5** — All discovery surfaces agree → Test `scoped_help_suggestions_and_four_shell_completions_agree` (CLI): root/nested help, suggestions and generated scripts have the same retained operations; JSON output remains clean.

### §5 — Coordinated contracts and merge

Update generated OpenAPI, client adapters, permissions, help, examples and public documentation together. Rebase onto the merged UI refresh before editing its callers. Re-run native setup/Plugin and lifecycle journeys using the refreshed interface; preserve the design system and assistant-ui runtime.

Use an exact-base baseline for test counts. Run required repository lanes and reviews. Resolve CI and Greptile, including two quiet green polls. The owner merges after the UI PR has merged; do not merge independently.

- **Dimension 5.1** — All callers use the current contract → Test `plugin_contract_matches_cli_ui_and_openapi` (integration/browser): create/revise/reuse/delete and failure recovery work under the new paths and payload names.
- **Dimension 5.2** — The release has one vocabulary → Test `removed_public_spellings_have_only_historical_references` (source/docs): old public spellings appear only in explicit history/removal notes; no live examples or generated aliases survive.

## Interfaces

Root groups: `login`, `logout`, `whoami`, `doctor`, `workspace`, `chat`, `fleet`, `plugins`, `models`, `secrets`, `connections`, `api-keys`, `usage`, plus scope-advertised `platform`.

Example: `agentsfleet chat --plugin <plugin_id> --revision <revision_id>` creates a setup from a complete saved version. `agentsfleet fleet steer <fleet_id> "Check the release"` requests runtime work. Existing argument spelling/cardinality must be inventoried before changing examples.

Fleet-scoped approvals, grants and schedules require the target fleet. Account API keys remain `/v1/api-keys`; platform defaults remain `/v1/admin/platform-keys`. Grouping commands does not itself rename unrelated HTTP routes. No new identity endpoint or backend permission family is needed for discovery.

## Failure Modes

| Mode | Kind | Handling | Test |
|---|---|---|---|
| Scope read unknown or malformed | Security | Hide platform advertising; preserve explicit-operation authorization | `discovery_refuses_malformed_expired_and_unavailable_identity` |
| Endpoint/identity changes | Security | Discard invocation state; never reuse elevated results | `discovery_does_not_reuse_another_identity_or_target` |
| Root parser/help diverge | Failure | Build from the same domain factories | `canonical_groups_share_help_and_handlers` |
| Delete partially succeeds | Failure | Retain confirmed stop and retry only deletion | `fleet_delete_retains_confirmed_archive_checkpoint` |
| Plugin write conflicts or access is lost | Security | Preserve ETags and scoped refusal; do not alter installed copies | `plugin_revision_and_installed_copy_do_not_follow_updates` |
| Generated completion becomes stale | Failure | Document regeneration; backend refuses unauthorized calls | `hidden_command_and_stale_completion_cannot_bypass_authorization` |

## Invariants

1. Backend authorization is authoritative; hidden/visible help grants no access.
2. Credential bytes stay out of argv, diagnostics, discovery state and reports.
3. Plugins remain complete setups; revisions and installed copies do not follow later edits.
4. Creator-private setup history does not become account-wide transcript data.
5. Model keys remain workspace-backed; no primary vault or implicit replacement.
6. No compatibility aliases, backfill, dual-read path or guarded development cutover.
7. The UI refresh merges before this workstream; starting CLI work is independent.

## Metrics & Observability

N/A: no new product events. Preserve existing command instrumentation, request IDs and coarse outcomes. Permission-discovery diagnostics contain no raw credentials or identity payloads.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | CLI behavior and discovery (§1–§4) | `make test-unit-cli` | exit 0, scope/ownership/completion proofs pass | P0 | |
| R2 | All package and Rust unit gates | `make test-unit-all` | exit 0, existing coverage floors retained | P0 | |
| R3 | Native API scope and lifecycle (§2–§4) | `make test-integration-rustd` | exit 0 | P0 | |
| R4 | Refreshed callers (§5) | `make dry-app` | exit 0 | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Version | `make check-version` | exit 0 | P0 | |
| S3 | Work/verify/PR gates | `orly gate work`; `orly gate verify`; `orly gate pr` | exit 0 on the appropriate committed/pushed state | P0 | |
| S4 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

Delete `root.command.ts`, retired platform catalogue command/route entries and obsolete public aliases after callers are updated. Search executable source, generated OpenAPI, completions, documentation and examples. Keep historical records readable; do not rewrite old changelogs as if removed spellings never existed.

## Out of Scope

- Flow 2 capability composition, `@plugin` invocation or plugin-to-fleet bindings; see M239's deferred discussion.
- M239 fleet-to-fleet steering/activity implementation.
- The separate backend data-completeness work for the refreshed UI, except coordinated caller/path changes.
- New platform settings UI, personal-only Plugin ownership, sharing or a new ACL model.
- Readable environment-variable resources, real upload storage/analysis, a second chat runtime or speculative virtualization.

## Product Clarity (authoring record)

1. **Successful user moment** — A user finds the right fleet command; an operator sees only appropriate platform choices.
2. **Preserved user behaviour** — Existing handlers, scopes, private history, immutable setups and recovery remain authoritative.
3. **Optimal-way check** — Effect's existing command tree and backend identity contract cover the need; no second parser or identity-provider client.
4. **Rebuild-vs-iterate** — Refactor assembly and vocabulary; reuse transport and resource semantics.
5. **What we build** — Domain groups, coordinated Plugin vocabulary, scope-aware discovery and four-shell generation.
6. **What we do NOT build** — Capability attachment, a new permission system, new platform UI or guarded rollout machinery.
7. **Fit with existing features** — Reuse fleet lifecycle, draft review/publication and platform-key management.
8. **Surface order** — CLI work starts in parallel; coordinate public API/UI caller changes after the UI refresh merges.
9. **Dashboard restraint** — Keep the approved UI; update its data adapters instead of rebuilding its layout.
10. **Confused-user next step** — Canonical help suggests current commands; exact restricted help explains the required right.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Five slices separate composition, resource vocabulary, lifecycle, discovery and contract validation.
- **Alternatives considered:** Text-filtered help diverges from parsing/completions; a new parser duplicates Effect; a module-global mutable tree risks carrying authority between invocations.
- **Patch-vs-refactor verdict:** A refactor of assembly, with targeted contract changes; existing resource implementations remain in place.

## Discovery (consult log)

- **Consults:** Source inspected at local `3aa7e368b`, with current `origin/main` fetched at `4f3159269`. Re-inventory the CLI and pinned Effect APIs when implementation starts.
- **Model reviews:** This is a consolidated pending implementation spec; fresh EGM review is still required before implementation is declared ready. No review result is implied by this draft.
- **Open blockers:** None to starting independent CLI planning/work. M249_001 merge is a merge-order prerequisite, and exact affected files must be inventoried before edits.
- **Deferrals and transfers:** Flow 2 remains deferred in M239; current development work does not need legacy migration machinery.

> Owner (Indy, Nov 12, 2026): "2, 3 can be one spec pushed in your PR. claude will need a handoff prompt to start the spec and create work tree from your merged PR1 by a separate agent. For 2,3 commit in the PR you do for 1, so upon merge we pull and start on this."

> Owner clarification (Indy, Nov 12, 2026): "well if the cli is discrete, then it could start before your merged UI PR. But i will merge the cli after PR merges. so they go in parallel."

> Owner decisions: "Yes flow 1"; "Permanent stop, retain history, no Restore"; "there is no primary vault"; "i dont need a guarded cutover"; "i am only in development".
