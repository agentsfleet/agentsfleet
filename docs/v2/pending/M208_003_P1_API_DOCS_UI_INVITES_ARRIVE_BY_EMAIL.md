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

# M208_003: An invite arrives by email with a working accept link, and a failed send never loses the invite

**Prototype:** v2.0.0
**Milestone:** M208
**Workstream:** 003
**Date:** Sep 30, 2026
**Status:** PENDING
**Priority:** P1 — without email an owner must carry the link to the invitee by hand
**Categories:** API, DOCS, UI
**Batch:** B1 — third of three M208 workstreams in one Pull Request (PR)
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M208_001 (`core.invites`, its email-status columns, the invite routes, the members page, and §1's `resend-app` enumeration)
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 30, 2026); decisions in Discovery are Indy's
**Canonical architecture:** `docs/AUTH.md` §Invites (added by M208_001)

---

## Overview

**Goal (testable):** `test_invite_email_carries_accept_link` — John invites bob@example.com; one email leaves through Resend from the configured address, naming John and carrying `{dashboard}/invites/{invite_id}`; the invite's `email_status` reads `sent`.
**Problem:** M208_001's invites work only when the owner copies the link to the invitee. No mail service exists in this repository or its siblings; Clerk's invitations email only people without an account, so Bob, who has one, would get nothing.
**Solution summary:** The daemon sends one transactional email per invite through Resend's HTTPS API, with the key held as the `resend-app` platform bag in the admin-workspace vault. The invite commits first, so a send that fails or is unconfigured leaves a valid invite with `email_status` `failed` or `unconfigured`; the owner sees the status and can send again or copy the link. A registration playbook and a secret-sync case put the key in place.

## PR Intent & comprehension handshake

