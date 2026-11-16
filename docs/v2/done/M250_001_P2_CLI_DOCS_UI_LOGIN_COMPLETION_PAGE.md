# M250_001: Login completion and Orbit

**Prototype:** v0.28.2
**Milestone:** M250
**Workstream:** 001
**Date:** Nov 16, 2026
**Status:** DONE
**Priority:** P2, manual sign-in leaves the browser showing a used code.
**Categories:** CLI, DOCS, UI
**Batch:** B1, browser completion
**Depends on:** M230_003, existing local callback login
**Provenance:** agent-generated from the approved authentication preview
**Canonical architecture:** `docs/AUTH_DEVICE_LOGIN.md`

## Overview

**Goal (testable):** Both login modes show the approved completion page, and the pet command animates the approved Orbit jellyfish.

**Problem:** The manual page displays the code after the terminal uses it. The local callback page says “Terminal authorized”.

**Solution summary:** Apply the approved “Unleash your agents.” design to both modes. The manual browser observes the existing session read until it reports the consumed error code, cancellation, or expiry.

**Design flow:** Browser approval → manual code remains visible → terminal verifies → browser observes `410 / UZ-AUTH-012` → completion page. Default login retains its local callback and finalization boundary.

## PR Intent & comprehension handshake

- **PR title (eventual):** Polish terminal login completion and animate Orbit
- **Intent:** Confirm terminal authorization and give the terminal its approved jellyfish pet.
- **Authoring handshake:** Implement the signed-off login preview and Orbit pet. ASSUMPTIONS I'M MAKING: keep the five-second preview and use a compact jellyfish during requests; no authentication endpoints change.
- **Implementer handshake:** Matches the approved intent; code stays in this task's checkout and remains available for review.

## Implementing agent — read these first

1. `docs/AUTH.md` — credential lifecycle.
2. `docs/AUTH_DEVICE_LOGIN.md` — both browser transports.
3. `rustd/crates/daemon/afd_tenant/src/session/mod.rs` — consumed, aborted and expired reads.
4. `cli/src/commands/auth/login-callback-page.ts` — self-contained callback page.
5. `ui/packages/app/app/cli-auth/[session_id]/page.tsx` — manual browser ownership.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| This spec | CREATE | Record approval, scope and proof |
| `cli/src/commands/auth/login-callback-page.ts` | EDIT | Approved success design |
| `cli/test/login-callback-page.unit.test.ts` | EDIT | Check success content and unchanged response policy |
| `cli/test/login-callback.unit.test.ts` | EDIT | Updated success heading |
| `cli/test/login-browser-flow.unit.test.ts` | EDIT | Updated success heading |
| `cli/test/acceptance/fixtures/browser.ts` | EDIT | Updated success locator |
| `ui/packages/app/app/cli-auth/[session_id]/page.tsx` | EDIT | Observe manual completion with cancellation ownership |
| `ui/packages/app/app/cli-auth/[session_id]/cli-auth-ui.tsx` | EDIT | Approved code and completion views |
| `ui/packages/design-system/src/tokens.css` | EDIT | Name the approved authorization-card width |
| `ui/packages/design-system/src/theme.css` | EDIT | Expose the authorization-card width utility |
| `ui/packages/app/lib/auth/manual-approval-completion.ts` | CREATE | Bound observation of existing session status |
| `ui/packages/app/tests/cli-auth-page.test.ts` | EDIT | Approved code-step wording |
| `ui/packages/app/tests/cli-auth-page.fixtures.ts` | CREATE | Keep unchanged test fixtures outside the case file’s 350-line limit |
| `ui/packages/app/tests/cli-auth-completion.test.tsx` | CREATE | Completion, refusal, expiry and cancellation regressions |
| `ui/packages/app/tests/manual-approval-completion.test.ts` | CREATE | Observer boundaries, cancellation races, stream cleanup and read cap |
| `ui/packages/app/tests/e2e/acceptance/cli-loopback-login.spec.ts` | EDIT | Updated success locator |
| `docs/AUTH_DEVICE_LOGIN.md` | EDIT | Browser completion behavior |
| `cli/src/constants/pet.ts` | EDIT | Approved pet identity and animation dimensions |
| `cli/src/output/orbit.ts` | CREATE | Approved jellyfish frames |
| `cli/src/output/palette.ts` | EDIT | Pet colors and bounded terminal redraw helpers |
| `cli/src/output/progress.ts` | EDIT | Compact request animation and full pet preview |
| `cli/src/output/capability.ts` | EDIT | Terminal row count for preview sizing |
| `cli/src/services/output.ts` | EDIT | Route preview display through the output owner |
| `cli/src/commands/pet/preview.ts` | EDIT | Play and cancel the approved preview |
| `cli/src/program/tree/diagnostics.command.ts` | EDIT | Updated pet help |
| `cli/test/pet-preview.unit.test.ts` | EDIT | Pet identity, preview and cancellation |
| `cli/test/output-progress.unit.test.ts` | EDIT | Jellyfish progress and resource cleanup |
| `cli/test/orbit.unit.test.ts` | CREATE | Frames, colors and terminal controls |
| `cli/test/acceptance/help-and-errors.spec.ts` | EDIT | Updated pet JSON identity |
| `cli/README.md` | EDIT | Explain Orbit, preview behavior and motion preference |

