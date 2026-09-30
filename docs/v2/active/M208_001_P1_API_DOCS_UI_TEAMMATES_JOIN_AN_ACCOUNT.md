<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M208_001: An owner invites a teammate into the account, who then opens and steers its fleets within a member's bounds

**Prototype:** v2.0.0
**Milestone:** M208
**Workstream:** 001
**Date:** Sep 30, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — nobody but a workspace's creator can open it today; a team cannot share one fleet
**Categories:** API, DOCS, UI
**Batch:** B1 — first of three M208 workstreams; all three ship in one Pull Request (PR) (Indy, Sep 30, 2026)
**Branch:** `feat/m208-team-accounts`
**Baseline revision:** `f90ed13159ff5a405b28ce3fdbe0e6ab17c4a012`
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none (M207_005 merged: `614813f12`)
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 30, 2026); decisions in Discovery are Indy's
**Canonical architecture:** `docs/AUTH.md` §Scopes and §Signup

---

## Overview

**Goal (testable):** `test_invited_member_opens_and_steers_owner_fleet` — John invites Bob; Bob accepts, opens John's workspace, reads and steers MARY-001, and is refused writing John's secrets or connectors and managing his members.
**Problem:** Workspace access is `core.users.tenant_id` equality (`rustd/crates/afd_tenant/src/sql/workspace.rs:38-44`), so only a workspace's creator can open it; `core.memberships` exists but no access check reads it. Platform operators holding `workspace:any` reach any workspace through the API (`rustd/crates/afd_tenant/src/workspace/mod.rs:104-149`), but the dashboard cannot find one, and the one scope grants read and write together.
**Solution summary:** Access resolves through memberships with two roles, owner and member. Owners invite by email, list and revoke invites, and remove members; an invitee accepts from the dashboard. `workspace:any` splits into `workspace-any:read` and `workspace-any:write`, a platform directory lists every workspace, each crossing is audited, and read-only access renders without write controls. §1 lists the one credential M208 needs, for M208_003's invite email.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat: invite teammates to an account and show every message live (the M208 PR)
- **Intent (one sentence):** a team works one account's fleets together, each person within their role.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_tenant/src/workspace/mod.rs` — the one access decision (`authorize`, `owner_matching`, `cross_tenant_override`) this spec changes.
2. `rustd/crates/afd_http/src/auth/ownership.rs` — the layer every workspace route passes; the role gate and the read-only override land here, not in handlers.
3. `docs/AUTH.md` — scopes, provisioning at Clerk, signup's five rows; this spec edits it.
4. `rustd/crates/afd_tenant/src/signup.rs` — the one transactional membership write; accepting an invite mirrors it.
5. `playbooks/founding/02_preflight/001_playbook.md` — the platform credential list §1 extends.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `playbooks/founding/02_preflight/{001_playbook.md,credentials_test.sh}` | EDIT | list `resend-app`; report it missing by name |
| `schema/923_workspace_invites.sql` | CREATE | `core.invites`: tenant, email, role, inviter, expiry, accepted/revoked, and the email-status columns M208_003 writes |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | register 923 |
| `rustd/crates/afd_tenant/src/sql/workspace.rs` | EDIT | access through memberships; answers tenant and role |
| `rustd/crates/afd_tenant/src/workspace/mod.rs` | EDIT | access record: role and via (membership, platform read, platform write) |
| `rustd/crates/afd_tenant/src/invite/` | CREATE | create, list, revoke, accept (one transaction), expiry |
| `rustd/crates/afd_tenant/src/member/` | CREATE | list members, remove member, last-owner guard |
| `rustd/crates/afd_auth/src/scope.rs` | EDIT | `workspace:any` → `workspace-any:read` + `workspace-any:write` |
| `rustd/crates/afd_http/src/auth/ownership.rs` | EDIT | role gate; read-only crossing admits safe methods only; audit fields |
| `rustd/crates/afd_http/src/services/tenant.rs` | EDIT | `WorkspaceOwnership` returns the access record |
| `rustd/crates/afd_http/src/route/{tenant,workspace,admin}.rs` | EDIT | invite, member, directory and access routes |
| `rustd/crates/afd_api_tenant/src/handler/tenant/{invite,member}.rs` | CREATE | owner and invitee handlers |
| `rustd/crates/afd_api_tenant/src/handler/tenant/workspace.rs` | EDIT | list spans memberships; detail carries `access` |
| `rustd/crates/afd_api_tenant/src/handler/stream.rs` | EDIT | re-authorize open streams on a bounded cadence |
| `rustd/crates/afd_api_tenant/src/handler/admin/workspaces.rs` | CREATE | platform directory |
| `rustd/crates/afd_core/src/error_code/{auth,invite}.rs` | EDIT/CREATE | `UZ-AUTH-025`, `UZ-AUTH-026`, `UZ-INV-001`…`004` |
| `public/openapi.json` | EDIT | new routes and `access` |
| `ui/packages/app/lib/api/{tenant-members,invites,admin-workspaces}.ts` | CREATE | clients |
| `ui/packages/app/components/layout/WorkspaceSwitcher*.tsx` | EDIT | workspaces grouped by account |
| `ui/packages/app/components/layout/PlatformAccessBanner.tsx` | CREATE | "Viewing as platform operator: read only / can act" |
| `ui/packages/app/components/domain/FleetThread.tsx` | EDIT | no composer when `can_write` is false |
| `ui/packages/app/app/(dashboard)/settings/members/` | CREATE | owner: invite, copy link, pending invites, members, remove |
| `ui/packages/app/app/(dashboard)/invites/` | CREATE | invitee: accept or decline |
| `ui/packages/app/app/(dashboard)/admin/workspaces/` | CREATE | platform directory |
| `docs/AUTH.md` | EDIT | memberships, roles, scope split, operator bundle |
| `~/Projects/docs` (branch `chore/m208-team-accounts-changelog`) | EDIT | members and invites pages, changelog `<Update>` |
| `rustd/crates/{afd_crypto,afd_tenant,afd_fleet_lifecycle,afd_vault,afd_admin,afd_admission,afd_approval,afd_billing,afd_connector,afd_cron,afd_credential,afd_dragonfly,afd_fleet,afd_gate,afd_http,afd_ingress,afd_library,afd_runner}/{src,tests}/**` | EDIT | hand-rolled Rust cleanup Indy asked for in session (Discovery): one identifier mint, sqlx's unique-violation check, `error_lifts!`, dead kinds removed |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (role, via and scope strings as named constants), NDC and NLR (retire `workspace:any` with no alias), NLG, ORP (orphan sweep for the old scope), FLL (split before 350 lines), ECL (an outage never answers "not a member").
- `docs/REST_API_DESIGN_GUIDELINES.md` — new routes, six-place registration, problem bodies, 403 parity with `UZ-AUTH-001`.
- `docs/SCHEMA_CONVENTIONS.md` and the static-strings rule — `role` and `email_status` stay `TEXT` with no `CHECK`; vocabularies live in constants, overriding the "gains a constraint" note in `schema/230_memberships.sql`.
- `docs/RUST_ERROR_STANDARD.md` — new modules declare `ErrorKind` behind `error_shell!`.
- `docs/LOGGING_STANDARD.md` — audit and invite events carry `event` and ids, never an email in clear.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — new table | additive migration only; no `DROP`/`ALTER` on shipped tables |
| ERROR REGISTRY | yes — six codes | declared in `afd_core::error_code`, each with a negative test |
| UFS | yes | constants for `owner`, `member`, via kinds, scope strings |
| LOGGING | yes | structured events per `docs/LOGGING_STANDARD.md` |
| UI / DESIGN TOKEN | yes — three pages, banner | design-system primitives and token utilities only |
| File & Function Length (≤350/≤50/≤70) | yes | invite and member logic in their own modules; `ownership.rs` gains a sibling before 300 lines |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_tenant/src/signup.rs` — the transactional membership write accept mirrors.
- **Reference:** `cross_tenant_override` — the audit-before-honour shape the read/write split keeps.
- **Reference:** Sentry `is_global` (`docs/AUTH.md:165`) — Sentry separates superuser read from superuser write the same way.

## Sections (implementation slices)

### §1 — Milestone credential enumeration (credential gate)

M208 needs one external credential, for M208_003: the `resend-app` bag `{api_key, from_address, api_base?}` in the admin-workspace vault, the `<provider>-app` convention `slack-app` and `github-app` use. Source of record: the 1Password item `resend-app`, created by Indy with M208_003's registration playbook after verifying `agentsfleet.net` at Resend, and loaded with `playbooks/lib/platform_secret_sync.sh resend-app`. M208_001 and M208_002 ship with zero new credentials.

- **Dimension 1.1** — the preflight lists `resend-app` and fails naming it when absent → Test `test_preflight_names_missing_resend_app`

### §2 — Access resolves through memberships, with two roles

The access check answers `{tenant, role, via}`: a `core.memberships` row for the caller's user in the workspace's tenant grants access with that row's role. Tenant API keys and CLI credentials keep resolving to their own tenant only. A member is refused, with `UZ-AUTH-025`, where the route's required capability for the request's method is `secret:write` or `connector:write`, the only owner-grade capabilities on workspace routes (`rustd/crates/afd_http/src/route/{workspace,connector}.rs`); the capability gate still applies to everyone. The workspace list spans every membership, and each item names its account and the caller's role. Open streams re-run the check every 60 s (**Implementation default:** heartbeat-aligned, since the stream wakes then) and end with a problem frame when access is gone.

- **Dimension 2.1** — a member opens, lists, streams and steers the owner's workspace → Test `test_member_reaches_owner_workspace`
- **Dimension 2.2** — a non-member still gets `403 UZ-AUTH-001`, byte-identical to today → Test `test_non_member_refused_unchanged`
- **Dimension 2.3** — a member is refused every secret-write and connector-write route with `UZ-AUTH-025` → Test `test_member_refused_owner_only_routes`
- **Dimension 2.4** — a removed member's open stream ends within 60 s → Test `test_removed_member_stream_ends`
- **Dimension 2.5** — a datastore failure during the check is `503`, never a denial → Test `test_access_check_outage_is_not_denial`
- **Dimension 2.6** — an account with no invites behaves exactly as today → Test `test_single_owner_paths_unchanged`

### §3 — Owners invite; invitees accept

An owner invites an email (lowercased) as `member`; one pending invite per `(tenant, email)`; invites expire after 7 days. The invitee sees pending invites for their account email and accepts: the membership insert and the invite's `accepted_at` commit in one transaction, and accepting twice is a no-op. An owner lists members, lists and revokes invites, and removes a member; the last owner is never removed. The create response carries the accept `link` (`{dashboard}/invites/{invite_id}`, from `services.dashboard()`).

- **Dimension 3.1** — create, list, revoke by an owner → Test `test_owner_manages_invites`
- **Dimension 3.2** — accept by the matching account email creates one member row → Test `test_invitee_accepts_once`
- **Dimension 3.3** — a different email is `403 UZ-INV-002`; expired or revoked is `404 UZ-INV-001` → Test `test_accept_refusals`
- **Dimension 3.4** — a duplicate pending invite or existing member is `409 UZ-INV-003`; removing the last owner is `409 UZ-INV-004` → Test `test_invite_and_member_conflicts`

### §4 — The dashboard: members, invites, the account-grouped switcher

Owners get Settings → Members (invite, copy link, pending invites, members, remove). Invitees get an Invites page and a one-line notice while any invite is pending. The workspace switcher groups by account ("Yours", "John's account").

- **Dimension 4.1** — an owner invites, copies the link and removes a member on the page → Test `test_members_page_owner_journey`
- **Dimension 4.2** — an invitee accepts and the workspace appears under the owner's account → Test `test_invitee_accept_journey`

### §5 — Platform operators: find any workspace; read, and act when granted

`workspace-any:read` admits only `GET` and `HEAD` across tenants; `workspace-any:write` admits every method. Each crossing emits `cross_tenant_workspace_override` before it is honoured, now with `access=read|write` and the method. `GET /v1/admin/workspaces` lists every workspace for either scope. The workspace detail's `access` drives the banner and, for read, hides every write control.

- **Dimension 5.1** — a read-only operator streams and reads another tenant's fleet; any write is `403 UZ-AUTH-026` → Test `test_platform_read_is_read_only`
- **Dimension 5.2** — a write operator steers another tenant's fleet, attributed to the operator → Test `test_platform_write_acts_attributed`
- **Dimension 5.3** — every crossing logs one audit event with access and method → Test `test_platform_crossing_audited`
- **Dimension 5.4** — the directory serves either scope and refuses others → Test `test_admin_directory_scoped`
- **Dimension 5.5** — read-only access shows the banner and no write control → Test `test_read_only_banner_hides_controls`

### §6 — Documentation

`docs/AUTH.md` (memberships, roles, scope split, operator bundle); public members and invites pages and the changelog on the docs branch.

- **Dimension 6.1** — no page in `docs/` or `public/openapi.json` names `workspace:any` → Test `test_docs_name_scope_split`

## Interfaces

```
POST   /v1/tenants/me/invites            {email}   -> 201 {id, email, role:"member", expires_at, link}
GET    /v1/tenants/me/invites                      -> 200 {items:[...]}
DELETE /v1/tenants/me/invites/{invite_id}          -> 204
GET    /v1/me/invites                              -> 200 {items:[{id, account:{owner_name}, expires_at}]}
POST   /v1/me/invites/{invite_id}/accept           -> 200 {workspace_ids:[...]}
GET    /v1/tenants/me/members                      -> 200 {items:[{user_id, display_name, email, role}]}
DELETE /v1/tenants/me/members/{user_id}            -> 204
GET    /v1/workspaces/{workspace_id}/members       -> 200 {items:[{user_id, display_name, role}]}
GET    /v1/tenants/me/workspaces   items gain {account:{tenant_id, owner_name}, role}
GET    /v1/workspaces/{workspace_id}   gains access:{role|null, via:"membership"|"platform_read"|"platform_write", can_write}
GET    /v1/admin/workspaces?q=&cursor=              -> 200 {items:[{id, name, owner_name, fleet_count}], next_cursor}
Errors: UZ-AUTH-025 role refused (403) · UZ-AUTH-026 read-only platform access (403)
        UZ-INV-001 not found/expired/revoked (404) · UZ-INV-002 email mismatch (403)
        UZ-INV-003 already pending or member (409) · UZ-INV-004 last owner (409)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Link reaches a third party | forwarded invite | accept requires the signed-in account's email to equal the invite's: `403 UZ-INV-002` |
