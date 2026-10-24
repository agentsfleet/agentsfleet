# M230_003: Consistent page loading and terminal sign-in

**Prototype:** v2.0.0
**Milestone:** M230
**Workstream:** 003
**Date:** Oct 24, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — navigation and human sign-in
**Categories:** API | CLI | UI
**Batch:** B1 — existing approved stream
**Branch:** fix/approved-fleet-chat
**Baseline revision:** 74a39c60ef260973894a68f29d0b6e4f6f1c6eda
**Test Baseline:** pending — measure unit and integration before updating the Pull Request (PR)
**Baseline evidence:** pending — revision and command output before PR update
**Depends on:** none
**Provenance:** LLM-drafted (Codex, Oct 24, 2026)
**Canonical architecture:** `docs/architecture/api_resources.md` — authentication

## Overview

**Goal (testable):** Every page uses the same loader; browser approval connects its initiating terminal automatically and confirms only acknowledged completion.
**Problem:** Loading pages show inconsistent skeletons, and sign-in needs a code copied back into the terminal.
**Solution summary:** Shared centered spinner inside the persistent shell; key-derived visual comparison code and separate random private verifier; authenticated completion after credential persistence and identity validation.

## PR Intent & comprehension handshake

- **PR title (eventual):** Unify page loading and simplify terminal sign-in
- **Intent (one sentence):** Preserve recognizable navigation and finish terminal sign-in with one informed browser approval.
- **Handshake:** Indy approved both in PR #784 and requested eyeballing afterward. Assumptions: same branch/tree, shell persists, no artificial delays, comparison code is public, private verifier stays in terminal/server request bodies, human acceptance remains pending.

## Implementing agent — read these first

1. `ui/packages/app/components/layout/ShellFrame.tsx` — persistent navigation.
2. `docs/DESIGN_SYSTEM.md` — tokens, motion and components.
3. `docs/AUTH_DEVICE_LOGIN.md` — encryption, expiry and atomic replay.
4. `cli/src/commands/login.ts` — credential persistence and identity rollback.
5. `rustd/crates/afd_dragonfly/src/session/verify_consume.lua` — single redemption.

## Files Changed (blast radius)

This approved stream covers the following source families; expand this inventory with exact edited paths before review. No unrelated files may change.

| File | Action | Why |
|---|---|---|
| `ui/packages/app/components/layout/RouteLoading.tsx` | EDIT | Shared centered loading indicator. |
| `ui/packages/app/components/layout/ShellFrame.tsx` | EDIT | Same indicator in empty shell slot. |
| `ui/packages/app/app/**/loading.tsx` | CREATE / EDIT | All route loading boundaries, including first load. |
| `ui/packages/app/app/cli-auth/[session_id]/*` | CREATE / EDIT | Compare, approve, cancel, account and completion. |
| `ui/packages/app/lib/auth/cli-flow.ts` | EDIT | Key-derived public comparison code. |
| `ui/packages/app/tests/*loading*` | EDIT | Common indicator render proofs. |
| `ui/packages/app/tests/*cli-auth*` | EDIT | Browser approval and recovery proofs. |
| `ui/packages/app/tests/fleets-routes/list-and-loading.test.ts` | EDIT | Updated loading behavior. |
| `ui/packages/app/tests/app-shell-frame.test.ts` | EDIT | Shared shell fallback. |
| `cli/src/commands/login*` | CREATE / EDIT | Private verifier, bounded wait and completion. |
| `cli/src/lib/cli-flow.ts` | EDIT | Private random verifier and comparison code. |
| `cli/test/*login*` | CREATE / EDIT | Automatic sign-in and failure proofs. |
| `rustd/crates/afd_api_wire/src/auth.rs` | EDIT | Required verifier and completion wire types. |
| `rustd/crates/afd_api_tenant/src/handler/auth/**` | CREATE / EDIT | Parse verifier and authenticated completion. |
| `rustd/crates/afd_api_tenant/src/lib.rs` | EDIT | Register completion endpoint. |
| `rustd/crates/afd_http/src/route/auth.rs` | EDIT | Completion route identity and bearer guard. |
| `rustd/crates/afd_http/src/services/device_flow.rs` | EDIT | Completion service seam. |
| `rustd/crates/afd_tenant/src/session/**` | CREATE / EDIT | Digest at creation and completion ownership. |
| `rustd/crates/afd_tenant/tests/*device_flow*` | EDIT | Real dependency regression. |
| `rustd/crates/afd_tenant/tests/support/device_flow_lane.rs` | EDIT | Updated session fixture. |
| `rustd/crates/afd_dragonfly/src/session/**` | CREATE / EDIT | Preserve digest and atomic completion. |
| `rustd/crates/afd_dragonfly/tests/integration_cluster_semantics/sessions.rs` | EDIT | Completion and race proofs. |
| `docs/AUTH.md` | EDIT | Current login flow. |
| `docs/AUTH_DEVICE_LOGIN.md` | EDIT | Current trust boundaries. |
| `docs/DESIGN_SYSTEM.md` | EDIT | Shared page loader. |
| `docs/architecture/api_resources.md` | EDIT | Completion resource. |
| `docs/v2/active/M230_003_P1_API_CLI_UI_LOADING_TERMINAL_LOGIN.md` | EDIT | Scope and evidence. |