## Applicable Rules

- `code.md`: GRD, CODE-SEAM, TGU, OWN, FLL.
- `languages/typescript.md`: TS-SCHEMA, TS-TIMEOUT, TS-RETRY, TS-BOUNDED, TS-PRIMITIVE, TS-TOKEN.
- `domains/auth.md`: AUTH-DOC, AUTH-SECRET, AUTH-NOLOG.
- `tests.md`: TEST-REDGREEN, TEST-ERROR, TEST-NEG, TEST-LANE, TEST-CLAIM.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `orly gate work` | Yes | Run on the review diff |
| `orly gate verify` | Yes | Run the declared checks; record actual results |
| File and function length | Yes | Separate the bounded observer from the page |

## Prior-Art / Reference Implementations

- `cli/src/commands/auth/login-callback.ts` owns callback finalization and stays authoritative for default login.
- `ui/packages/app/lib/api/draft-request.ts` bounds response reads and aborts them with the caller.
- `/workspace/outputs/agentsfleet-cli-review/authentication-success.html` holds the human-approved appearance and copy.
- `/workspace/outputs/agentsfleet-cli-review/orbit-pet.html` holds the approved Orbit poses and colors.

## Sections (implementation slices)

### §1 — Observe manual verification

The existing session endpoint remains the authority. Only its consumed response completes the browser view. Cancellation, malformed responses and transient failures never become success. Requests, response bodies and waits belong to the mounted page and stop on unmount or expiry.

- **Dimension 1.1** — DONE — Observe consumed code → Test `shows completion only after the terminal consumes the verification code` (unit): pending reads retain the code; consumed response displays completion.
- **Dimension 1.2** — DONE — Reject unrelated terminal states → Test `does not report a refused session as successful` (unit): aborted and expired reads display failure.
- **Dimension 1.3** — DONE — Reject malformed completion proofs → Test `does not trust a consumed code on the wrong response` (unit): wrong statuses and malformed bodies never display completion.
- **Dimension 1.4** — DONE — Survive a transient read failure → Test `keeps the code after a network failure and observes a later completion` (unit): the code remains available until a valid consumed read.
- **Dimension 1.5** — DONE — Bound ownership → Test `aborts an outstanding completion read on unmount` (unit): the request signal aborts and no subsequent request starts.
- **Dimension 1.6** — DONE — Bound expiry → Test `stops observing when the login session expires` (unit): renewed expiry replaces the code with a restart instruction.
- **Dimension 1.7** — DONE — Preserve renewed verification window → Test `keeps a renewed code available after the original session deadline` (unit): approval at four minutes keeps the code visible beyond the original deadline and still observes terminal consumption.

### §2 — Render the approved design

Both modes use the approved headline, explanation and close-tab footer. The callback remains self-contained and loads no resources. The manual code step uses the same brand and card layout.

