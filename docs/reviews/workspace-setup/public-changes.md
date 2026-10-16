---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Public documentation handoff for the chat cutover

## What it is

A reviewable change list for a branch in `~/Projects/docs`, plus the owner-approved REST-guide amendment. This worktree does not edit that repository, commit, push, or deploy. Published claims wait for implementation and passing acceptance evidence.

## Why it exists

Public endpoint and command behavior must match the one-PR cutover. The exact proposed resource definitions live in [the resource reference](../../architecture/api_resources.md), not in duplicated examples with different paths.

## How it behaves

### Approved REST-guide amendment

L1 explicitly authorizes the following clause for the owning harness source of `.orly/docs/REST_API_DESIGN_GUIDELINES.md`. The owning guide at `/Users/kishore/Projects/orly/docs/REST_API_DESIGN_GUIDELINES.md` has been amended and narrowly materialized into this worktree using Orly's renderer and installation transaction. It is a deliberate exception to the generic header-key/asynchronous-operation guidance, covering both conversation resources:

> Conversation sends to fleet and creator-private draft `/messages` resources use one required UUIDv7 `operation_id` in the JSON body. They reject an `Idempotency-Key` header. A repeated key with the same normalized message and actor recovers the accepted message; a changed message or actor conflicts. They return 202 and a receipt, with `Location` pointing to that conversation's messages collection. Pending state, answers, and usage are read through messages and its authorized live stream. They do not create a public turn-status or generic operation resource. Authorization and lifecycle checks apply before replay; creator-private drafts never appear in workspace streams.

The installed guide matches the owning render; only that managed file and its recorded digest changed. Conversation handlers still require implementation and behavior evidence.

### Required public pages and examples

| Area / decision | Required revision |
|---|---|
| Install tutorial and command reference / L5 | Show partial `installing`, missing checks, fleet ID, safe retry command, nonzero exit, and separate initial-run/schedule-sync outcomes. |
| Save tutorial / L6,L7 | Explain Save vs Save and run now, unchanged triggers, explicit Save to library, and unchanged Library install without a draft. |
| Library reference, CLI verbs, app walkthrough / L11,L12 | Replace GET/DELETE library-entries with owned literal path and Library member delete; Inspect/revisions/history use fleet-libraries. No alias. |
| Fleet API and setup editor / L13 | PATCH status/name only; move all setup updates to POST fleet revisions with candidate/digest/setup tag. Both clients edit the same typed Definition. |
| Concurrency and retry / L14,L15 | Mandatory If-Match on fleet PATCH, 428/412 examples, body UUIDv7 fleet/draft conversation key, header key on creation/publication, one identity per action. |
| Definition/configuration reference / L19,L20 | One native validator; replace daily_dollars/monthly_dollars with daily_nanos/monthly_nanos decimal strings, including monthly budgets, null/zero rejection, caps and native JSON examples. Remove file/bundle/frontmatter import and export instructions. |
| Terminal authoring | Start or resume a private draft, chat, inspect the exact review and explicitly Save. Remove path/GitHub/template imports, exports and their flags; retain installation from a pinned Library revision and live GitHub runtime tools/triggers. |
| Private draft API | Creator scope, unlimited drafts, message states/usage and bounded failure/action fields, cutoff Stop, delete and 30-day close retention. Explain queued acceptance and failure after acceptance. |
| Models and Account | Workspace-owner eligibility for joined members, authoring/runtime distinction, idle version-checked model replacement after disablement, reload retention and pinned earlier turns. Joined members use authorized owner recovery links. |
| Setup review and approvals | Immutable requirements vs observed readiness, review digest, grants before activation, explicit resume/Kill, policy Needs re-review. |
| Privacy / deletion | Server204 boundary, runner memory limits, 24-hour logical spool expiry and pre-read restart cleanup. State physical copies may remain on powered-off hosts and encryption alone cannot prevent offline decryption with retained keys. Provider/backups retention, content-free accounting, shared setup unaffected. |
| Runner reference | Tagged claim, combined worker_count including renewal expiry guards, daemon-mediated capability/model-calls plus renew/report/activity, candidate version receipts, token/time/spend bounds and unknown usage. |
| Fleet messages | Accepted means durably recorded, potentially awaiting delivery. Existing message reads recover receipt-less pending admissions; same operation_id handles uncertain replies. Initial start.accepted additionally requires append and saved receipt. |
| Schedule reference | Registration after activation, honest sync failure and Retry sync, inherited Restore/version tag. Stable-ID fix prerequisite; content-revision fencing remains a follow-up. |
| Changelog | Enumerate route removals, required keys/preconditions, budget units, partial install result, private draft flow, and explicit Library publication. Link revised reference pages. |

