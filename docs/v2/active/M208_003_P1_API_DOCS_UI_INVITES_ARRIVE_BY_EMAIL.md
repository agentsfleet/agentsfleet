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
**Status:** IN_PROGRESS
**Priority:** P1 — without email an owner must carry the link to the invitee by hand
**Categories:** API, DOCS, UI
**Batch:** B1 — third of three M208 workstreams in one Pull Request (PR)
**Branch:** `feat/m208-team-accounts`
**Baseline revision:** `3b61121c3c7da8b97cc348cca1d1dbfb99c3bce4`
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M208_001 (`core.invites`, its email-status columns, the invite routes, the members page, and §1's `smtp-relay` enumeration)
**Provenance:** LLM-drafted (Claude Opus 5.5, Sep 30, 2026; amended Oct 1, 2026 for the SMTP transport); decisions in Discovery are Indy's
**Canonical architecture:** `docs/AUTH.md` §Invites (added by M208_001)

---

## Overview

**Goal (testable):** `test_invite_email_carries_accept_link` — John invites bob@example.com; one message reaches the integration lane's mail sink from the bag's `from_address`, naming John and "John's account" and carrying `{dashboard}/invites/{invite_id}`; the invite's `email_status` reads `sent`.
**Problem:** M208_001's invites work only when the owner copies the link to the invitee. No mail service exists in this repository or its siblings; Clerk's invitations email only people without an account, so Bob, who has one, would get nothing.
**Solution summary:** The daemon renders one invite email from a template kept in this repository (askama) and sends it over Simple Mail Transfer Protocol (SMTP) with lettre, through the relay named by the `smtp-relay` platform bag in the admin-workspace vault. Resend is the relay behind that bag today; any SMTP relay plugs in. The invite commits first, so a send that fails or is unconfigured leaves a valid invite with `email_status` `failed` or `unconfigured`; the owner sees the status and can send again or copy the link. A registration playbook and a secret-sync case put the credentials in place.

## PR Intent & comprehension handshake

- **PR title (eventual):** the M208 PR (see M208_001)
- **Intent (one sentence):** the invitee learns of the invite from their inbox and joins in one click.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `rustd/crates/afd_api_tenant/src/handler/connector/connect.rs` — how a platform bag is found through `services.platform_admin_workspace()` and how "unconfigured" answers.
2. `rustd/Cargo.toml` — the one rustls provider the workspace links (`aws-lc-rs`, the comment above the crypto backend); the SMTP client links that provider and no second one.
3. `playbooks/operations/slack_app_registration/001_playbook.md` + `playbooks/lib/platform_secret_sync.sh` — the registration playbook and sync case the `smtp-relay` ones mirror.
4. https://resend.com/docs/send-with-smtp — host, ports, credentials, and the `Resend-Idempotency-Key` header.
5. `rustd/crates/afd_tenant/src/team/invitation/lifecycle.rs` — where an invite is issued; the send follows its commit.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_mail/` (+ `rustd/Cargo.toml`, `rustd/Cargo.lock`) | CREATE | render the invite (askama) and send it over SMTP (lettre): bounded deadline, idempotency header, typed outcome |
| `rustd/crates/afd_mail/templates/invite.{html,txt}` | CREATE | the invite email; the HTML ports the relay account's `invitation_dev` design; three variables |
| `rustd/crates/afd_mail/templates/layout.html` | CREATE | the brand every email extends: design-system colors (light, and dark where the client honours it), Bricolage Grotesque and Instrument Sans, the pulse-dot wordmark, the footer |
| `rustd/crates/afd_api_tenant/src/handler/tenant/invite_email.rs` | EDIT | a compile-time check that the email's `INVITE_VALID_DAYS` equals `INVITE_TTL_MS` |
| `rustd/crates/afd_tenant/src/team/invitation/lifecycle.rs` | EDIT | send after the invite commits; record status and attempts; send again |
| `rustd/crates/afd_http/src/route/tenant.rs` | EDIT | `POST /v1/tenants/me/invites/{invite_id}/send` |
| `rustd/crates/afd_api_tenant/src/handler/tenant/invite.rs` | EDIT | responses carry `email_status` |
| `rustd/crates/afd_core/src/{error_code,problem}/invite.rs` | EDIT | `UZ-INV-005` email unavailable (send-again only) |
| `docker-compose.yml`, `make/test-infra.mk`, `scripts/test-infra-ports.sh` | EDIT | a Mailpit SMTP sink the integration lane sends to and reads back; CI boots it through the same compose file (`.github/workflows/test-integration-rustd.yml` needs no edit) |
| `playbooks/operations/smtp_relay_registration/001_playbook.md` | CREATE | relay account, domain records, SMTP credentials, the 1Password item, sync |
| `playbooks/lib/platform_secret_sync.sh` | EDIT | `smtp-relay` case: `host`, `port`, `username`, `password`, `from_address` |
| `public/openapi.json` | EDIT | `email_status`, the send route |
| `ui/packages/app/lib/api/invites.ts` | EDIT | status and send-again client |
| `ui/packages/app/app/(dashboard)/settings/members/` | EDIT | email status on each `invited` row, a "Send again" action |
| `docs/AUTH.md` | EDIT | invite email: what is sent, through what, what a failure leaves |
| `~/Projects/docs` (branch `chore/m208-team-accounts-changelog`) | EDIT | `workspaces/teammates.mdx`: the email and its three states |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (`sent`, `failed`, `unconfigured`, the bag name and the idempotency header name as constants), ECL (a relay failure is `failed`, never a lost invite), FLL, NDC.
- `docs/RUST_ERROR_STANDARD.md` — `afd_mail` declares `ErrorKind` behind `error_shell!`; the relay's reply code is carried, not stringified.
- `docs/LOGGING_STANDARD.md` — send events carry the invite id and reply code, never the recipient address or body.
- `docs/REST_API_DESIGN_GUIDELINES.md` — the send route, six-place registration, problem body for `UZ-INV-005`.
- `docs/AUTH.md` — platform credentials live in the admin-workspace vault, never in configuration or arguments.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ERROR REGISTRY | yes — `UZ-INV-005` | declared with a negative test |
| LOGGING | yes | structured send events without addresses |
| UFS | yes | status, bag and header constants |
| UI / DESIGN TOKEN | yes — status badge, action | design-system primitives and token utilities |
| File & Function Length (≤350/≤50/≤70) | yes | `afd_mail` keeps render and send apart; the email body lives in its template files |

## Prior-Art / Reference Implementations

- **Reference:** `slack-app` in `playbooks/lib/platform_secret_sync.sh` and `playbooks/operations/slack_app_registration/001_playbook.md` — the provisioning path `smtp-relay` takes.
- **Reference:** `rustd/crates/afd_outbound/` — loopback fakes in tests, the shape the SMTP refusal and stall tests take.

## Sections (implementation slices)

### §1 — Provisioning: the credentials reach the vault by a playbook

`playbooks/operations/smtp_relay_registration/001_playbook.md` walks the human steps: a relay account (Resend today); verify `agentsfleet.net` with the DNS records the relay lists; SMTP credentials (Resend: host `smtp.resend.com`, port `465`, username `resend`, password an API key with sending access); the 1Password item `smtp-relay` with `host`, `port`, `username`, `password` and `from_address` in `ZMB_CD_DEV` and `ZMB_CD_PROD`; then `playbooks/lib/platform_secret_sync.sh smtp-relay`. Both items exist (M208_001 Discovery). The bag has no test field: the integration lane reaches its sink through a `test-util` seam.

- **Dimension 1.1** — the sync script's `smtp-relay` case maps exactly `host`, `port`, `username`, `password`, `from_address` → Test `test_secret_sync_maps_smtp_relay` — DONE (`playbooks/lib/platform_secret_sync_test.sh`)

### §2 — One send per invite, after the invite commits

Creating an invite commits the row, then increments `email_attempts` and commits, then sends one message: from `from_address`, to the invite's email, subject "You're invited to join {account_name} on agentsfleet", plain-text and HTML parts rendered from the repository's template, and no tracking. The template takes exactly three variables, `inviter_name`, `account_name` ("John's account") and `invite_url`; company name and address are template text. The transport is implicit Transport Layer Security (TLS) on port 465 and required STARTTLS on any other port, over the workspace's one rustls provider; plaintext is allowed only to a loopback host (`127.0.0.1`, `::1`, `localhost`), so the message never leaves the machine — that is how the local Docker daemon and the integration lane reach Mailpit. The message carries `Resend-Idempotency-Key: invite-{invite_id}-{attempt}`, which Resend deduplicates on and other relays ignore, and the send runs under a named deadline (`MAIL_SEND_DEADLINE`); a connection failure is retried once under the same key. The outcome is recorded as `email_status` and `email_sent_at`, and the create response carries it.

Rendering and delivery stay apart: `render_invite` returns a `RenderedEmail` (subject, HTML, text) and `deliver` hands it to a `Mailer`; production's mailer is lettre's SMTP transport, unit tests use lettre's `StubTransport`.

- **Dimension 2.1** — a created invite sends one message with the link and both names → Test `test_invite_email_carries_accept_link` — DONE (`afd_api/tests/integration_invite_email.rs`)
- **Dimension 2.2** — a retried attempt carries the same idempotency header → Test `test_send_retry_reuses_idempotency_key` — DONE (`afd_api/tests/integration_invite_email.rs`)
- **Dimension 2.3** — the address and body never appear in logs → Test `test_send_logs_carry_no_address` — DONE (`afd_mail/src/mailer/tests.rs`)
- **Dimension 2.4** — a display name carrying markup renders escaped in the HTML part → Test `test_invite_template_escapes_names` — DONE (`afd_mail/src/invite/tests.rs`)
- **Dimension 2.5** — the rendered HTML and text parts match reviewed snapshots → Test `test_invite_render_snapshots` — DONE (`afd_mail/src/invite/tests.rs`; the HTML ports `invitation_dev` onto `templates/layout.html` in the design system's colors and faces)
- **Dimension 2.6** — `deliver` builds the envelope, subject, idempotency header and both MIME parts → Test `test_deliver_builds_message` — DONE (`afd_mail/src/deliver/tests.rs`)
- **Dimension 2.7** — plaintext SMTP is refused for any non-loopback host → Test `test_plaintext_refused_off_loopback` — DONE (`afd_mail/src/relay/tests.rs`)

### §3 — A failed or unconfigured send leaves a valid invite

With no `smtp-relay` bag, the invite still returns 201 with `email_status: "unconfigured"`, and one error event names the missing bag. A relay refusal (any 4xx or 5xx SMTP reply, authentication failure included), a lost connection after the retry, or the deadline leaves `email_status: "failed"` with the reply code in the event; the invite stays pending and acceptable.

- **Dimension 3.1** — unconfigured → 201, `unconfigured`, one error event naming `smtp-relay` → Test `test_unconfigured_email_keeps_invite` — DONE (`afd_api/tests/integration_invite_email.rs`)
- **Dimension 3.2** — relay 4xx, 5xx, authentication refusal and stall → 201, `failed`, invite acceptable → Test `test_failed_email_keeps_invite` — DONE (`afd_api/tests/integration_invite_email.rs`)

### §4 — The owner sees the status and can send again

Each `invited` row on the members page shows "Email sent", "Email not sent" or "Email not set up" beside its copy-link action, with a "Send again" action when not sent. `POST /v1/tenants/me/invites/{invite_id}/send` is a new attempt (new key); when email is unconfigured or the relay refuses it answers `503 UZ-INV-005` and the status records it.

- **Dimension 4.1** — send-again after a failure sends under a new key and flips to `sent` → Test `test_send_again_after_failure` — DONE (`afd_api/tests/integration_invite_email.rs`)
- **Dimension 4.2** — send-again while unconfigured is `503 UZ-INV-005` → Test `test_send_again_unconfigured_refused` — DONE (`afd_api/tests/integration_invite_email.rs`)
- **Dimension 4.3** — the members page shows each status with its actions → Test `test_members_page_shows_email_status` — written (`ui/packages/app/tests/e2e/acceptance/team-members.spec.ts`); the view is unit-proven in `MembersView.test.tsx`; the journey runs on DEV after merge

### §5 — Real delivery on DEV

After §1's sync runs on DEV and this change reaches DEV, an invite to a test mailbox arrives, and its link, opened while signed in as that mailbox's account, accepts. DEV runs `main`, so this runs after merge, as M208_001's R2 does.

- **Dimension 5.1** — a real invite email arrives and its link accepts, recorded with the relay's message id → Test `test_dev_invite_email_round_trip` (manual — Indy, after merge)

### §6 — Documentation

`docs/AUTH.md` states what is sent, through which bag, and what each failure leaves; `workspaces/teammates.mdx` on the docs branch describes the email and its three states.

- **Dimension 6.1** — `docs/AUTH.md` names the three email states and the `smtp-relay` bag → Test `test_auth_doc_names_email_states` — DONE (`playbooks/operations/smtp_relay_registration/smtp_relay_registration_test.sh`)

## Interfaces

```
POST /v1/tenants/me/invites                   201 {…M208_001 fields, email_status:"sent"|"failed"|"unconfigured"}
GET  /v1/tenants/me/invites                   items gain {email_status, email_sent_at}
POST /v1/tenants/me/invites/{invite_id}/send  200 {email_status:"sent"} | 503 UZ-INV-005
smtp-relay bag   {host, port, username, password, from_address}    (admin-workspace vault)
SMTP send        implicit TLS on 465, STARTTLS required elsewhere; AUTH with username and password
                 header Resend-Idempotency-Key: invite-{invite_id}-{attempt}
Template         rustd/crates/afd_mail/templates/invite.{html,txt}  {inviter_name, account_name, invite_url}
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unconfigured | no `smtp-relay` bag, or no admin workspace | invite 201 `unconfigured`; one error event naming the bag; owner copies the link |
| Relay refusal | bad credentials, unverified domain, invalid recipient, rate limit (SMTP 4xx or 5xx) | invite 201 `failed`; event with the reply code; "Send again" |
| Relay down | connection refused or lost, or the deadline | one retry under the same key, then `failed` |
| Crash after send, before recording | process dies mid-request | status stays unrecorded (`failed` on read); an owner's send-again uses a new attempt, so at most one extra email |
| Duplicate on retry | the relay accepted the message but its reply was lost | Resend deduplicates on `Resend-Idempotency-Key`; a relay without it may deliver one extra copy per attempt |

## Invariants

1. The invite exists before any email is sent — the send runs after the invite's commit; a test fails the send and asserts the row.
2. One attempt sends at most one email through Resend — the attempt counter commits before the send and names the idempotency header; tested against a listener that records headers.
3. The recipient address and body never reach logs — send events take only ids and reply codes; a log-capture test asserts it.
4. The relay password never leaves the vault read — it is loaded per send from the admin-workspace bag and never logged or returned.
5. No variable reaches the HTML part unescaped — askama escapes by default; `test_invite_template_escapes_names` renders a hostile display name.
6. Credentials never cross the network in the clear — TLS to every relay host; plaintext only to loopback; `test_plaintext_refused_off_loopback`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `invite_email_sent` | product | the relay accepts the message | invite id, attempt, reply code | no address | `test_invite_email_carries_accept_link` |
| `invite_email_failed` | ops | refusal, lost connection or deadline | invite id, attempt, reply code | no address, no body | `test_failed_email_keeps_invite` |
| `invite_email_unconfigured` | ops | no `smtp-relay` bag | invite id | none needed | `test_unconfigured_email_keeps_invite` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_secret_sync_maps_smtp_relay` | `smtp-relay` case lists exactly the five fields |
| 2.1 | integration | `test_invite_email_carries_accept_link` | Mailpit receives one message: from, to, subject, link, inviter name, account name; HTML and text parts |
| 2.2 | integration | `test_send_retry_reuses_idempotency_key` | a loopback listener drops the first connection → the retry carries the same `Resend-Idempotency-Key` |
| 2.3 | unit | `test_send_logs_carry_no_address` | captured events hold no `@` address and no body text |
| 2.4 | unit | `test_invite_template_escapes_names` | inviter name `<b>x</b>` → the HTML part holds `&lt;b&gt;x&lt;/b&gt;` |
| 2.5 | unit | `test_invite_render_snapshots` | fixed names and link → HTML and text equal the reviewed `insta` snapshots |
| 2.6 | unit | `test_deliver_builds_message` | `StubTransport` records one message: envelope to/from, subject, `Resend-Idempotency-Key`, `text/plain` and `text/html` parts |
| 2.7 | unit | `test_plaintext_refused_off_loopback` | bag host `smtp.example.test` with plaintext → refused before connecting; `127.0.0.1` → allowed |
| 3.1 | integration | `test_unconfigured_email_keeps_invite` | no bag → 201 `unconfigured`, one error event, invite listed |
| 3.2 | integration | `test_failed_email_keeps_invite` | listener answers 535, 450 and 550, then stalls → 201 `failed`, invite acceptable |
| 4.1 | integration | `test_send_again_after_failure` | fail then send again → new key, `sent` |
| 4.2 | integration | `test_send_again_unconfigured_refused` | no bag → `503 UZ-INV-005` |
| 4.3 | e2e | `test_members_page_shows_email_status` | each status renders its label and actions |
| 5.1 | manual | `test_dev_invite_email_round_trip` | Indy on DEV after merge: email arrives, link accepts; evidence = relay message id + screenshot in Session Notes |
| 6.1 | unit | `test_auth_doc_names_email_states` | `docs/AUTH.md` → names `sent`, `failed`, `unconfigured` and `smtp-relay` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Email sends once, failures keep the invite (§2, §3, §4) | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | Owner sees status and actions (§4) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts --project=journeys -g test_members_page_shows_email_status` | `1 passed` | P0 | |
| R3 | Real email on DEV (§5) | manual: Session Notes carry the relay message id and the accepted invite | present | P1 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Versions in sync | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears verbatim above (`make test-integration-rustd` is R1). See `dispatch/lifecycle.md` for timing.

**Grading protocol (VERIFY):** run each Verify command verbatim; Graded = ✅/❌ + one decisive output line; repository rows point to the final `orly gate pr` results. **Ship gate:** any ❌ returns to EXECUTE; R3 ❌ needs an Indy-acked deferral quote (it runs after merge); a P0 is MOVED only into a named successor that carries it, with Indy's quote, and is never ✅.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Any email other than invites (receipts, alerts, digests).
- Bounce and complaint webhooks from the relay — a later change if volume warrants.
- Localized templates, and more than one design.
- Resend's stored templates — they exist only on its HTTP API, not over SMTP (Discovery).

---

## Product Clarity (authoring record)

1. **Successful user moment** — Bob opens "You're invited to join John's account on agentsfleet", clicks the link, signs in, and lands on the Invites page with Accept ready.
2. **Preserved user behaviour** — M208_001's copy-link path keeps working whatever email does.
3. **Optimal-way check** — direct: one transactional send per invite; bounce handling waits until volume says it matters.
4. **Rebuild-vs-iterate** — new, small: one crate, one template, one playbook.
5. **What we build** — the render and send, its status, send-again, the playbook, the sync case and a mail sink for the integration lane.
6. **What we do NOT build** — other emails, bounce webhooks, stored provider templates.
7. **Fit with existing features** — follows the platform-credential path every connector app uses; must not delay or fail invite creation.
8. **Surface order** — User Interface (UI) first on the members page; the status is in the API for scripts.
9. **Dashboard restraint** — "Send again" appears only when a send did not succeed; no delivery claims beyond what the relay accepted.
10. **Confused-user next step** — "Email not set up" points the owner to the copy-link action and names the playbook for operators.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** provisioning first (§1), the send (§2), failure states (§3), owner actions (§4), real delivery (§5), docs (§6).
- **Alternatives considered:** Clerk invitations (rejected: no email for existing accounts); Postmark and Amazon Simple Email Service (SES) (Indy chose Resend); Resend's HTTP API, this spec's first draft (replaced by SMTP, Discovery); sending from the Next.js app (rejected: API-created invites would get no email).
- **Patch-vs-refactor verdict:** this is a **patch**: one outbound send beside M208_001's invite write.

## Discovery (consult log)

- **Consults** — Sep 30, 2026, Indy: "I want the invite emails into an account, since that will be helpful"; provider "Resend (Recommended)". Source findings: no mail service in this repository or its siblings (grep of `~/Projects` for Resend, Postmark, SendGrid, SES and SMTP clients); platform credentials resolve through `services.platform_admin_workspace()` (`handler/connector/connect.rs:73`).
- **Transport** — Sep 30, 2026, Indy: "i thought its afd_mail? with smtp_relay", in reply to the recommendation to call Resend's HTTP API (decision `a3d20406`). The bag became `smtp-relay` (M208_001 §1) and any SMTP relay plugs in. A real SMTP sink (Mailpit) replaces the HTTP loopback fake for delivery tests; refusals and stalls keep a loopback listener.
- **Template** — decisions `9c96e2e8` and `956040a2`: Indy wants templates over SMTP. Resend's stored templates exist only on `POST /emails` (resend.com/docs/api-reference/emails/send-email), and its SMTP guide covers headers and `Resend-Idempotency-Key` only (resend.com/docs/send-with-smtp), so the template lives here, rendered by askama, which checks variables at compile time and escapes HTML. Three variables, since only the invitee's address is known at invite time: `inviter_name`, `account_name`, `invite_url`; company name and address are template text; no `first_name`. The HTML ports Indy's Resend dev template `invitation_dev` (sender hello@agentsfleet.net; `invitation_dev-1` sends from hello@agentsfleet.dev).
- **Amendment** — Oct 1, 2026: this spec moved from Resend's HTTP API to SMTP. §1's playbook, the sync case, §2's transport, §3's failure codes, Interfaces, Failure Modes, Metrics and the test names follow; the invite module is `team/invitation/` since M208_001's refactor (`bfb021bf8`); Dimension 2.4 and Invariant 5 are added for template escaping; Dimension 6.1 tests `docs/AUTH.md`, which this repository can read, in place of the public page.
- **Test layers and local eyeball** — Oct 1, 2026, Indy: "Do all three. Anything less misses a failure class" — askama with `insta` snapshots for the template, lettre's `StubTransport` for the send path, Mailpit for the real SMTP exchange; Mailpit runs in `docker-compose.yml`, in Continuous Integration (CI) and in `make test-integration-rustd`. The members page and the email are eyeballed on the local Docker stack ("it can be on docker like here, where i can check on http://... url"): the daemon sends to Mailpit and Indy reads the mail at Mailpit's web page. That needs plaintext SMTP outside `test-util`, so the rule is now loopback-only plaintext (§2, Invariant 6). Template HTML is fetched from Resend's API (Indy: "Fetch via Resend API").
- **Metrics review** — pending.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none. §5 runs after merge because DEV runs `main`; R3 needs Indy's ack quote before the Pull Request.