- **Dimension 2.1** — DONE — Render success copy → Test `renders the approved completion message` (unit): callback content contains the headline and return-to-terminal instruction.
- **Dimension 2.2** — DONE — Preserve policy → Test `keeps the strict content policy on every outcome` (unit): callback headers retain every existing directive.

### §3 — Check the user path

Run the changed package lanes, render both outcomes with the real components and styles, and document the new browser behavior.

- **Dimension 3.1** — DONE — Render completion (manual): inspect light and dark appearances at desktop and mobile widths and verify no horizontal clipping.
- **Dimension 3.2** — DONE — Verify the changed callback scenario: `login-browser-flow.unit.test.ts` exercises the real callback, crypto, credential persistence and identity path; Chromium recognizes the exact approved heading in the actual callback response. The live identity-provider acceptance suite was not run.

### §4 — Animate Orbit in the terminal

Render the approved half-block jellyfish with colors owned by the palette. Keep request progress on one line. Reserve and clear the full preview inside its effect scope, and cancel the preview promptly on interrupt.

- **Dimension 4.1** — DONE — Approved pet identity → Test `pet needs no login and preserves machine output` (unit): every machine-output path returns only the Orbit name, without requests.
- **Dimension 4.2** — DONE — Preview and cleanup → Test `pet animates Orbit locally and clears its rows before the final sentence` (unit): colored jellyfish frames clear before the result.
- **Dimension 4.3** — DONE — Resource ownership → Test `releases the animation on request cancellation` (unit): cancellation clears progress and stops writes.
- **Dimension 4.4** — DONE — Safe terminal fallback → Test `uses compact Orbit when the preview does not fit` (unit): short and narrow terminals never reserve extra rows.
- **Dimension 4.5** — DONE — Motion preference → Test `shows a static status when animation is disabled` (unit): reduced-motion and automated callers receive one static line.

## Interfaces

No endpoint, credential, token, flag or wire shape changes. The browser reads the existing session error body, whose `error_code` is declared by the Rust registry. The new observer delivers a local completed-or-failed result to its caller.

The pet name in the existing JSON payload becomes `Orbit`. The human preview remains five seconds and shows idle, working and celebration poses. Requests use a compact jellyfish. Small terminals and terminals without color use the compact form; reduced-motion settings show a static status. Only the explicit preview reserves multiple terminal rows.

## Failure Modes

| Mode | Kind | Cause | Handling | Test |
|------|------|-------|----------|------|
| Cancelled session | failure | Terminal cancels | Restart instruction | `does not report a refused session as successful` |
| Invalid proof | security | Wrong status, missing code or oversized body | Never display success | `does not trust a consumed code on the wrong response` |
| Network failure | failure | Status read fails | Keep code and retry within the deadline | `keeps the code after a network failure and observes a later completion` |
| Unmount | failure | Navigation | Abort read and stop waits | `aborts an outstanding completion read on unmount` |
| Expiry | failure | Deadline passes | Stop reads and show restart instruction | `stops observing when the login session expires` |

## Invariants

1. Only `410 / UZ-AUTH-012` completes manual approval, enforced by status and schema checks.
2. A cancelled page cannot publish a result, enforced by its abort signal.
3. The callback success page loads no resources, enforced by response policy and existing tests.

## Metrics & Observability

N/A: no product or operator signal changes. Codes, session identifiers and credentials are never logged.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|-----------|--------|----------|----------|-----------------|
| R1 | Manual completion and failures | `env -u AGENTSFLEET_UI_ENV_FILE -u NEXT_PUBLIC_POSTHOG_ENABLED make test-unit-app` | exit 0 | P0 | ✅ CI: 491 files, 4,686 tests passed; statements, branches, functions and lines all 100% (4,656 baseline, +30) |
| R2 | Pet preview, callback completion and policy | `make test-unit-cli` | exit 0 | P0 | ✅ CI: 2,359 passed, 17 skipped, 0 failed; line and function coverage 100% (2,335 baseline, +24) |
| R3 | Changed package lint and version | `env -u AGENTSFLEET_UI_ENV_FILE make lint-app lint-cli lint-design-system check-version` | exit 0 | P0 | ✅ app, CLI and design-system lint passed; all versions match 0.77.0 |
| R4 | Rendered app paths | `make dry-app` | exit 0 | P1 | ✅ CI dry-app passed; twelve actual-component Chromium renders also passed locally |
| R5 | Public login bundle | `cd ui/packages/app && bun run build && bun run size` | exit 0, at most 240 KiB | P1 | ✅ local build and all budgets passed; login route 243.32 kB gzip versus 245.76 kB limit |
| S1 | Work gate | `orly gate work` | exit 0 | P0 | ✅ repo.config passed |
| S2 | Verification gate | `orly gate verify` | exit 0 | P0 | ✅ passed with task-owned CMake, libclang and the system C include directory |

