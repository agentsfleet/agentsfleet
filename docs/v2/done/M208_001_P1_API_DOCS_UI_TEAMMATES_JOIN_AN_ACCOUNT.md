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
**Status:** DONE
**Priority:** P1 — nobody but a workspace's creator can open it today; a team cannot share one fleet
**Categories:** API, DOCS, UI
**Batch:** B1 — first of three M208 workstreams; all three ship in one Pull Request (PR) (Indy, Sep 30, 2026)
**Branch:** `feat/m208-team-accounts`
**Baseline revision:** `3b61121c3c7da8b97cc348cca1d1dbfb99c3bce4`
**Test Baseline:** unit=2884 integration=683 — at `3b61121c3`: unit 2884 passed / 0 failed / 708 ignored (`make test-unit-all`, Rust half; TypeScript 3308 + 142 + 640 = 4090; Zig runner passed) · integration 683 passed / 0 failed (`make test-integration-rustd`, 681 + 2 exclusive). Final counts land at CHORE(close).
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M208_001-3b61121c3.md`
**Depends on:** none (M207_005 merged: `614813f12`)
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 30, 2026); decisions in Discovery are Indy's
**Canonical architecture:** `docs/AUTH.md` §Scopes, §Signup and §Memberships and roles

---

## Overview

**Goal (testable):** `test_invited_member_opens_and_steers_owner_fleet` — John invites Bob; Bob accepts, opens John's workspace, reads and steers MARY-001, and is refused writing John's secrets or connectors and managing his members.
**Problem:** Workspace access is `core.users.tenant_id` equality (`rustd/crates/afd_tenant/src/sql/workspace.rs:38-44`), so only a workspace's creator can open it; `core.memberships` exists but no access check reads it. Platform operators holding `workspace:any` reach any workspace through the API (`rustd/crates/afd_tenant/src/workspace/mod.rs:104-149`), but the audit event cannot tell a look from an act.
**Solution summary:** Access resolves through memberships with two roles, owner and member. Owners invite by email, list and revoke invites, and remove members; an invitee accepts from the dashboard. `workspace:any` keeps its read-and-write crossing, and each crossing is audited with its method. §1 lists the one credential M208 needs, for M208_003's invite email.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat: invite teammates to an account and show every message live (the M208 PR)
- **Intent (one sentence):** a team works one account's fleets together, each person within their role.
- **Handshake** — Sep 30, 2026, stated to Indy before EXECUTE: a team shares one account; an owner invites by email, the invitee reads and steers that account's fleets, and only the owner writes secrets and connectors or manages members. Assumptions stated: claim-bound credentials act as owner of their own account only; `UZ-AUTH-025` is taken, so the role refusal is `UZ-AUTH-026` (a read-only platform code, `UZ-AUTH-027`, was planned and later dropped, Discovery); the directory is an admin route served by the operator plane; streams re-check on their existing beats; the operator's identity-provider scopes are re-provisioned before deploy. Indy's reply added: "Ensure the query is performant" (Dimension 2.7).

## Implementing agent — read these first

1. `rustd/crates/afd_tenant/src/workspace/mod.rs` — the one access decision (`authorize`, `owner_matching`, `cross_tenant_override`) this spec changes.
2. `rustd/crates/afd_http/src/auth/ownership.rs` — the layer every workspace route passes; the role gate and the crossing audit land here, not in handlers.
3. `docs/AUTH.md` — scopes, provisioning at Clerk, signup's five rows; this spec edits it.
4. `rustd/crates/afd_tenant/src/signup.rs` — the one transactional membership write; accepting an invite mirrors it.
5. `playbooks/founding/02_preflight/001_playbook.md` — the platform credential list §1 extends.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `playbooks/founding/02_preflight/{001_playbook.md,credentials_test.sh}` | EDIT | list `smtp-relay` among the post-deploy outputs; prove the early gates never read it |
| `schema/923_workspace_invites.sql` | CREATE | `core.invites`: tenant, email, role, inviter, expiry, accepted/revoked, the email-status columns M208_003 writes; a partial index on pending invites by email |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | register 923 |
| `rustd/crates/afd_tenant/src/sql/workspace.rs` | EDIT | access through memberships; answers tenant and role |
| `rustd/crates/afd_tenant/src/workspace/mod.rs` | EDIT | access record: tenant and grant; the crossing event moves to the ownership layer |
| `rustd/crates/afd_tenant/src/{team/,sql/{invite,member}.rs,lib.rs}` | CREATE/EDIT | one `Team` store: `team/invitation/` holds the `Invitation` and its acceptance rule, and its lifecycle (issue, list, revoke, waiting, accept in one transaction); members (list, remove, last-owner guard) |
| `rustd/crates/afd_tenant/src/workspace/{access,accounts,crossing}.rs`, `src/sql/mod.rs`, `src/error/*`, `Cargo.toml` | CREATE/EDIT | the access record, the accounts a caller holds, the crossing audit, the refusals; the trace capture as a dev-dependency |
| `rustd/crates/afd_api_tenant/src/handler/connector/callback.rs` | EDIT | the callback authorizes outside the layer, so it records a crossing itself |
| `rustd/crates/afd_http/src/auth/ownership/{role,extract}.rs` | CREATE | the member rule; the extractors, split out at the file cap |
| `rustd/crates/afd_sse/src/{frame,lib}.rs`, `afd_api_tenant/src/handler/stream/{guard,wall}.rs` | CREATE/EDIT | streams re-check access and end on `access_revoked` |
| `rustd/crates/afd_api_tenant/src/handler/tenant/workspace/{input,render}.rs`, `afd_wire/src/workspace.rs` | CREATE/EDIT | list items carry `account` and `role`; the handler split at the file cap |
| `rustd/crates/afd_core/src/{error_code,problem}.rs` | EDIT | the registry lists the new codes and the invite family |
| `rustd/crates/{afd_api_tenant/src/handler/auth/session,afd_api_runner/src/handler/runner/enrolment}.rs` | EDIT | published descriptions name no identity vendor |
| `rustd/crates/{afd_api,afd_tenant}/tests/**` | CREATE/EDIT | the access suites; the harness decides from real rows on request; an uncompiled harness file removed |
| `rustd/crates/afd_http/src/auth/ownership.rs` | EDIT | role gate; the crossing audit event, with the method, before the handler |
| `rustd/crates/afd_api/src/router/mod.rs` | EDIT | the ownership layer receives the route's scopes, which the role gate reads |
| `rustd/crates/afd_http/src/services/tenant.rs` | EDIT | `WorkspaceOwnership` returns the access record |
| `rustd/crates/afd_http/src/route/{tenant,workspace}.rs` | EDIT | invite, member and access routes |
| `rustd/crates/afd_api_tenant/{Cargo.toml,src/lib.rs,src/openapi.rs,src/handler/tenant/{mod,invite,member}.rs}`, `rustd/Cargo.lock` | CREATE/EDIT | owner and invitee handlers, registered and documented |
| `rustd/crates/{afd_wire/src/{lib,team}.rs,afd_http/src/{openapi.rs,openapi/path.rs,services/{mod,tenant_surface,team}.rs},agentsfleetd/src/{plane.rs,plane/services.rs}}` | CREATE/EDIT | invite and member bodies; the `Team` service wired into the tenant plane and its OpenAPI document |
| `rustd/crates/afd_api_tenant/src/handler/tenant/workspace.rs` | EDIT | list spans memberships |
| `rustd/crates/afd_api_tenant/src/handler/stream.rs` | EDIT | re-authorize open streams on a bounded cadence |
| `rustd/crates/afd_core/src/{error_code,problem}/{auth,invite}.rs` | EDIT/CREATE | `UZ-AUTH-026`, `UZ-INV-001`…`004`, with their statuses |
| `public/openapi.json` | EDIT | new routes |
| `ui/packages/app/lib/api/{tenant-members,invites,decode,workspaces}.ts` | CREATE/EDIT | clients; workspace items decode `account` and `role` |
| `ui/packages/app/components/layout/{WorkspaceSwitcher*.tsx,workspace-groups.ts,InviteNotice.tsx,SidebarNavigation.tsx,ShellFrame.tsx}`, `app/(dashboard)/layout.tsx` | CREATE/EDIT | workspaces grouped by account; the Members entry; the pending-invite notice |
| `ui/packages/app/{tests,lib,components,app}/**/*.test.{ts,tsx}`, `tests/e2e/acceptance/{team-members.spec.ts,global-teardown.ts}` | CREATE/EDIT | unit suites; the two journeys; the sweep reaps a leaked invitee |
| `ui/packages/app/app/(dashboard)/settings/members/`, `components/domain/island-dynamic/InviteDialogDynamic.tsx` | CREATE | owner: one table of people and pending invites, an Invite dialog behind a dynamic shim, copy link, revoke, remove |
| `ui/packages/app/app/(dashboard)/invites/` | CREATE | invitee: accept or decline |
| `docs/AUTH.md` | EDIT | memberships, the two roles, the owner-only refusal |
| `~/Projects/docs` (branch `chore/m208-team-accounts-changelog`) | EDIT | members and invites pages; no changelog `<Update>` (Discovery) |
| `rustd/crates/afd_db/src/{lib,constraint}.rs` | CREATE/EDIT | one named-constraint check (`violates_unique`) for the three callers that each wrote it |
| `rustd/crates/{afd_crypto,afd_tenant,afd_fleet_lifecycle,afd_vault,afd_admin,afd_admission,afd_approval,afd_billing,afd_connector,afd_cron,afd_credential,afd_dragonfly,afd_fleet,afd_gate,afd_http,afd_ingress,afd_library,afd_runner}/{src,tests}/**` | EDIT | hand-rolled Rust cleanup Indy asked for in session (Discovery): one identifier mint, sqlx's unique-violation check, `error_lifts!`, dead kinds removed |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (role, via and scope strings as named constants), NDC and NLR (no alias scopes), NLG, FLL (split before 350 lines), ECL (an outage never answers "not a member").
- `docs/REST_API_DESIGN_GUIDELINES.md` — new routes, six-place registration, problem bodies, 403 parity with `UZ-AUTH-001`.
- `docs/SCHEMA_CONVENTIONS.md` and the static-strings rule — `role` and `email_status` stay `TEXT` with no `CHECK`; vocabularies live in constants, overriding the "gains a constraint" note in `schema/230_memberships.sql`.
- `docs/RUST_ERROR_STANDARD.md` — new modules declare `ErrorKind` behind `error_shell!`.
- `docs/LOGGING_STANDARD.md` — audit and invite events carry `event` and ids, never an email in clear.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — new table | additive migration only; no `DROP`/`ALTER` on shipped tables |
| ERROR REGISTRY | yes — five codes | declared in `afd_core::error_code`, each with a negative test |
| UFS | yes | constants for `owner`, `member`, scope strings |
| LOGGING | yes | structured events per `docs/LOGGING_STANDARD.md` |
| UI / DESIGN TOKEN | yes — three pages | design-system primitives and token utilities only |
| File & Function Length (≤350/≤50/≤70) | yes | invite and member logic in their own modules; `ownership.rs` gains a sibling before 300 lines |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_tenant/src/signup.rs` — the transactional membership write accept mirrors.
- **Reference:** `cross_tenant_override` — the audit-before-honour shape the layer's event keeps.

## Sections (implementation slices)

### §1 — Milestone credential enumeration (credential gate)

M208 needs one external credential, for M208_003: the `smtp-relay` bag `{host, port, username, password, from_address}` in the admin-workspace vault. Source of record: the 1Password item `smtp-relay` in `ZMB_CD_DEV` and `ZMB_CD_PROD`, created by Indy after verifying `agentsfleet.net` at Resend (host `smtp.resend.com`, port `465`, username `resend`, password a Resend API key per environment), and loaded with `playbooks/lib/platform_secret_sync.sh smtp-relay` (M208_003). It is a post-deploy input like `slack-app` and `github-app`: the early gates never read it (`credentials_test.sh`, `test_post_deploy_values_are_not_early_inputs`), the preflight playbook's post-deploy table names it and its source, and the sync fails naming a missing field. M208_001 and M208_002 ship with zero new credentials.

- **Dimension 1.1** — the post-deploy table lists `smtp-relay` and its five fields; neither early gate reads it → Test `test_smtp_relay_is_a_post_deploy_input` — DONE (`playbooks/founding/02_preflight/credentials_test.sh`)

### §2 — Access resolves through memberships, with two roles

The access check answers `{tenant, role, via}`: a `core.memberships` row for the caller's user in the workspace's tenant grants access with that row's role. The caller's own account keeps today's rule and answers `owner`; tenant API keys and CLI credentials resolve only through it. The check stays one statement with no subquery: three unique-index probes (workspace by id, user by subject, membership by account and user). A member is refused, with `UZ-AUTH-026`, where the route's required capability for the request's method is `secret:write` or `connector:write`, the only owner-grade capabilities on workspace routes (`rustd/crates/afd_http/src/route/{workspace,connector}.rs`); the capability gate still applies to everyone. The workspace list spans every membership, and each item names its account and the caller's role. Open streams re-run the check on the beat each already has, the workspace stream's 10 s refresh (`handler/stream/wall.rs`) and the fleet stream's 15 s heartbeat, and end with a problem frame when access is gone.

- **Dimension 2.1** — a member opens, lists, streams and steers the owner's workspace → Test `test_member_reaches_owner_workspace` — DONE (`afd_api/tests/integration_workspace_members.rs`)
- **Dimension 2.2** — a non-member still gets `403 UZ-AUTH-001`, byte-identical to today → Test `test_non_member_refused_unchanged` — DONE (`afd_api/tests/integration_workspace_members.rs`)
- **Dimension 2.3** — a member is refused every secret-write and connector-write route with `UZ-AUTH-026` → Test `test_member_refused_owner_only_routes` — DONE (`afd_api/tests/workspace_member_roles.rs` over every mounted route, and live)
- **Dimension 2.4** — a removed member's open stream ends within one re-check (15 s) → Test `test_removed_member_stream_ends` — DONE (both streams, `integration_workspace_members.rs`)
- **Dimension 2.5** — a datastore failure during the check is `503`, never a denial → Test `test_access_check_outage_is_not_denial` — DONE (`afd_api/tests/workspace_member_roles.rs`, the real resolver over a dead pool)
- **Dimension 2.6** — an account with no invites behaves exactly as today → Test `test_single_owner_paths_unchanged` — DONE (`afd_api/tests/integration_workspace_members.rs`)
- **Dimension 2.7** — the access statement plans as index probes only → Test `test_access_check_plans_as_index_probes` — DONE (`afd_tenant/tests/integration_workspace_access_plan.rs`, custom and generic plans)

### §3 — Owners invite; invitees accept

An owner invites an email (lowercased) as `member`; one pending invite per `(tenant, email)`; invites expire after 7 days. The invitee sees pending invites for their account email and accepts: the membership insert and the invite's `accepted_at` commit in one transaction, and accepting twice is a no-op. An owner lists members, lists and revokes invites, and removes a member; the last owner is never removed. The create response carries the accept `link` (`{dashboard}/invites/{invite_id}`, from `services.dashboard()`).

- **Dimension 3.1** — create, list, revoke by an owner → Test `test_owner_manages_invites` — DONE (`afd_tenant/tests/integration_team.rs`; the seven routes end to end in `afd_api/tests/integration_team_routes.rs`)
- **Dimension 3.2** — accept by the matching account email creates one member row → Test `test_invitee_accepts_once` — DONE (`integration_team.rs`, plus a failed second write leaving no member row)
- **Dimension 3.3** — a different email is `403 UZ-INV-002`; expired or revoked is `404 UZ-INV-001` → Test `test_accept_refusals` — DONE (`integration_team.rs`)
- **Dimension 3.4** — a duplicate pending invite or existing member is `409 UZ-INV-003`; removing the last owner is `409 UZ-INV-004` → Test `test_invite_and_member_conflicts` — DONE (`integration_team.rs`)

### §4 — The dashboard: members, invites, the account-grouped switcher

Owners get Settings → Members (invite, copy link, pending invites, members, remove). Invitees get an Invites page and a one-line notice while any invite is pending. The workspace switcher groups by account ("Yours", "John's account").

- **Dimension 4.1** — an owner invites, copies the link and removes a member on the page → Test `test_members_page_owner_journey` — written (`tests/e2e/acceptance/team-members.spec.ts`); runs after merge, once `main` deploys to dev (Discovery)
- **Dimension 4.2** — an invitee accepts and the workspace appears under the owner's account → Test `test_invitee_accept_journey` — written (same spec); runs after merge, once `main` deploys to dev (Discovery)

### §5 — Platform operators: act in any workspace, audited

`workspace:any` admits every method across tenants, as it does today; there is no read-only crossing (Discovery). The ownership layer emits `cross_tenant_workspace_override` before a crossing is honoured, with the method, so a look and an act read differently in the log. The event moves there from `afd_tenant/src/workspace/mod.rs:163`, which cannot see the method and logged again on every stream re-check. An operator opens a workspace by its URL; there is no directory (Discovery). The operator's identity-provider scopes need no edit.

- **Dimension 5.1** — an operator steers another tenant's fleet, attributed to the operator → Test `test_platform_write_acts_attributed` — DONE (`afd_api/tests/integration_workspace_members.rs`)
- **Dimension 5.2** — every honoured crossing logs one audit event with the method, before the handler; access from inside the account logs none → Test `test_platform_crossing_audited` — DONE (`afd_tenant/src/workspace/crossing.rs`; placement in the layer by `test_layer_records_platform_crossings`, `afd_api/tests/workspace_member_roles.rs`)

### §6 — Documentation

`docs/AUTH.md` (memberships, the two roles, the owner-only refusal); public members and invites pages on the docs branch; no changelog entry (Discovery).

- **Dimension 6.1** — `docs/AUTH.md` names both roles and `UZ-AUTH-026` → Test `test_docs_name_member_roles` — DONE (`afd_tenant/src/workspace/access.rs`)

## Interfaces

```
POST   /v1/tenants/me/invites            {email}   -> 201 {id, email, role:"member", expires_at, link}
GET    /v1/tenants/me/invites                      -> 200 {items:[...]}
DELETE /v1/tenants/me/invites/{invite_id}          -> 204
GET    /v1/users/me/invites                              -> 200 {items:[{id, account:{owner_name}, expires_at}]}
POST   /v1/users/me/invites/{invite_id}/accept           -> 200 {workspace_ids:[...]}
GET    /v1/tenants/me/members                      -> 200 {items:[{user_id, display_name, email, role, joined_at}]}
DELETE /v1/tenants/me/members/{user_id}            -> 204
GET    /v1/workspaces/{workspace_id}/members       -> 200 {items:[{user_id, display_name, role}]}
GET    /v1/tenants/me/workspaces   items gain {account:{tenant_id, owner_name}, role}
Errors: UZ-AUTH-026 role refused (403)
        UZ-INV-001 not found/expired/revoked (404) · UZ-INV-002 email mismatch (403)
        UZ-INV-003 already pending or member (409) · UZ-INV-004 last owner (409)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Link reaches a third party | forwarded invite | accept requires the signed-in account's email to equal the invite's: `403 UZ-INV-002` |
| Concurrent accept | two tabs accept at once | unique `(tenant_id, user_id)`; the second returns the same body |
| Member removed mid-stream | owner removes Bob while he watches | problem frame within one re-check; next request `403 UZ-AUTH-001` |
| Access-check outage | Postgres unavailable | `503` with the outage code, never `403`; a failed stream re-check keeps the stream until the next pass |
| Last owner removal | sole owner removes self | `409 UZ-INV-004` |

## Invariants

1. Access is decided once, in `ownership.rs`, for every workspace route — mounted from the route template (existing); the role gate reads the same access record.
2. A member never exceeds their own Clerk scopes — the capability gate runs before ownership (existing order), and roles only subtract.
3. Membership insert and invite acceptance commit together — one transaction; a unit test fails the second statement and asserts no member row.
4. No crossing is honoured before its audit event — the ownership layer and the connector callback call `crossing::audit` before the work; the layer suite fails when the layer's call is removed.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `workspace_invite_created` | product | owner creates an invite | tenant id, invite id, role | no email logged | `test_owner_manages_invites` |
| `workspace_invite_accepted` | product | invitee accepts | tenant id, invite id, user id | no email | `test_invitee_accepts_once` |
| `workspace_member_removed` | ops | owner removes a member | tenant id, user id | no email | `test_invite_and_member_conflicts` |
| `cross_tenant_workspace_override` | ops | platform crossing | operator id and tenant, target tenant, workspace, method | no request body | `test_platform_crossing_audited` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_smtp_relay_is_a_post_deploy_input` | bootstrap and deployment gates with every item present → neither reads `smtp-relay/`; the post-deploy table names `smtp-relay` and its five fields |
| 2.1 | integration | `test_member_reaches_owner_workspace` | membership row → detail, list, stream and steer 2xx |
| 2.2 | integration | `test_non_member_refused_unchanged` | no row → `403 UZ-AUTH-001`, body equal to today's |
| 2.3 | integration | `test_member_refused_owner_only_routes` | member on every secret-write and connector-write route → `403 UZ-AUTH-026`; their reads 2xx |
| 2.4 | integration | `test_removed_member_stream_ends` | removal during an open stream → problem frame within one re-check |
| 2.5 | unit | `test_access_check_outage_is_not_denial` | query error → `503`, never `403` |
| 2.6 | integration | `test_single_owner_paths_unchanged` | no invites → list, detail, stream, steer as before |
| 2.7 | integration | `test_access_check_plans_as_index_probes` | thousands of seeded rows, `ANALYZE` → `EXPLAIN` of the access statement holds no `Seq Scan` on workspaces, users or memberships |
| 3.1 | integration | `test_owner_manages_invites` | create → listed → revoke → list empty |
| 3.2 | integration | `test_invitee_accepts_once` | accept twice → one membership, same body |
| 3.3 | integration | `test_accept_refusals` | other email → `UZ-INV-002`; expired, revoked → `UZ-INV-001` |
| 3.4 | integration | `test_invite_and_member_conflicts` | duplicate, existing member → `UZ-INV-003`; last owner → `UZ-INV-004` |
| 4.1 | e2e | `test_members_page_owner_journey` | invite, copy link, remove on the rendered page |
| 4.2 | e2e | `test_invitee_accept_journey` | accept → switcher shows the workspace under the owner's account |
| 5.1 | integration | `test_platform_write_acts_attributed` | `workspace:any` steer on another tenant's fleet → 202, row actor `steer:<operator>` |
| 5.2 | unit | `test_platform_crossing_audited` | platform `POST` → one warn event naming `POST`, operator and target; owner or member → none; the layer suite sees one event per crossing request and none for the owner |
| 6.1 | unit | `test_docs_name_member_roles` | `docs/AUTH.md` → names `owner`, `member` and `UZ-AUTH-026` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Members reach and are bounded (§2, §3) | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | Invite and accept journeys (§4) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts --project=journeys -g "test_invitee_accept_journey\|test_members_page_owner_journey"` | `2 passed` | P0 | post-merge (Indy, Discovery) |
| R3 | Operator crossings are audited (§5) | `cd rustd && cargo test -p afd_tenant --all-features --lib crossing && cargo test -p afd_api --all-features --test tenant_plane test_layer_records_platform_crossings` | exit 0 | P0 | |
| R4 | Member roles documented | `git grep -c "UZ-AUTH-026" -- docs/AUTH.md` | ≥ 1 | P0 | |
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

| File | Replaced by |
|------|-------------|
| `rustd/crates/afd_tenant/src/team/accept.rs` | `team/invitation/{mod,lifecycle}.rs` |
| `rustd/crates/afd_tenant/src/team/invite.rs` | `team/invitation/{mod,lifecycle}.rs` |

**2. Orphaned references — zero remaining imports/uses.**

`git grep -nE "team::(accept|invite)\b|LockedInvite|Standing\b|is_name_conflict" -- rustd` → 0 matches (Oct 1, 2026). `workspace:any` keeps its meaning (Discovery).

## Out of Scope

- A viewer (watch-only) role — owner and member only (Indy, Sep 30, 2026).
- Per-workspace invites — an invite grants the whole account (Indy, Sep 30, 2026; kept when asked again the same day).
- A read-only platform crossing — `workspace:any` reads and writes (Indy, Sep 30, 2026, Discovery).
- An operator directory of every workspace — an operator opens one by its URL (Indy, Sep 30, 2026, Discovery).
- A changelog entry for M208 (Indy, Sep 30, 2026, Discovery).
- API keys and CLI credentials reaching an invited account — they stay bound to their own tenant.
- Transferring ownership or multiple owners — the account creator stays the one owner.
- Live messages and sender names — M208_002. Invite email — M208_003.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Bob accepts John's invite, finds MARY-001 under "John's account" in the switcher, opens it and steers it.
2. **Preserved user behaviour** — a solo account works exactly as today: same list, refusals, stream and steer; API keys and the CLI unchanged.
3. **Optimal-way check** — the optimal shape adds a viewer role and per-workspace grants; neither blocks a two-person team, and both extend the same access record later.
4. **Rebuild-vs-iterate** — iterate: `core.memberships` was shaped for teams at signup; access and roles extend one existing seam.
5. **What we build** — membership access with two roles, invites, members and invites pages, grouped switcher, and a crossing audit that names the method.
6. **What we do NOT build** — viewer role, per-workspace invites, cross-account API keys, ownership transfer, read-only operator access, an operator directory.
7. **Fit with existing features** — the steer path, pending-sends ledger and chat hold are unchanged; M208_002 names the senders this creates.
8. **Surface order** — User Interface (UI) first: inviting and accepting are dashboard acts; the API is public and documented; the CLI follows later.
9. **Dashboard restraint** — no Members entry for a member, no invite notice without a pending invite.
10. **Confused-user next step** — a refused accept says to sign in with the address the invite was sent to, without naming it; a member refused an owner action reads "Only the account owner can do this."

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** M208 is three workstreams in one Pull Request — 001 access (the core the other two need), 002 live messages, 003 invite email — because one file cannot hold all three under the 320-line cap.
- **Alternatives considered:** per-workspace memberships (Indy chose account-level); Clerk Organizations as the membership authority (rejected: a second authority beside `core.memberships`, while scopes already resolve per person).
- **Patch-vs-refactor verdict:** this is a **refactor** of the access decision: every workspace route moves from one column to a relation, kept to one layer.

## Discovery (consult log)

- **Consults** — Sep 30, 2026, Indy via AskUserQuestion: "One spec, one PR" (carried as three workstream specs in one PR because of the 320-line cap); an invite grants "John's whole account"; roles "Owner + member"; operators "See and act, audited, but must allow adding write or act as well, need to know the design of this" (§5's scope split). Then: "I want the invite emails into an account" and a send-time broadcast "is also a must have" (M208_003, M208_002); email provider "Resend (Recommended)". Source finding: access by `core.users.tenant_id` (`afd_tenant/src/sql/workspace.rs:38-44`).
- **Metrics review** — at the Pull Request boundary, with the skill chain.
- **Skill-chain outcomes** — `/orly-write-unit-test` and `/review` run once at the Pull Request boundary over the whole M208 diff, since the three workstreams ship in one Pull Request (Batch B1); outcomes land in the Pull Request Session Notes.
- **Deferrals** — none. The changelog is skipped, not deferred: Sep 30, 2026, Indy: "Skip change log".
- **Operator scope** — Sep 30, 2026, Indy via AskUserQuestion: "Keep workspace:any". It keeps its read-and-write crossing, so the operator's identity-provider profile needs no edit and no deploy step; `workspace-any:read` is added for read-only. This supersedes the rename to `workspace-any:write` and its pre-deploy step.
- **No read-only crossing** — Sep 30, 2026, Indy: "I want to keep it simple and provide write both for the invitee accepted + platform admin with the scope workspace:Any". `workspace-any:read`, `UZ-AUTH-027`, the workspace detail's `access`, the banner and the hidden composer are dropped; §5 keeps the directory and the audit event, which moves to the ownership layer to carry the method. Asked again the same day, Indy kept account-wide invites ("Keep whole account").
- **§1 source correction** — Sep 30, 2026: provider bags are post-deploy inputs the early gates must not read (`playbooks/founding/02_preflight/credentials_test.sh`, `test_post_deploy_values_are_not_early_inputs`), so §1 lists `smtp-relay` in the playbook's post-deploy table rather than failing the preflight. The bag is `smtp-relay` per decision `a3d20406` (SMTP through `afd_mail`); Resend's SMTP settings are from resend.com/docs/send-with-smtp. No `smtp-relay` item existed yet in `ZMB_CD_DEV` or `ZMB_CD_PROD` (`op item list`, Sep 30, 2026); Indy added both that evening, and `op item get smtp-relay` in each lists `username, password, host, port, from_address`.
- **§4 source corrections** — Sep 30, 2026: every `/v1/tenants/me/*` route resolves the caller's own account (`afd_api_tenant/src/handler/tenant/mod.rs:96-111`) and signup makes every person its owner, so Settings → Members always shows; restraint 9's "no Members entry for a member" has no case. Interfaces has no decline route, so the Invites page offers accept only. The switcher labels accounts only once a person holds more than one, keeping Dimension 2.6's solo view. The acceptance env targets the shared dev API (`ui.env.local`), which runs `main`, so R2 runs after this branch deploys there.
- **§3 source corrections** — Sep 30, 2026: the `UZ-INV-002` refusal names no address (`afd_core/src/problem/invite.rs:27`), since the link may reach a third party and naming the address would leak it; Product Clarity 10 amended. A revoked stream ends with `event: access_revoked`, data `{kind, error_code:"UZ-AUTH-001"}` (`afd_api_tenant/src/handler/stream.rs:41-43`). Invite create takes no `Idempotency-Key` (`docs/REST_API_DESIGN_GUIDELINES.md:142`): a repeat create answers `409 UZ-INV-003` and the pending list offers the link; Indy kept the 409 over returning the pending invite (AskUserQuestion, Sep 30, 2026: "Keep the 409").
- **Hand-rolled Rust cleanup** — Sep 30, 2026, Indy in session: "Are there any handrolled rust code? where you can use the afd_core or external crates, if yes fix them", then "Are there any duplicate handrolled code you have, if yes fix them." and "and clean it up". Its own commit on this branch: `Entropy::uuid7` replaces every draw-then-encode copy, `sqlx::error::DatabaseError::is_unique_violation` replaces the hand-written `23505` checks, `afd_tenant` lifts through `error_lifts!`, the identifier kinds that became dead are removed, and `DETAIL_NOT_DASHBOARD` drops the identity vendor's name.
- **Directory cut** — Sep 30, 2026, Indy: "I donot see value in doing Direcotry since opening a workspace by URL works if i am a platform admin with workspace:any scope. So cut tht scope". Dimensions 5.3 and 5.4 are removed with their Interfaces line, tests, the `core.workspaces (created_at, id)` index, and the admin route, handler and page rows. No directory code had been written (`git grep`, Sep 30, 2026). This supersedes "§5 keeps the directory" above.
- **R2 after merge** — Sep 30, 2026, Indy via AskUserQuestion: "Run R2 after merge". The journeys call the shared dev API, which runs `main`; a branch deploy (`gh workflow run deploy-dev.yml --ref feat/m208-team-accounts`) was offered and declined. The Pull Request opens without R2 under an Orly-Override Indy records; R2 runs once `main` deploys to dev, and Dimensions 4.1 and 4.2 are graded then.
- **Members page redesign** — Oct 1, 2026, Indy: "Keep things simple and follow our standard design in the UI (table, buttons and so on), iconify and refer the existing and design it. Less clutter"; via AskUserQuestion, "One table (recommended)"; then "use the ago format, and ensure the column is named accordingly? Time i think?". The page follows API Keys (`settings/api-keys/components/ApiKeysView.tsx`): one section, a `+ Invite` dialog behind a `next/dynamic` shim, one `DataTable` holding people and `invited` rows, `IconAction` row actions, and a Time column in the relative format. Members gain `joined_at` (`core.memberships.created_at`) for that column; the route is new on this branch, so no client depended on the old shape. The Invites page's accept became a check `IconAction`, and its Expires column became Time.
- **Reuse, behaviour and query cost** — Oct 1, 2026, Indy: "Replace any handrolled rust code with crates and afd_core or refer other on how its being done"; "any repeated code is avoided. Handrolled coded is avoided if there is an existing code reuse in afd_core (enrich reusability) or via any popular battle tested create"; "I think you can have a invitation.rs with behavior to accept, revoke, resend etc of invites. I dont know what is a LockedInvite"; "Ensure the queries are optimized, and performant concurrent and not bloated with joins or orderby group by for no reason." Outcomes: `Role` parses through `afd_core::spelling`; rows read by name (`try_get`), single columns by `query_scalar`; `afd_db::constraint::violates_unique` replaces three copies; `team/invitation/` replaces `LockedInvite`, `Standing` and the owner-side `Invite` with one `Invitation` and its `Acceptance`. `EXPLAIN` over 40,000 seeded invites showed `idx_invites_tenant_id_created_at` never chosen (the pending list reads `uq_invites_tenant_id_email_pending`, 4 buffers), so it is dropped; the accounts statement's `ORDER BY t.id` had no reader and is dropped; two sort keys an `AS id` alias had turned into text sorts are qualified. `test_team_reads_plan_as_index_probes` pins the five team reads to index probes under custom and generic plans; a mutation that defeats the pending index fails it.
- **Close** — Oct 1, 2026: closed per Indy's pick "Close now, R2 gates PR (recommended)". Dimensions 4.1 and 4.2 are graded after merge (R2 above). `Baseline revision:` moves from `f90ed13` to `3b61121c3`, the merge-base once `main` was merged into the branch, so the Test Delta compares like with like; the baseline lanes run before the Pull Request. M208_002 and M208_003 gain `Folded-into: M208_001` at their own close, so `orly gate pr` finds one owner among the branch's `done/` specs. §6's public page is `workspaces/teammates.mdx` on the docs branch (`2adea19`), with the error codes and the Workspaces entry; its Invites and Members API reference groups are stashed in that worktree, since the docs pre-commit drift check refuses routes `main` does not serve yet, and land once this Pull Request merges.