## Applicable Rules

- `.orly/docs/greptile-learnings/RULES.md` — NDC, NLR, NLG, UFS, FLL, CTM, ECL: remove replaced prompts, centralize constants, constant-time secret comparisons, distinguish waiting and failures.
- `.orly/dispatch/write_ts_adhere_bun.md` — shared controls, tokens and effect cleanup.
- `.orly/dispatch/write_auth.md`, `docs/AUTH.md`, `docs/RUST_ERROR_STANDARD.md` — human consent, typed boundaries and preserved causes.
- `.orly/docs/REST_API_DESIGN_GUIDELINES.md` — ownership and complete route registration.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| UI / Design Token | yes | Existing Spinner, Button and Card primitives. |
| File & Function Length (≤350/≤50/≤70) | yes | Extract polling, browser hook and completion by concern. |
| Auth / Error / Logging | yes | Validated private verifier, owner checks, secret-free analytics. |
| Spec / UFS / Orphan | yes | Test mappings; delete replaced interactive prompt. |

## Prior-Art / Reference Implementations

- **Reference:** Existing session scripts provide atomic transitions on a single cluster key. Existing CLI output services, typed errors, replacement prompt, rollback and hydration remain authoritative.
- **Reference:** ShellFrame and the design-system Spinner provide persistent navigation and reduced-motion rendering.

## Sections (implementation slices)

### §1 — Shared page loader

Use the same centered accessible indicator in every route boundary and empty shell fallback. Before the shell resolves it fills the viewport; afterward it fills main content. Component-sized refresh indicators stay local.

- **Dimension 1.1** — Common indicator on all page boundaries → Test `shared_route_loading`
- **Dimension 1.2** — Slow navigation preserves shell and first load fills viewport → Test `loading_browser_walk`

### §2 — Compare and approve

Terminal creates an ephemeral encryption key and independent 256-bit private verifier. Server stores its session-bound peppered digest at creation. Browser derives a six-digit visual code from the public key and requires deliberate human comparison and approval. Approval cannot overwrite the verifier. Required creation fields reject outdated clients immediately. CLI polls with bounded cancellation; after decrypting it persists and validates identity before acknowledging completion. An authenticated owner sees connected only after that acknowledgement.

- **Dimension 2.1** — Private redemption, expiry, attempt bound and replay remain atomic → Test `private_redemption`
- **Dimension 2.2** — Automatic login validates and saves before acknowledging; failed save/identity never acknowledges → Test `automatic_login`
- **Dimension 2.3** — Browser comparison, account/environment, cancellation and recoverable errors → Test `terminal_login_ui`
- **Dimension 2.4** — Indy eyeballs both experiences → Test `indy_acceptance`

## Interfaces

- Create requires `public_key`, `token_name`, `verification_code`: a private 43-character base64url verifier. Missing or invalid fields fail before storage. Update server and clients together; no parallel compatibility path.
- Approve carries encrypted payload and dashboard public key; never private verifier.
- Verify presents private `verification_code`; existing single-use and same-fingerprint replay remain atomic.
- `GET /v1/auth/sessions/{session_id}/completion`: authenticated approving human reads `{completed: boolean}`.
- `POST /v1/auth/sessions/{session_id}/completion`: approving human plus private verifier acknowledges a consumed session. Idempotent, no expiry extension.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|---|---|---|
| Invalid / outdated client | Missing verifier, wrong shape | 400 before storage; update and restart guidance. |
| Pending / competing approval | No approval or concurrent tabs | Wait while pending; first approval wins; no payload overwrite. |
| Wrong secret / replay | Guess or different fingerprint | Existing bounded attempts and replay; no credential released. |
| Timeout / cancel | Expiry or interrupt | Stop waiting, no new local credential; restart instructions. |
| Network / unavailable dependency | Dropped response or server failure | Bounded retry; uncertain browser approval recovers via status, never false success. |
| Persistence / identity failure | Cannot save or credential refused | Existing failure and rollback, no acknowledgement. |
| Completion attack | Other owner, wrong verifier or premature acknowledgement | Refuse without setting completed. |