Generate the public OpenAPI definition from implemented route/wire types; do not patch a speculative public OpenAPI JSON now. Examples must include response status, exact fields, error codes, and relevant retry/precondition headers. Remove obsolete client paths in published reference and tutorials together.

### Coordinated acceptance procedure

This is a required implementation/release procedure, not a completed run. `make/acceptance.mk:13–26` runs browser/CLI suites against configured services. `ui/packages/app/playwright.acceptance.config.ts:178–186` conditionally starts only the app. A passing lane cannot establish which daemon or runner served it without the following evidence.

1. Produce one expected build manifest for app, daemon, runner, and CLI. Record full source revisions, source-tree digests for dirty builds, output hashes/deployment identifiers, schema migration, policy marker, tokenizer/provider versions, and environment name. Record app/API origins and the expected runner IDs. Keep secrets and private text out of the manifest.
2. Build/start this worktree's daemon with the existing make up flow and FOLLOW_LOGS=0 on a prepared isolated development stack. It builds and migrates the selected checkout; use the chosen release-reset procedure only at its authorized boundary. Record actual /healthz commit/version and /readyz dependencies. Existing startup evidence: `scripts/local-dev.sh:153–171`; health build identity: `rustd/crates/afd_api/src/router/probes.rs:32–45,103–114`.
3. Build and start the runner binary from that same candidate on a prepared Linux runner host. Use the existing onboarding service configuration with the runner-reachable AGENTSFLEET_API_URL and separate storage home. Record the running process's binary hash against the manifest and observe authenticated heartbeat advancement. The macOS helper only prints runner environment; it does not start a matching Linux runner (`scripts/local-dev.sh:173–180`). A published installer fetching another revision is insufficient evidence.
4. Build the CLI from the candidate and record the exact cli/dist build-file hash used by the existing CLI fixture helper. Set its AGENTSFLEET_API_URL to the same daemon. For the app, set NEXT_PUBLIC_API_URL at build time. Allow Playwright to build/start the candidate when BASE_URL is absent, or supply a deployed app whose serving deployment and static build/chunk hashes match the manifest. Record its actual served identity; a checkout's HEAD alone is insufficient.
5. Extend the existing global setup to run read-only identity preflight before clerkSetup, fixture-user creation, tenant bootstrap, and cleanup. Compare observed app static/deployment identity, daemon /healthz, active runner process/heartbeat evidence, and CLI build file against the expected manifest. Read trusted local/deployment process records; do not add another person endpoint. Missing, unknown, stale, or mismatched observations fail closed. The later Playwright preflight project cannot replace this ordering.
6. Run make acceptance-e2e acceptance-cli with recorded origins/environment and capture manifest plus preflight/lane results. Run make _dry_website for the folded website lane. Deliberately mismatch each component and prove failure occurs before any fixture mutation. A local mock/provider stub supplies deterministic failures; paid provider verification needs its existing authorization.
7. Rehearse matched release smoke and an injected failed-release recovery. Record each component revision/hash, migration state, stopped traffic/claims, recovery actions, and final smoke. Traffic resumes only after the aligned build and recovery checks pass. Image rollback cannot restore removed schema/data.

Implementation must expose these observations through existing build/deployment records and fixture helpers. No manifest, startup, test result, or release evidence is claimed by this document update. The independent schedule fix still needs its own merge/test evidence before schedule acceptance.

### Release boundary

Coordinate daemon, runner, app, CLI, generated API, and public docs in one cutover. Stop traffic/claims, clear acceptance-test setup data through the chosen release procedure, run the new migration, release aligned binaries/clients, smoke-test, then restore traffic. No reset is performed now. Applied migrations remain historical; image rollback alone cannot recreate removed data or columns. Prepare a tested roll-forward/reset recovery procedure before release.

## Limits

Page filenames in the separate docs repository must be selected by its maintainer from actual files, not invented here. No publishing sign-off or live verification is claimed by this handoff.

## Related pages

- [Exact resource inputs/outputs](../../architecture/api_resources.md)
- [Creation and CLI outcomes](../../architecture/data_flow.md#fleet-creation-and-setup-recovery)
- [Owner decisions](decisions.md)