| Concurrent accept | two tabs accept at once | unique `(tenant_id, user_id)`; the second returns the same body |
| Member removed mid-stream | owner removes Bob while he watches | problem frame within 60 s; next request `403 UZ-AUTH-001` |
| Access-check outage | Postgres unavailable | `503` with the outage code, never `403`; a failed stream re-check keeps the stream until the next pass |
| Read-only operator writes | non-safe method with `workspace-any:read` | `403 UZ-AUTH-026`; the audit event is still emitted |
| Last owner removal | sole owner removes self | `409 UZ-INV-004` |

## Invariants

1. Access is decided once, in `ownership.rs`, for every workspace route — mounted from the route template (existing); the role gate reads the same access record.
2. A member never exceeds their own Clerk scopes — the capability gate runs before ownership (existing order), and roles only subtract.
3. Membership insert and invite acceptance commit together — one transaction; a unit test fails the second statement and asserts no member row.
4. `workspace-any:read` never admits a non-safe method — the override matches against a constant method set, tested across every mounted workspace route.
5. No crossing is honoured before its audit event — emitted first (existing order), asserted by a test that fails the emit.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `workspace_invite_created` | product | owner creates an invite | tenant id, invite id, role | no email logged | `test_owner_manages_invites` |
| `workspace_invite_accepted` | product | invitee accepts | tenant id, invite id, user id | no email | `test_invitee_accepts_once` |
| `workspace_member_removed` | ops | owner removes a member | tenant id, user id | no email | `test_invite_and_member_conflicts` |
| `cross_tenant_workspace_override` | ops | platform crossing | operator id and tenant, target tenant, workspace, access, method | no request body | `test_platform_crossing_audited` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_preflight_names_missing_resend_app` | no `resend-app` item → preflight exits non-zero naming `resend-app` |
| 2.1 | integration | `test_member_reaches_owner_workspace` | membership row → detail, list, stream and steer 2xx |
| 2.2 | integration | `test_non_member_refused_unchanged` | no row → `403 UZ-AUTH-001`, body equal to today's |
| 2.3 | integration | `test_member_refused_owner_only_routes` | member on every secret-write and connector-write route → `403 UZ-AUTH-025`; their reads 2xx |
| 2.4 | integration | `test_removed_member_stream_ends` | removal during an open stream → problem frame within one re-check |
| 2.5 | unit | `test_access_check_outage_is_not_denial` | query error → `503`, never `403` |
| 2.6 | integration | `test_single_owner_paths_unchanged` | no invites → list, detail, stream, steer as before |
| 3.1 | integration | `test_owner_manages_invites` | create → listed → revoke → list empty |
| 3.2 | integration | `test_invitee_accepts_once` | accept twice → one membership, same body |
| 3.3 | integration | `test_accept_refusals` | other email → `UZ-INV-002`; expired, revoked → `UZ-INV-001` |
| 3.4 | integration | `test_invite_and_member_conflicts` | duplicate, existing member → `UZ-INV-003`; last owner → `UZ-INV-004` |
| 4.1 | e2e | `test_members_page_owner_journey` | invite, copy link, remove on the rendered page |
| 4.2 | e2e | `test_invitee_accept_journey` | accept → switcher shows the workspace under the owner's account |
| 5.1 | integration | `test_platform_read_is_read_only` | read scope: every mounted `GET` 2xx, every other method `403 UZ-AUTH-026` |
| 5.2 | integration | `test_platform_write_acts_attributed` | write scope steer → row actor `steer:<operator>` |
| 5.3 | unit | `test_platform_crossing_audited` | crossing → one event with access and method, before the handler |
| 5.4 | integration | `test_admin_directory_scoped` | either scope → all tenants' workspaces; neither → `403 UZ-AUTH-022` |
| 5.5 | e2e | `test_read_only_banner_hides_controls` | `can_write:false` → banner, no composer, no write button |
| 6.1 | unit | `test_docs_name_scope_split` | grep of `docs/` and `public/openapi.json` → no `workspace:any` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Members reach and are bounded (§2, §3) | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | Invite and accept journeys (§4) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts --project=journeys -g "test_invitee_accept_journey\|test_members_page_owner_journey"` | `2 passed` | P0 | |
| R3 | Operator read is read-only, write is audited (§5) | `cd rustd && cargo test -p afd_http --all-features platform_` | exit 0 | P0 | |
| R4 | Old scope gone | `git grep -n "workspace:any" -- . ':!docs/v2/done'` | 0 matches | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the three M208 Files Changed tables | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Versions in sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. `make test-integration-rustd` is R1. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ needs an Indy-acked deferral quote in Discovery; a P0 may be **MOVED** only into a named successor spec that carries the row as its own P0, recorded in both, with Indy's verbatim quote, and a MOVED row is never ✅.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `Scope::WorkspaceAny` | `git grep -n "WorkspaceAny" rustd/` | 0 matches |
| `workspace:any` | `git grep -n "workspace:any" -- . ':!docs/v2/done'` | 0 matches |