## Invariants

1. Public comparison code cannot redeem — private verifier digest is fixed at creation and checked in atomic consume.
2. Only browser-session identity approves — DashboardIdentity extractor.
3. Completion requires consumed state, matching owner and private verifier in one script; CLI calls after saving and successful identity read.
4. Private verifier never enters URL, browser or analytics; changed login events omit session identifiers.
5. Existing workspace auth layouts guard resolved content; common loading never skips those checks.

## Metrics & Observability

Keep existing terminal completion event after identity validation; omit session identifiers. Distinguish browser approval from saved terminal completion in authentication docs and event semantics. Navigation already records views; add no loading-view event.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|---|---|---|---|
| 1.1 | unit | `shared_route_loading` | Every route renders one accessible common status and no page-specific skeleton. |
| 1.2 | e2e | `loading_browser_walk` | Slow first load and navigation show the common loader with expected shell. |
| 2.1 | integration | `private_redemption` | Missing/wrong secret, no approval, wrong owner, expiry, competing consume and replay cannot release another credential or acknowledge prematurely. |
| 2.2 | unit | `automatic_login` | Pending retries; cancellation, timeout, network, persistence and identity failures do not acknowledge; success orders save then identity then acknowledgement. |
| 2.3 | e2e | `terminal_login_ui` | Compare/approve shows waiting; only authenticated completion shows connected; uncertain approval recovers. |
| 2.4 | manual | `indy_acceptance` | Indy navigates preview and signs in from terminal; preserve his verdict in PR notes. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Both experiences work | Local browser and terminal acceptance walk | Navigation and login evidence, Indy verdict recorded separately | P0 | |
| S1 | Conform | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint | `make lint-all` | exit 0 | P0 | |
| S3 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S4 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version | `make check-version` | exit 0 | P0 | |
| S6 | Secrets | `gitleaks git --staged --redact` | no leaks | P0 | |

## Dead Code Sweep

Remove browser-code prompt and route-specific skeleton bodies. Search `promptVerificationCode`, `verifyAndDecryptWithRetry`, `generateVerificationCode` and remove unused helpers and imports. No compatibility aliases.

## Out of Scope

- Deployment, merging PR #784, and repeated paid fleet acceptance.
- Component-sized loading for buttons or background refresh.

## Product Clarity (authoring record)

1. **Successful user moment** — Recognizable navigation; terminal connected after comparison and one browser approval.
2. **Preserved user behaviour** — Manual URL opening, credential replacement confirmation, identity rollback and workspace hydration.
3. **Optimal-way check** — Remove code copying while preserving deliberate human approval.
4. **Rebuild-vs-iterate** — Iterate on encrypted session relay and shell boundaries.
5. **What we build** — Shared loader, private redemption, bounded waiting, confirmed completion and recovery.
6. **What we do NOT build** — New account system or fleet runtime behavior.
7. **Fit with existing features** — Preserve workspace access and durable credentials.
8. **Surface order** — Shared loader and server safety before client integration; preview together.
9. **Dashboard restraint** — Approval says waiting; connected requires acknowledgement.
10. **Confused-user next step** — Compare codes, change account before approval, or restart `agentsfleet login` after cancellation/expiry.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Shared loader and focused login handshake update in existing PR, as requested.
- **Alternatives considered:** Approval as success cannot prove persistence; public comparison code as redemption secret allows third-party consumption.
- **Patch-vs-refactor verdict:** A refactor of login orchestration retaining encryption and atomic storage primitives.

## Discovery (consult log)

- **Consults** — Indy approved: “ys go on both signed off approved, fix that in this PR. let indy eyeball both experience when done”. Same branch/tree; visual acceptance remains pending. App-wide common loader supersedes fleet-only skeleton proposal.
- **Metrics review** — Remove session identifiers from changed login events; document approval versus completion.
- **Skill-chain outcomes** — Spec authored; unit, integration, review and post-push follow-up pending.
- **Deferrals** — None.