## Dead Code Sweep

N/A: no files deleted. Update every active test locator for the retired success heading.

## Out of Scope

- Merging and releases are not requested. The owner requested publishing both companion PRs.
- Pet selection settings, new animation dependencies and image-specific terminal protocols are not planned.

## Product Clarity (authoring record)

1. **Successful user moment** — the browser confirms authorization and says to return to the terminal.
2. **Preserved user behaviour** — compare the default code or enter the manual code as before.
3. **Optimal-way check** — observe the existing session endpoint, without a second approval action.
4. **Rebuild-vs-iterate** — update the two existing presentations.
5. **What we build** — a bounded observer and the approved completion design.
6. **What we do NOT build** — authentication endpoints, persistent browser state or pet selection.
7. **Fit with existing features** — retain callback finalization and encrypted manual verification.
8. **Surface order** — both browser surfaces together.
9. **Dashboard restraint** — retain the code until verification is confirmed.
10. **Confused-user next step** — follow the browser's return-to-terminal or restart instruction.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** separate observation, presentation and validation.
- **Alternatives considered:** a backend endpoint is unnecessary because consumed reads already have a distinct error code; completing on approval would hide the code prematurely.
- **Patch-vs-refactor verdict:** a patch using the existing session authority.

## Discovery (consult log)