- **PR title (eventual):** the M208 PR (see M208_001)
- **Intent (one sentence):** the invitee learns of the invite from their inbox and joins in one click.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_api_tenant/src/handler/connector/connect.rs` — how a platform `<provider>-app` bag is found through `services.platform_admin_workspace()` and how "unconfigured" answers.
2. `rustd/crates/afd_outbound/src/lib.rs` — bounded outbound HTTPS to a vendor, and its loopback-fake integration tests.
3. `playbooks/operations/slack_app_registration/001_playbook.md` + `playbooks/lib/platform_secret_sync.sh` — the registration playbook and sync case the Resend ones mirror.
4. https://resend.com/docs/api-reference/emails/send-email — `POST /emails`, bearer key, `Idempotency-Key` header.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_mail/` (+ `rustd/Cargo.toml`, `rustd/Cargo.lock`) | CREATE | one Resend send: bounded deadline, idempotency key, typed outcome |
| `rustd/crates/afd_tenant/src/invite/` | EDIT | send after the invite commits; record status and attempts; send again |
| `rustd/crates/afd_http/src/route/tenant.rs` | EDIT | `POST /v1/tenants/me/invites/{invite_id}/send` |
| `rustd/crates/afd_api_tenant/src/handler/tenant/invite.rs` | EDIT | responses carry `email_status` |
| `rustd/crates/afd_core/src/error_code/invite.rs` | EDIT | `UZ-INV-005` email unavailable (send-again only) |
| `playbooks/operations/resend_email_registration/001_playbook.md` | CREATE | Resend account, domain records, key, 1Password item, sync |
| `playbooks/lib/platform_secret_sync.sh` | EDIT | `resend-app` case: `api_key`, `from_address`, optional `api_base` |
| `public/openapi.json` | EDIT | `email_status`, the send route |
| `ui/packages/app/lib/api/invites.ts` | EDIT | status and send-again client |
| `ui/packages/app/app/(dashboard)/settings/members/` | EDIT | status chip, "Send again", copy link on every pending invite |
| `docs/AUTH.md` | EDIT | invite email: what is sent, from where, what a failure leaves |
| `~/Projects/docs` (branch `chore/m208-team-accounts-changelog`) | EDIT | invites page: the email and its failure states |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (`sent`, `failed`, `unconfigured` and the bag name as constants), ECL (a provider failure is `failed`, never a lost invite), FLL, NDC.
- `docs/RUST_ERROR_STANDARD.md` — `afd_mail` declares `ErrorKind` behind `error_shell!`; the provider's status is carried, not stringified.
- `docs/LOGGING_STANDARD.md` — send events carry the invite id and provider status, never the recipient address or body.
- `docs/REST_API_DESIGN_GUIDELINES.md` — the send route, six-place registration, problem body for `UZ-INV-005`.
- `docs/AUTH.md` — platform credentials live in the admin-workspace vault, never in configuration or arguments.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ERROR REGISTRY | yes — `UZ-INV-005` | declared with a negative test |
| LOGGING | yes | structured send events without addresses |
| UFS | yes | status and bag constants |
| UI / DESIGN TOKEN | yes — status chip, button | design-system primitives and token utilities |
| File & Function Length (≤350/≤50/≤70) | yes | `afd_mail` stays one small module; the email body lives in its own template module |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_outbound/` — bounded outbound vendor calls with loopback fakes in tests.
- **Reference:** `slack-app` in `playbooks/lib/platform_secret_sync.sh` and `playbooks/operations/slack_app_registration/001_playbook.md` — the provisioning path `resend-app` takes.

## Sections (implementation slices)

### §1 — Provisioning: the key reaches the vault by a playbook

`playbooks/operations/resend_email_registration/001_playbook.md` walks the human steps: Resend account; add and verify `agentsfleet.net` (the DNS records Resend lists); an API key with sending access; the 1Password item `resend-app` with `api_key` and `from_address` (`agentsfleet <invites@agentsfleet.net>`); then `playbooks/lib/platform_secret_sync.sh resend-app`. `api_base` is optional and exists so tests point the client at a loopback fake.

- **Dimension 1.1** — the sync script's `resend-app` case maps exactly `api_key`, `from_address`, `api_base` → Test `test_secret_sync_maps_resend_app`

### §2 — One send per invite, after the invite commits

Creating an invite commits the row, then increments `email_attempts` and commits, then sends one email: from `from_address`, to the invite's email, subject "{inviter} invited you to their agentsfleet account", plain text and HTML bodies with the inviter's display name, the accept link and the expiry date, and no tracking. The call carries `Idempotency-Key: invite-{invite_id}-{attempt}` and a named deadline (`MAIL_SEND_DEADLINE`); a transport failure is retried once under the same key, so a retry never sends twice. The outcome is recorded as `email_status` and `email_sent_at`, and the create response carries it.

- **Dimension 2.1** — a created invite sends one email with the link, name and expiry → Test `test_invite_email_carries_accept_link`
- **Dimension 2.2** — a transport retry reuses the key; the fake sees one delivery → Test `test_send_retry_reuses_idempotency_key`
- **Dimension 2.3** — the address and body never appear in logs → Test `test_send_logs_carry_no_address`

### §3 — A failed or unconfigured send leaves a valid invite

With no `resend-app` bag, the invite still returns 201 with `email_status: "unconfigured"`, and one error event names the missing bag. A provider refusal (4xx, including 429) or a deadline or 5xx leaves `email_status: "failed"` with the provider status in the event; the invite stays pending and acceptable.

- **Dimension 3.1** — unconfigured → 201, `unconfigured`, one error event naming `resend-app` → Test `test_unconfigured_email_keeps_invite`
- **Dimension 3.2** — provider 4xx, 429, 5xx and timeout → 201, `failed`, invite acceptable → Test `test_failed_email_keeps_invite`

### §4 — The owner sees the status and can send again

Each pending invite shows "Email sent", "Email not sent" or "Email not set up", with "Copy link" always and "Send again" when not sent. `POST /v1/tenants/me/invites/{invite_id}/send` is a new attempt (new key); when email is unconfigured or the provider refuses it answers `503 UZ-INV-005` and the status records it.

- **Dimension 4.1** — send-again after a failure sends under a new key and flips to `sent` → Test `test_send_again_after_failure`
- **Dimension 4.2** — send-again while unconfigured is `503 UZ-INV-005` → Test `test_send_again_unconfigured_refused`
- **Dimension 4.3** — the members page shows each status with its actions → Test `test_members_page_shows_email_status`

### §5 — Real delivery on DEV

After §1's playbook runs on DEV, an invite to a test mailbox arrives, and its link, opened while signed in as that mailbox's account, accepts.

- **Dimension 5.1** — a real invite email arrives and its link accepts, recorded with the Resend message id → Test `test_dev_invite_email_round_trip` (manual — Indy, after provisioning)

### §6 — Documentation

`docs/AUTH.md` states what is sent, from where, and what each failure leaves; the public invites page on the docs branch.

- **Dimension 6.1** — the public invites page names the three email states → Test `test_invite_docs_name_email_states`

## Interfaces

```
POST /v1/tenants/me/invites                 201 {…M208_001 fields, email_status:"sent"|"failed"|"unconfigured"}
GET  /v1/tenants/me/invites                 items gain {email_status, email_sent_at}
POST /v1/tenants/me/invites/{invite_id}/send  200 {email_status:"sent"} | 503 UZ-INV-005
resend-app bag  {api_key, from_address, api_base?}    (admin-workspace vault)
Resend call     POST {api_base or https://api.resend.com}/emails
                Authorization: Bearer {api_key}; Idempotency-Key: invite-{invite_id}-{attempt}
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unconfigured | no `resend-app` bag, or no admin workspace | invite 201 `unconfigured`; one error event naming the bag; owner copies the link |
| Provider refusal | bad key, unverified domain, invalid recipient, 429 | invite 201 `failed`; event with provider status; "Send again" |
| Provider down | 5xx or deadline | one retry under the same key, then `failed` |
| Crash after send, before recording | process dies mid-request | status stays unrecorded (`failed` on read); an owner's send-again uses a new attempt, so at most one extra email |
| Duplicate on retry | network retry of one attempt | Resend deduplicates on the idempotency key |

## Invariants

1. The invite exists before any email is sent — the send runs after the invite's commit; a test fails the send and asserts the row.
2. One attempt sends at most one email — the attempt counter commits before the send and names the idempotency key; tested with a fake that records keys.
3. The recipient address and body never reach logs — send events take only ids and statuses; a log-capture test asserts it.
4. The key never leaves the vault read — it is loaded per send from the admin-workspace bag and never logged or returned.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `invite_email_sent` | product | Resend accepts the send | invite id, attempt, provider message id | no address | `test_invite_email_carries_accept_link` |
| `invite_email_failed` | ops | refusal, 5xx or deadline | invite id, attempt, provider status | no address, no body | `test_failed_email_keeps_invite` |
| `invite_email_unconfigured` | ops | no `resend-app` bag | invite id | none needed | `test_unconfigured_email_keeps_invite` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_secret_sync_maps_resend_app` | `resend-app` case lists exactly the three fields |
| 2.1 | integration | `test_invite_email_carries_accept_link` | loopback fake receives one request: from, to, link, inviter name, expiry |
| 2.2 | integration | `test_send_retry_reuses_idempotency_key` | first request drops → retry with the same key → one delivery |
| 2.3 | unit | `test_send_logs_carry_no_address` | captured events hold no `@` address and no body text |
| 3.1 | integration | `test_unconfigured_email_keeps_invite` | no bag → 201 `unconfigured`, one error event, invite listed |
| 3.2 | integration | `test_failed_email_keeps_invite` | fake answers 422, 429, 500, stalls → 201 `failed`, invite acceptable |
| 4.1 | integration | `test_send_again_after_failure` | fail then send again → new key, `sent` |
| 4.2 | integration | `test_send_again_unconfigured_refused` | no bag → `503 UZ-INV-005` |
| 4.3 | e2e | `test_members_page_shows_email_status` | each status renders its label and actions |
| 5.1 | manual | `test_dev_invite_email_round_trip` | Indy on DEV: email arrives, link accepts; evidence = Resend message id + screenshot in Session Notes |
| 6.1 | unit | `test_invite_docs_name_email_states` | the public page names `sent`, `failed`, `unconfigured` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Email sends once, failures keep the invite (§2, §3, §4) | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | Owner sees status and actions (§4) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts --project=journeys -g test_members_page_shows_email_status` | `1 passed` | P0 | |
| R3 | Real email on DEV (§5) | manual: Session Notes carry the Resend message id and the accepted invite | present | P1 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Versions in sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears verbatim above (`make test-integration-rustd` is R1). See `dispatch/lifecycle.md` for timing.

**Grading protocol (VERIFY):** run each Verify command verbatim; Graded = ✅/❌ + one decisive output line; repository rows point to the final `orly gate pr` results. **Ship gate:** any ❌ returns to EXECUTE; R3 ❌ needs an Indy-acked deferral quote (it waits on provisioning); a P0 is MOVED only into a named successor that carries it, with Indy's quote, and is never ✅.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Any email other than invites (receipts, alerts, digests).
- Bounce and complaint webhooks from Resend — a later change if volume warrants.
- Localized or branded HTML templates beyond one plain design.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Bob opens "John invited you to their agentsfleet account", clicks the link, signs in, and lands on the Invites page with Accept ready.
2. **Preserved user behaviour** — M208_001's copy-link path keeps working whatever email does.
3. **Optimal-way check** — direct: one transactional send per invite; bounce handling waits until volume says it matters.
4. **Rebuild-vs-iterate** — new, small: one crate, one call, one playbook.
5. **What we build** — the send, its status, send-again, the playbook and the sync case.
6. **What we do NOT build** — other emails, bounce webhooks, template branding.
7. **Fit with existing features** — follows the platform-credential path every connector app uses; must not delay or fail invite creation.
8. **Surface order** — User Interface (UI) first on the members page; the status is in the API for scripts.
9. **Dashboard restraint** — "Send again" appears only when a send did not succeed; no delivery claims beyond what Resend accepted.
10. **Confused-user next step** — "Email not set up" links the owner to the copy-link action and names the playbook for operators.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** provisioning first (§1), the send (§2), failure states (§3), owner actions (§4), real delivery (§5), docs (§6).
- **Alternatives considered:** Clerk invitations (rejected: no email for existing accounts); Postmark and Amazon Simple Email Service (SES) (Indy chose Resend); sending from the Next.js app (rejected: API-created invites would get no email).
- **Patch-vs-refactor verdict:** this is a **patch**: one outbound call beside M208_001's invite write.

## Discovery (consult log)

- **Consults** — Sep 30, 2026, Indy: "I want the invite emails into an account, since that will be helpful"; provider "Resend (Recommended)". Source findings: no mail service in this repository or its siblings (grep of `~/Projects` for Resend, Postmark, SendGrid, SES and Simple Mail Transfer Protocol (SMTP) clients); platform credentials resolve through `services.platform_admin_workspace()` (`handler/connector/connect.rs:73`).
- **Metrics review** — pending.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