## Out of Scope

- A viewer (watch-only) role — owner and member only (Indy, Sep 30, 2026).
- Per-workspace invites — an invite grants the whole account (Indy, Sep 30, 2026).
- API keys and CLI credentials reaching an invited account — they stay bound to their own tenant.
- Transferring ownership or multiple owners — the account creator stays the one owner.
- Live messages and sender names — M208_002. Invite email — M208_003.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Bob accepts John's invite, finds MARY-001 under "John's account" in the switcher, opens it and steers it.
2. **Preserved user behaviour** — a solo account works exactly as today: same list, refusals, stream and steer; API keys and the CLI unchanged.
3. **Optimal-way check** — the optimal shape adds a viewer role and per-workspace grants; neither blocks a two-person team, and both extend the same access record later.
4. **Rebuild-vs-iterate** — iterate: `core.memberships` was shaped for teams at signup; access and roles extend one existing seam.
5. **What we build** — membership access with two roles, invites, members and invites pages, grouped switcher, the split operator scope, directory and banner.
6. **What we do NOT build** — viewer role, per-workspace invites, cross-account API keys, ownership transfer.
7. **Fit with existing features** — the steer path, pending-sends ledger and chat hold are unchanged; M208_002 names the senders this creates.
8. **Surface order** — User Interface (UI) first: inviting and accepting are dashboard acts; the API is public and documented; the CLI follows later.
9. **Dashboard restraint** — no Members entry for a member, no invite notice without a pending invite, no directory without a `workspace-any` scope, no write control on read-only access.
10. **Confused-user next step** — a refused accept names the account email it expects; a member refused an owner action reads "Only the account owner can do this."

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** M208 is three workstreams in one Pull Request — 001 access (the core the other two need), 002 live messages, 003 invite email — because one file cannot hold all three under the 320-line cap.
- **Alternatives considered:** per-workspace memberships (Indy chose account-level); Clerk Organizations as the membership authority (rejected: a second authority beside `core.memberships`, while scopes already resolve per person).
- **Patch-vs-refactor verdict:** this is a **refactor** of the access decision: every workspace route moves from one column to a relation, kept to one layer.

## Discovery (consult log)

- **Consults** — Sep 30, 2026, Indy via AskUserQuestion: "One spec, one PR" (carried as three workstream specs in one PR because of the 320-line cap); an invite grants "John's whole account"; roles "Owner + member"; operators "See and act, audited, but must allow adding write or act as well, need to know the design of this" (§5's scope split). Then: "I want the invite emails into an account" and a send-time broadcast "is also a must have" (M208_003, M208_002); email provider "Resend (Recommended)". Source finding: access by `core.users.tenant_id` (`afd_tenant/src/sql/workspace.rs:38-44`).
- **Metrics review** — pending.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
- **Hand-rolled Rust cleanup** — Sep 30, 2026, Indy in session: "Are there any handrolled rust code? where you can use the afd_core or external crates, if yes fix them", then "Are there any duplicate handrolled code you have, if yes fix them." and "and clean it up". Its own commit on this branch: `Entropy::uuid7` replaces every draw-then-encode copy, `sqlx::error::DatabaseError::is_unique_violation` replaces the hand-written `23505` checks, `afd_tenant` lifts through `error_lifts!`, the identifier kinds that became dead are removed, and `DETAIL_NOT_DASHBOARD` drops the identity vendor's name.