- **Human decision:** “The login changes are signed off”. Approved headline: “Unleash your agents.”
- **Human pet decision:** “yes go with orbit”. The existing task includes the approved jellyfish and its idle, working and celebration poses.
- **Agent defaults:** two-second bounded reads with jitter; transient failures retain the code; mounted ownership cancels every request and wait.
- **Source revision:** implementation began at `4f3159269644b2a1d814fbcc9119f01830b7eaf8`; the publishing branch starts from fetched `origin/main` at `11e9944e3aad41f3d46ef4678e1953c06d7ef3f0`. Only this task’s changes were carried forward.
- **Publication request:** “send the PR in both repos”. The source and companion docs PRs will be cross-linked.
- **Regression proof:** the new completion test failed against the original page because the completion heading never appeared; 13 focused completion cases then passed.
- **Browser proof:** actual React views and the actual callback response rendered in Chromium at 320px and 768px, in light and dark mode. Both completion views fit without horizontal overflow; callback response policy remained intact.
- **Orbit proof:** the built client rendered all poses in an 80-column, 24-row pseudo-terminal. Interrupt returned exit 130 and cleared the drawing. A 40-column terminal with `NO_COLOR=1` emitted only compact plain text; reduced motion emitted one static jellyfish. The full CLI lane passed 2,243 tests with 17 skipped and no failures. The pet, sprite, palette and progress modules each reported 100% line and function coverage.
- **Companion docs:** the separate `chore/m250-login-completion-changelog` branch updates the command reference, motion setting and changelog. `make test` passed 25 documentation tests and the documentation checker.
- **Environment correction:** the first app run inherited a nonexistent environment-file path and disabled analytics. Removing those two test-process overrides produced 4,284 passing tests.
- **Publishing checks:** CMake and libclang were installed into a task-owned prefix. Passing the system C include directory through `BINDGEN_EXTRA_CLANG_ARGS` resolved binding generation. `orly gate verify` passed every declared lint and version criterion on the current-base candidate. Updating the docs base resolved the OpenAPI drift; its PR gate passed on `a39c96f`. The full dry lane passed in CI; live identity-provider acceptance was not run.
- **Review fixes:** the new page uses the current Phosphor icon package. Manual completion confirms authorization rather than claiming credential persistence. A terminal resize stops full-preview redraws and relative cursor cleanup. Regression tests failed for the previous wording and resize handling before these repairs. Standards review prompted declaration ordering, unchanged fixture extraction, and coverage of resize before the next redraw.
- **Current visual proof:** twelve actual-component Chromium renders passed at 320px and 768px in light and dark appearances, including the pending code step. The current manual authorization text fits; callback policy is unchanged. `final-login-render.log` records each output. Fresh pseudo-terminal probes observed all three Orbit poses, exit 0, owned-row cleanup, and no cursor-relative writes after resizing to 40 columns and 10 rows. `final-pet-terminal.log` records both passes.
- **Reviews:** the native adversarial review found no remaining defects after the three repairs; fixtures were reviewed in summary mode. The standards reviewer found no remaining issues against 244 screened rules. The docs standards review found none against 43 rules. Optional outside-model review was unavailable because that CLI is absent.
- **PR validation follow-up:** the first exact-head PR gate stopped at local integration setup: QStash could not trust the cloud certificate chain. A task-owned Compose override adds that certificate bundle and removes the inherited HTTP proxy for NATS loopback health probes. Repository infrastructure and gates are unchanged.
- **Bundle repair:** the first Actions app size check measured the public login route at 273.18 kB gzip, above its existing 245.76 kB limit. The observer now reuses the app’s existing Valibot schema validator. The same local build and size commands passed at 243.32 kB and 38 focused authentication tests passed; no bundle limit changed.
- **Coverage follow-up:** all application tests passed in Actions, but the existing 100% threshold identified uncovered observer branches. Seventeen focused cases now cover cancellation during waits and response reads, failed streams, invalid expiry and schema, redirects, transient statuses and clock skew. Combined with the thirteen rendered-page cases, thirty focused tests passed; observer coverage reached statements 83/83, branches 39/39, functions 9/9 and lines 57/57.
- **Gate restart:** the `f8713cc5e` gate was interrupted after the coverage follow-up was ready, so final verification can run against its new pushed commit. No completed full-repository result is claimed for that interrupted run.
- **Implementation PR:** https://github.com/agentsfleet/agentsfleet/pull/847 (ready for review).
- **Companion PR:** https://github.com/agentsfleet/docs/pull/250.
- **Closure:** every implementation Dimension is complete and the approved login and Orbit scope is closed. Validation of the callback copy uses the existing local callback/crypto scenario and actual Chromium heading checks; the live identity-provider suite is not claimed.
- **Remaining repository validation:** the final local PR gate on `26bda403c` reached the Rust integration suite and failed `integration_wall_ticks::a_second_lag_inside_a_tick_re_announces_when_the_tick_ends` at unchanged `wall_stream_fixture.rs:101` (`Elapsed(())`); that binary reported 225 passed and one failed. No Rust files or timeout thresholds changed, and this failure is recorded as a PR validation limit rather than unfinished login or pet implementation.
- **Feature check evidence:** app coverage https://github.com/agentsfleet/agentsfleet/actions/runs/43893709005/job/133391744847; CLI https://github.com/agentsfleet/agentsfleet/actions/runs/43893709005/job/133391745015; dry app https://github.com/agentsfleet/agentsfleet/actions/runs/43893709046/job/133391427174.
- **Deferrals and transfers:** none.
- **Renewed-expiry review repair:** review identified that approval renews the server deadline while the observer used the pre-approval deadline. A rendered-page regression failed on `75d3e1b3c` after approval at four minutes and a status read beyond the original deadline. The observer now validates renewed status expiry, including a finite parsed timestamp, while preserving its own five-minute and 150-read bounds. Native-runtime tests reproduce the malformed-offset failures hidden by DOM emulation. The same regressions and the observer's malformed-status, expiry, cancellation and bound checks pass (`bun run test tests/cli-auth-completion.test.tsx tests/manual-approval-completion.test.ts`: 37 passed).
