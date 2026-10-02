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

# M208_004: Team lists answer in bounded pages, team actions are counted, and the invite create's wait on email is measured and owned

**Prototype:** v2.0.0
**Milestone:** M208
**Workstream:** 004
**Date:** Oct 02, 2026
**Status:** PENDING
**Priority:** P3 — review findings Indy deferred out of the M208 Pull Request (PR); nothing in John-invites-Bob is broken without them
**Categories:** API, OBS, UI
**Batch:** B2 — after the M208 PR (M208_001–003) merges; one Section order inside, no parallel workstream
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M208_001 (`core.invites`, the four team list routes, the members and Invites pages) · M208_003 (the invite email, `MAIL_SEND_DEADLINE`, the `InviteEmail` product event)
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 2, 2026) from a production-readiness review of the M208 branch; every `file:line` re-read at `HEAD` of `feat/m208-team-accounts`; the decisions in Discovery are Indy's
**Canonical architecture:** `docs/AUTH.md` §Invites; `docs/architecture/observability.md` §PostHog is product analytics, not operations telemetry

---

## Overview

**Goal (testable):** `test_invite_list_pages_by_keyset` — an account with three pending invites read at `limit=2` answers two pages that together hold all three, newest first, once each; every team list statement carries a `LIMIT`, and the members page still shows every row.
**Problem:** The four team lists (`GET /v1/tenants/me/invites`, `GET /v1/users/me/invites`, `GET /v1/tenants/me/members`, `GET /v1/workspaces/{workspace_id}/members`) return every row in one response from statements with no `LIMIT`, against the repository's REST (Representational State Transfer) rules. An operator cannot tell from any metric whether invites are being sent, accepted or cancelled, or whether the mail relay is failing. The invite create waits on a Simple Mail Transfer Protocol (SMTP) exchange of up to 10 s, and no spec records the REST latency carve-out that wait needs.
**Solution summary:** The four lists page by keyset through `afd_core::paging`, and the app's User Interface (UI) clients walk `next_cursor` to the end, as the API-key list does, so John sees no paging control and no change. Invite created, accepted and revoked become product events, and every invite-email outcome increments one operator counter. The create keeps sending before it answers, because its `email_status` is how John knows to send again; this spec measures that wait and records it as the REST §12 carve-out for Indy to accept. Deleting closed invite rows and hiding expired invites are dropped with evidence (§2, §3).

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(team): page the team lists, count team actions, own the invite wait`
- **Intent (one sentence):** the team surfaces John and Bob use keep working exactly as they do, while every list read is bounded and an operator can see invites and their email succeed or fail.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_api_tenant/src/handler/tenant/workspace.rs` — the keyset list to mirror: `limit` and `starting_after` read through `workspace/input.rs`, `total: None` in `workspace/render.rs`.
2. `ui/packages/app/lib/api/api_keys.ts` — a list the client walks to its end through `walkList` (`lib/api/list-walk.ts`, bound `MAX_LIST_WALK_REQUESTS` = 40) with no paging controls; the team clients take this shape.
3. `rustd/crates/afd_tenant/tests/integration_api_key_paging.rs` — the paging integration tests the team paging tests mirror.
4. `rustd/crates/afd_observability/src/metrics/label/fleet.rs` — `SignupFailure`, the closed label set and census ceiling the invite-email counter copies; `docs/metrics.census.tsv` holds its row.
5. `docs/REST_API_DESIGN_GUIDELINES.md` — §3 Filtering, sorting, pagination (keyset only, `limit` 50 default, 100 max) and §12 Performance (the five facts a carve-out carries).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_tenant/src/sql/invite.rs`, `sql/member.rs` | EDIT | the three list statements gain a keyset predicate and `LIMIT`; order and the open-invite predicate unchanged |
| `rustd/crates/afd_tenant/src/team/invitation/lifecycle.rs`, `team/member.rs` | EDIT | list methods take a cursor and a limit; revoke reports whether it closed a row |
| `rustd/crates/afd_http/src/services/team.rs`, `rustd/crates/afd_api/tests/harness/team_faults.rs` | EDIT | the `TenantTeam` trait, its implementation and its fault double follow the paged signatures |
| `rustd/crates/afd_api_tenant/src/handler/tenant/{mod,invite,member}.rs` | EDIT | routes read paging; `one_page` goes; create, accept and revoke report events |
| `rustd/crates/afd_api_tenant/src/handler/tenant/invite_email.rs` | EDIT | each send outcome increments the counter beside the product event it already reports |
| `rustd/crates/afd_observability/src/product/{telemetry,properties}.rs` | EDIT | `InviteCreated`, `InviteAccepted`, `InviteRevoked` |
| `rustd/crates/afd_observability/src/metrics/{declared,label}/fleet.rs`, `producers/fleet.rs` | EDIT | `agentsfleet_invite_emails_total`, its two closed sets, its recorder |
| `docs/metrics.census.tsv` | EDIT | one row, ceiling `fixed:9` |
| `docs/architecture/observability.md` | EDIT | the production event list gains the three team events and M208_003's invite-email events |
| `public/openapi.json` | EDIT | regenerated: `starting_after` and `limit` on four lists, prose drops "one page" |
| `ui/packages/app/lib/api/{invites,tenant-members,decode}.ts` (+ their tests) | EDIT | walk `next_cursor`; `decodeOnePage` goes |
| `rustd/crates/afd_tenant/tests/integration_team_paging.rs` (+ `tenant_suite.rs`) | CREATE | §1 paging at the store, registered in the crate's one test suite |
| `rustd/crates/afd_tenant/tests/integration_team*.rs` | EDIT | the §2 and §3 regression tests call the paged signatures, in whichever file the in-flight split of `integration_team.rs` leaves them |
| `rustd/crates/afd_api/tests/integration_team_signals.rs` (+ `tenant_plane_suite.rs`) | CREATE | §1 route refusals, §4 events |
| `rustd/crates/afd_api/tests/integration_invite_email.rs` | EDIT | §5 stall bound |
| `ui/packages/app/tests/e2e/acceptance/team-members.spec.ts` | EDIT | members page regression |
| `~/Projects/docs` (branch `chore/m208-team-lists-changelog`) | EDIT | changelog `<Update>`: the team lists page |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (outcome and reason spellings, event names, the walk's collection names as constants), NDC and ORP (`one_page` and `decodeOnePage` leave with their last caller), OBS (each team action leaves an event), ECL (an analytics or metric failure never fails the request), FLL.
- `docs/REST_API_DESIGN_GUIDELINES.md` — §3 keyset paging and the `items`/`total`/`next_cursor` envelope; §12 no unbounded statements and the carve-out's five facts.
- `docs/architecture/observability.md` — PostHog carries product events, operators read OpenTelemetry Protocol (OTLP) metrics; a counter's ceiling is the product of its closed sets.
- `docs/LOGGING_STANDARD.md` — events and counters carry identifiers, never an email address.
- `dispatch/write_rust.md`, `dispatch/write_ts_adhere_bun.md` — the Rust and TypeScript façades for every edit above.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS | yes — new spellings | constants beside their closed sets; walk names as module constants |
| LOGGING | yes — new reports | identifiers only; `test_team_events_carry_no_address` |
| UI | yes — `lib/api/*.ts` | no component or markup change; clients only |
| Architecture consult | yes — a metric family | census row and `observability.md` updated in the same commit |
| SCHEMA GUARD | no — no schema change | keyset reads use `idx_invites_tenant_id`, `idx_invites_email_pending` and the memberships tenant index already in place |
| File & Function Length (≤350/≤50/≤70) | yes | new tests go in new files; the existing team test files gain no test |

## Prior-Art / Reference Implementations

- **Reference:** `GET /v1/tenants/me/workspaces` (`handler/tenant/workspace.rs`) — keyset paging and `total: None`, mirrored.
- **Reference:** `listApiKeys` (`ui/packages/app/lib/api/api_keys.ts`) — a small human-made list the client walks whole; the team clients copy it.
- **Reference:** `SIGNUP_FAILED_TOTAL` (`afd_observability/src/metrics/declared/fleet.rs:29`) and `Telemetry::WorkspaceCreated` reported after its write (`handler/tenant/workspace.rs:187`) — the counter and the events mirror them.

## Sections (implementation slices)

### §1 — The four team lists page by keyset (item 1: build)

Each list reads `limit` and `starting_after` through `afd_core::paging` (default 50, max 100) and answers `next_cursor`. Invites keep newest first on `(created_at, id)`, with the invite id as cursor; members keep oldest membership first on the membership's `(created_at, id)`, with the member's `user_id` as cursor. A cursor is honoured only if it names a row of the same list's owner (the account, or the caller's address); the cursor row's position is read regardless of its state, so an invite revoked or accepted between pages does not break the walk. `one_page` (`handler/tenant/mod.rs:79`, new on this branch) and `decodeOnePage` (`lib/api/decode.ts:17`) leave. The app's invite, waiting and member clients walk through `walkList`, so the members page, the Invites page, the shell's invite notice and a thread's sender names render complete lists. **Implementation default:** `total` is `null` because the workspace list sets it so (`workspace/render.rs:47`) and a count would add a statement per page.

- **Dimension 1.1** — the owner's invite list pages newest first → Test `test_invite_list_pages_by_keyset`
- **Dimension 1.2** — both member lists page oldest membership first → Test `test_member_lists_page_by_keyset`
- **Dimension 1.3** — the invitee's waiting list pages newest first → Test `test_waiting_list_pages_by_keyset`
- **Dimension 1.4** — a `limit` of 0, 101 or `abc` is refused with 400 → Test `test_team_list_refuses_bad_limit`
- **Dimension 1.5** — a cursor naming another account's invite or a removed member is refused with 400 → Test `test_team_list_refuses_foreign_cursor`
- **Dimension 1.6** — revoking the cursor invite between pages leaves the next page intact → Test `test_invite_page_continues_after_cursor_invite_closes`
- **Dimension 1.7** — every team list statement binds a limit → Test `test_team_list_statements_are_bounded`
- **Dimension 1.8** — each team client walks a two-page response into one list → Test `test_team_clients_walk_every_page`
- **Dimension 1.9** — the members page still lists the owner and a pending invite → Test `test_members_page_lists_team_after_paging`

### §2 — Closed invite rows stay; no retention sweep (item 2: dropped)

Rows are never deleted except by cascade from their account or inviter (`schema/923_workspace_invites.sql:29,32`, and :55-57 says so). Accepted rows must stay: an accept replay reads them (`lifecycle.rs:187-195`, `Acceptance::AlreadyJoined`), and they are the record of how a member got access (`lifecycle.rs:7-9`). Revoked and expired rows have no reader: the list and send statements exclude them (`sql/invite.rs:52,78,133`). Growth is one small row per owner click, and no retention rule exists in `docs/` (`git grep -liE "data retention|personal data" -- docs` → no output). A sweep would add a background job, mirroring `afd_runner/src/sweep/retention.rs`, for no read the use case makes. This Section adds no code; its Dimensions hold the behaviour that justified dropping it against §1's statement rewrite.

- **Dimension 2.1** — revoked and accepted invites stay out of every paged list → Test `should_list_only_open_invites_when_revoked_or_accepted`
- **Dimension 2.2** — an accept replay still answers from the accepted row → Test `test_invitee_accepts_once`

### §3 — Expired invites are already hidden (item 3: dropped)

Both invite lists filter `expires_at > now` (`sql/invite.rs:52,78`), both pages render per request (`settings/members/page.tsx:9`, `invites/page.tsx:4`, `force-dynamic`), and `should_treat_invite_as_closed_everywhere_when_now_equals_expiry` (`afd_tenant/tests/integration_team.rs:573` at `HEAD`) proves the list, the waiting list, a send and an accept all close at the expiry instant. An expired link opened at `/invites/{invite_id}` still shows an Accept card; accepting answers 404 `UZ-INV-001` with "This invite is no longer valid. Ask the account owner to send a new one." (`afd_core/src/problem/invite.rs:16-19`), which is the next step John and Bob need. This Section adds no code.

- **Dimension 3.1** — expiry closes the invite in every paged read at the exact instant → Test `should_treat_invite_as_closed_everywhere_when_now_equals_expiry`
- **Dimension 3.2** — accepting an expired invite is 404 `UZ-INV-001` → Test `test_accept_refusals`

### §4 — Team actions and invite email outcomes are counted (item 4: build, narrowed)

The use case has three verbs (John invites, Bob joins, John cancels) and one failure (Bob gets no email). Create, accept and revoke each report one product event after their write commits, as `WorkspaceCreated` does; a replayed accept or an idempotent revoke that closed nothing reports none. Every invite-email outcome increments `agentsfleet_invite_emails_total{outcome, reason}` from the same value the existing `InviteEmail` product event reports (`telemetry.rs:149` at `HEAD`), so the two cannot disagree. `outcome` ∈ {`sent`, `failed`, `unconfigured`}; `reason` ∈ {`none`, `refused`, `no_reply`}, where `refused` is a failure carrying a relay reply code and `no_reply` one without. Member removal stays a log (`team/member.rs:84`): it is outside the use case. **Implementation default:** the counter lives in the signup family's files because both count a person-facing write's outcome.

- **Dimension 4.1** — create, accept and revoke each report one event carrying identifiers only → Test `test_invite_actions_report_product_events`
- **Dimension 4.2** — an accept replay reports no second `invite_accepted` → Test `test_accept_replay_emits_no_event`
- **Dimension 4.3** — revoking a closed invite reports no `invite_revoked` → Test `test_revoke_replay_emits_no_event`
- **Dimension 4.4** — each email outcome increments its `{outcome, reason}` series once → Test `test_invite_email_counter_counts_each_outcome`
- **Dimension 4.5** — with analytics unset, create, accept and revoke still answer → Test `test_invite_actions_answer_without_analytics`
- **Dimension 4.6** — no event or counter label carries an address → Test `test_team_events_carry_no_address`

### §5 — The create keeps sending before it answers, measured (item 5: keep, with a REST §12 carve-out)

`POST /v1/tenants/me/invites` sends inline (`handler/tenant/invite.rs:113`) under `MAIL_SEND_DEADLINE` = 10 s (`afd_mail/src/mailer.rs:28` at `HEAD`) so the 201 carries `email_status` (M208_003 §2). That status is how John learns Bob's email failed and sends again, the exact step in Indy's use case. Answering first would need a `sending` state, a client that polls, and either an outbox or a silent loss when the process dies between the 201 and the send. The cost is latency: an SMTP exchange is several round trips, so the create very likely misses REST §12's p99 < 200 ms, and a stalled relay holds one of 256 in-flight slots (`afd_http/src/admission/mod.rs:62`) for up to the deadline. This Section measures the wait, bounds it with a test, and records the carve-out below for Indy's disposition.

**Performance Considerations (REST §12 carve-out):** Endpoint — `POST /v1/tenants/me/invites` and `POST /v1/tenants/me/invites/{invite_id}/send`. Measured numbers and load profile — pending, produced by Dimension 5.1: 100 sequential creates to distinct addresses, one owner, local Docker stack, once against Mailpit and once against Resend's SMTP relay delivering to its test inbox. Reason — an SMTP dialogue with an external relay inside the request, kept so the response states whether the email left. Remediation — `accepted-permanent` proposed; Indy decides (Dimension 5.3).

- **Dimension 5.1** — p50, p95 and p99 of both create paths are measured and written into the carve-out above → Test `test_invite_create_latency_measured`
- **Dimension 5.2** — a relay that never answers yields 201 `failed` within the send deadline plus one second → Test `test_create_answers_within_send_deadline_on_stall`
- **Dimension 5.3** — Indy accepts or rejects `accepted-permanent`, quoted in Discovery → Test `test_carve_out_disposition_recorded`

## Interfaces

```
GET /v1/tenants/me/invites?starting_after=<invite_id>&limit=<1..100>         200 {items, total:null, next_cursor:<invite_id>|null}  newest first
GET /v1/users/me/invites?starting_after=<invite_id>&limit=<1..100>           same envelope, newest first
GET /v1/tenants/me/members?starting_after=<user_id>&limit=<1..100>           same envelope, oldest membership first
GET /v1/workspaces/{workspace_id}/members?starting_after=<user_id>&limit=..  same envelope, oldest membership first
    400  limit outside 1..100 or unreadable · cursor outside the list (the workspace list's details)
POST /v1/tenants/me/invites                                                  unchanged: 201 once the send settles or MAIL_SEND_DEADLINE passes
PostHog   invite_created | invite_accepted | invite_revoked  {actor, tenant_id, invite_id, request_id}
OTLP      agentsfleet_invite_emails_total  counter  labels outcome∈{sent,failed,unconfigured} reason∈{none,refused,no_reply}  fixed:9
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Bad limit | `limit` 0, above 100, or not a number | 400 before any statement runs |
| Foreign cursor | cursor names another account's invite, another address's invite, or a removed member | 400; the client starts again from the first page |
| Cursor row closes mid-walk | the cursor invite is revoked or accepted between pages | the keyset reads its position anyway; the next page is intact |
| Runaway cursor | a server bug echoes the same `next_cursor` | `walkList` throws after 40 requests (`list-walk.test.ts` "refuses a runaway cursor"); the page shows its error |
| Replayed accept or revoke | a second accept by the member, a revoke of a closed invite | same answer as today; no second event |
| Analytics down or unset | no PostHog client, or its buffer full | the event is dropped (`observability.md:394-396`); the request answers as before |
| Relay stall | the relay accepts the connection and never replies | 201 `failed` within the deadline; the counter takes `failed`/`no_reply` |

## Invariants

1. No team list statement runs unbounded — each binds a `LIMIT` parameter; `test_team_list_statements_are_bounded` asserts it over the statement constants.
2. Paging never reopens a closed or expired invite — the open-invite predicate stays in both paged statements; `should_treat_invite_as_closed_everywhere_when_now_equals_expiry` and `should_list_only_open_invites_when_revoked_or_accepted` run against them.
3. The counter cannot mint a series outside its ceiling — both labels are closed sets and `every_declared_ceiling_admits_its_label_product` grades the census row.
4. The product event and the counter for one send cannot disagree — both read the one outcome value `email_invite` holds.
5. The create never waits past its send deadline — the send runs under `tokio::time::timeout`; `test_create_answers_within_send_deadline_on_stall`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `invite_created` | product | an invite row commits | actor, tenant_id, invite_id, request_id | no address | `test_invite_actions_report_product_events` |
| `invite_accepted` | product | an accept writes a membership | actor, tenant_id, invite_id, request_id | no address | `test_accept_replay_emits_no_event` |
| `invite_revoked` | product | a revoke closes a row | actor, tenant_id, invite_id, request_id | no address | `test_revoke_replay_emits_no_event` |
| `agentsfleet_invite_emails_total` | ops | every send outcome | outcome, reason | labels only, no identifiers | `test_invite_email_counter_counts_each_outcome` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_invite_list_pages_by_keyset` | 3 invites, `limit=2` → page 1 holds the 2 newest and a cursor; page 2 holds the third and `next_cursor` null |
| 1.2 | integration | `test_member_lists_page_by_keyset` | owner + 2 members, `limit=2` on both routes → 3 rows oldest first across 2 pages; the workspace route carries no address |
| 1.3 | integration | `test_waiting_list_pages_by_keyset` | 3 accounts invite one address, `limit=2` → 3 rows newest first across 2 pages |
| 1.4 | integration | `test_team_list_refuses_bad_limit` | `limit=0`, `101`, `abc` on each route → 400 each |
| 1.5 | integration | `test_team_list_refuses_foreign_cursor` | cursor = another account's invite; = a removed member's `user_id` → 400 |
| 1.6 | integration | `test_invite_page_continues_after_cursor_invite_closes` | read page 1, revoke its last invite, read page 2 → the remaining invites, none twice |
| 1.7 | unit | `test_team_list_statements_are_bounded` | each list statement constant in `sql/invite.rs` and `sql/member.rs` holds `LIMIT $` |
| 1.8 | unit | `test_team_clients_walk_every_page` | fake fetch answers 2 pages → each client returns all rows; requests carry `starting_after` |
| 1.9 | e2e | `test_members_page_lists_team_after_paging` | signed in as the owner with one pending invite → both rows render, no paging control |
| 2.1 | integration | `should_list_only_open_invites_when_revoked_or_accepted` | regression on the paged signature: revoked and accepted invites absent |
| 2.2 | integration | `test_invitee_accepts_once` | regression: the second accept answers as the first |
| 3.1 | integration | `should_treat_invite_as_closed_everywhere_when_now_equals_expiry` | regression on the paged signature: open 1 ms early, closed everywhere at expiry |
| 3.2 | integration | `test_accept_refusals` | regression: an expired invite's accept is `UZ-INV-001` |
| 4.1 | integration | `test_invite_actions_report_product_events` | recording reporter: create, accept, revoke → exactly `invite_created`, `invite_accepted`, `invite_revoked` with the four identifiers |
| 4.2 | integration | `test_accept_replay_emits_no_event` | accept twice → one `invite_accepted` |
| 4.3 | integration | `test_revoke_replay_emits_no_event` | revoke twice; revoke an accepted invite → one `invite_revoked` |
| 4.4 | unit | `test_invite_email_counter_counts_each_outcome` | sent, failed with reply 550, failed without reply, unconfigured → `sent/none`, `failed/refused`, `failed/no_reply`, `unconfigured/none` each 1 |
| 4.5 | integration | `test_invite_actions_answer_without_analytics` | no analytics client → create 201, accept 200, revoke 204 |
| 4.6 | unit | `test_team_events_carry_no_address` | recorded events and counter attributes hold no `@` |
| 5.1 | manual | `test_invite_create_latency_measured` | implementing agent: the timing run named in §5; raw timings and revision in Session Notes; numbers in the carve-out |
| 5.2 | integration | `test_create_answers_within_send_deadline_on_stall` | listener accepts and never replies → 201 `failed`, elapsed ≤ deadline + 1 s |
| 5.3 | manual | `test_carve_out_disposition_recorded` | Indy reads the carve-out with numbers; his verbatim decision lands in Discovery |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Lists page; closed and expired invites stay out; events report; stall is bounded (§1–§5) | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | One-page helpers gone | `git grep -n -w -e one_page -e decodeOnePage -- rustd ui/packages/app` | 0 matches | P0 | |
| R3 | Counter declared in the census (§4) | `grep -c "^agentsfleet_invite_emails_total" docs/metrics.census.tsv` | `1` | P0 | |
| R4 | Production event list names the team events (§4) | `grep -oE "InviteCreated\|InviteAccepted\|InviteRevoked" docs/architecture/observability.md \| sort -u \| wc -l` | `3` | P1 | |
| R5 | Carve-out carries measured numbers and Indy's disposition (§5) | manual: §5 Performance Considerations holds p50/p95/p99 for both relays; Discovery holds Indy's quote | present | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Versions in sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears verbatim above (`make test-integration-rustd` is R1). See `dispatch/lifecycle.md` for timing.

**Grading protocol (VERIFY):** run each Verify command verbatim; Graded = ✅/❌ + one decisive output line; repository rows point to the final `orly gate pr` results. **Ship gate:** any ❌ returns to EXECUTE; R4 or R5 ❌ needs an Indy-acked deferral quote; a P0 is MOVED only into a named successor that carries it, with Indy's quote, and is never ✅.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `one_page` | `git grep -n -w one_page -- rustd` | 0 matches |
| `decodeOnePage` | `git grep -n -w decodeOnePage -- ui/packages/app` | 0 matches |

## Out of Scope

- A cap on invite sends (per invite or per account): Indy, Oct 02, 2026, "Skip this (or ignore) I donot see this issue in day 1, a future spec".
- A retention sweep for closed invite rows (§2) — revisit when a data-retention rule lands in `docs/` or someone asks for an invited address to be erased.
- A client-side expiry filter, and an "expired" state on the `/invites/{invite_id}` card (§3).
- A `member_removed` product event, and any invitee-domain property.
- Answering the create before the send: an outbox, a `sending` status, polling (§5) — revisit if Indy rejects `accepted-permanent` or the in-flight ceiling saturates.
- Changing `MAIL_SEND_DEADLINE`; paging controls in the UI; invite send caps (M208_003, Indy: "Skip this").

---

## Product Clarity (authoring record)

1. **Successful user moment** — John opens Settings → Members after inviting Bob and sees the same list as before; an operator sees `invite_created` then `invite_accepted` for John's account and a flat `failed` series on the email counter.
2. **Preserved user behaviour** — invite, copy link, send again, revoke, accept; both pages show every row with no paging control; the 201 still says whether Bob's email went out.
3. **Optimal-way check** — each item against John-invites-Bob. Paging: the use case never holds more than a handful of rows; built only because REST §3 and §12 forbid unbounded statements, and built so John sees nothing. Deleting old rows: no step in the use case reads a closed invite, and accepted rows are Bob's access record; dropped. Hiding expired invites: already true at every read; dropped. Metrics: the three verbs and the one failure of the use case, nothing else. Sending after the 201: would hide from John the one fact he acts on; kept and measured.
4. **Rebuild-vs-iterate** — iterate: two existing patterns copied, three events and one counter added.
5. **What we build** — keyset on four lists, walking clients, three product events, one counter, one latency measurement and its carve-out.
6. **What we do NOT build** — a retention sweep, an expiry filter, a removal event, an outbox, paging controls.
7. **Fit with existing features** — compounds with the workspace list and the API-key walk; must not destabilise M208_003's `email_status` and send-again.
8. **Surface order** — API first; the UI changes only in its clients; operators read the counter.
9. **Dashboard restraint** — no team-growth or relay-health panel until the events and counter carry real traffic.
10. **Confused-user next step** — an API caller follows `next_cursor` as the OpenAPI description says; an operator seeing `failed` follows `playbooks/operations/smtp_relay_registration/001_playbook.md`; Bob on an expired link reads "Ask the account owner to send a new one."

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one Section per auditor item, ordered so §1's statement rewrite lands before §2 and §3's regression tests run against it; §4 and §5 do not depend on §1.
- **Alternatives considered:** keeping `one_page` and capping members plus pending invites per account (rejected: invite scope Indy declined, and the waiting list stays unbounded); a `LIMIT` with no cursor (rejected: silently drops rows); paging controls on the members page (rejected: no account in the use case has a second page); answering 202 per REST §2's long-running convention (rejected: the invite itself is created synchronously; only the email is slow).
- **Patch-vs-refactor verdict:** this is a **patch**: bounded reads and a few signals on M208's surfaces, two of five items dropped.

## Discovery (consult log)

- **Consults** — Indy's direction, Oct 1, 2026, the night the M208 review ran: "Keep things simple, i dont want to add more scope on invite, the use case is John has fleets, and invites Bob, if bob for somereason didnt receive resends, and can revoke or cancel the invite." Auditor evidence re-read at `HEAD` of `feat/m208-team-accounts` (`3b61121c3` is `origin/main`):
  1. `one_page` is new on this branch — `git grep -n one_page origin/main -- rustd` finds only the test name `should_merge_both_libraries_into_one_page_each_keeping_its_label`; on the branch it has four callers (`invite.rs:155,233`, `member.rs:72,162`). The list statements carry no `LIMIT` (`sql/invite.rs:49-54,70-80`, `sql/member.rs:8-14`).
  2. Correction to the audit's item 3: both invite lists already filter `expires_at > $2` (`sql/invite.rs:52,78`) and a test proves it (`should_treat_invite_as_closed_everywhere_when_now_equals_expiry`, `integration_team.rs:573` at `HEAD`; an uncommitted split in this worktree moves it to `integration_team_invites.rs`).
  3. Correction to the audit's item 4: invite-email outcomes already reach PostHog as `invite_email_sent`, `invite_email_failed`, `invite_email_unconfigured` (`product/telemetry.rs:149,200-203` at `HEAD`); `docs/architecture/observability.md:401-405` lists nine production events and omits them, which §4 fixes alongside its own.
  4. Item 5: no M208 spec carries a REST §12 carve-out — `grep -rln "Performance Considerations" docs/v2/active docs/v2/done` printed nothing.
- **Metrics review** — adds `invite_created`, `invite_accepted`, `invite_revoked` and `agentsfleet_invite_emails_total`; no analytics or funnel playbook exists to update (`git grep -ln workspace_created -- docs playbooks` finds only specs); the event list in `docs/architecture/observability.md` is updated instead.
- **Skill-chain outcomes** — pending.
- **Deferrals** — this spec exists because of this answer:
  > Indy (Oct 01, 2026: 11:55 PM IST): "Defer to a follow-up (Recommended)" — context: AskUserQuestion, "Should the review findings that add new scope, rather than fix this PR, go to a follow-up spec? They are: paging for the invite and member lists, deleting old invite rows, hiding expired invites, team-action metrics, and sending the email after answering 201."
