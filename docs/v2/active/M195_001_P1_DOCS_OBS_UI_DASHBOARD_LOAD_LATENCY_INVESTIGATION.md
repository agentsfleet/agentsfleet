<!-- SPEC AUTHORING RULES — read docs/TEMPLATE.md before editing this file. -->

# M195_001: Where the dashboard's seconds go

**Prototype:** v2.0.0
**Milestone:** M195
**Workstream:** 001
**Date:** Sep 10, 2026
**Status:** IN_PROGRESS — PARKED after the measurement landed; §2 and §5.3 remain open
**Priority:** P1 — nothing here changes behaviour; it produces the measurements a behaviour change would have to be argued from. The fixes M194_001 is already making are the P0s.
**Categories:** DOCS, OBS, UI
**Batch:** B1 — one workstream; the measurement lane and the page it writes into are one change.
**Branch:** `feat/m195-dashboard-load-measurement`
**Baseline revision:** `a4e0ef2bda8a62742a400be18f58588056786f8c`
**Test Baseline:** unit=2713 integration=445 (at `a4e0ef2b`; HEAD unit=2733, +20 tests / +3 files). The diff touches no `.rs` file, so the integration lane is unchanged by it.
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M195_001-a4e0ef2bd.md`
**Depends on:** M194_001 for the build the measurement is taken on; its Pull Request also carries the CTA and action-button fixes this spec must not overlap. Runs in PARALLEL with M195_002 — the two declare no file in common — with one exception: **§2's request inventory is taken after M195_002 lands, or re-taken**, because that spec removes the wall's separate counter fetch by carrying the figures on the frame. §1, §3, §4 and §5 are independent of it.
**Provenance:** agent-generated from an operator's numbered findings on the development dashboard (findings 8, 9, 12, 13, 14), each cited fact read at source. One premise the findings carried did not survive the read — see Discovery.
**Canonical architecture:** `docs/architecture/web_app.md` §The two shapes

---

## Overview

**Goal (testable):** every wait an operator named on the dashboard is measured, attributed to a named stage, and written down where the next reader finds it — with the causes the measurement does not settle recorded as unresolved rather than guessed.

**Problem:** an operator walked the dashboard and asked five questions nobody in the repository can answer from a file: why a chat takes a while to load and how its concurrency works; what the Fleets wall actually requests, once or forever; whether ACTIVE is the same colour on every surface; why Secrets spins for about two seconds; and why Runners reads as if it never returns. Each is currently answered by reading source and guessing. There is no measured number for any dashboard surface anywhere in the repository, and the one place a wait was measured (`docs/v2/done/M194_001_P0_API_INFRA_UI_INTEGRATION_GRANT_REQUEST_PATH.md` §5) found a 4.6-second page that source-reading had not predicted.

**Solution summary:** one measurement lane and one page. The lane is a Playwright acceptance spec that navigates each named surface repeatedly, reads the server-side request audit the repository already has (`ui/packages/app/lib/acceptance/workspace-fetch-audit.ts`), times each stage, and attaches the table to its run. The page is `docs/architecture/dashboard_load.md`: the ASCII sequence diagram of a chat load, the concurrency model beside it, the wall's request inventory, the status matrix, and the measured tables with the run that produced them. Three of the claims the page makes are pinned by tests, so the drawing cannot drift from the code. No dashboard behaviour changes.

## PR Intent & comprehension handshake

- **PR title (eventual):** `docs(m195): measure and draw the dashboard's load path`
- **Intent (one sentence):** turn five questions about a slow dashboard into measured numbers and one readable page, so the fix that follows is chosen from evidence instead of from a reading of the source.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/tests/e2e/acceptance/chat-single-fetch.spec.ts` — the shape a request-profile measurement already takes here: reset the audit, navigate, read the snapshot, assert per-template counts. Mirror it; do not invent a harness.
2. `ui/packages/app/lib/acceptance/workspace-fetch-audit.ts` — the server-side counter, keyed by id-free route template and gated by `AGENTSFLEET_E2E_AUDIT`. The inventory is counted here, not inferred.
3. `ui/packages/app/lib/streaming/workspace-stream.ts` — one EventSource per workspace, demultiplexed per fleet. Its header states what the wall used to do and what it does now; the inventory must agree with the code, not with the finding's premise.
4. `rustd/crates/afd_db/src/error.rs` — `classify_acquire` separates a full pool from an absent datastore, because `PoolTimedOut` is the answer to both. Finding 14's hypothesis is settled here or not at all.
5. `docs/architecture/README.md` — the question→anchor index a new page has to join, and the voice the architecture set is written in.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/architecture/dashboard_load.md` | CREATE | The investigation's written artefact: the chat-load sequence diagram, the concurrency model, the wall's request inventory, the status matrix, and the measured stage tables naming the run behind each number. |
| `docs/architecture/README.md` | EDIT | Five rows in the question→anchor index, one per finding, so the answers are reached by question rather than by memory. |
| `ui/packages/app/tests/e2e/acceptance/dashboard-latency.spec.ts` | CREATE | The measurement: repeated navigations per surface, audited read counts asserted, per-stage timings attached as run artefacts. Lives in the acceptance directory, so it joins the journeys project with no config edit. |
| `ui/packages/app/lib/acceptance/workspace-fetch-audit.ts` · `ui/packages/app/tests/workspace-fetch-audit.test.ts` | EDIT | The audited path set gains the wall's, the Secrets page's and the Runners page's route templates, plus the completeness pin that keeps a later read from being invisible. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/latency.ts` | CREATE | Audit reset/read guards, stage timing and percentile summarisation, split out of the measurement spec before the length cap the way the acceptance directory already splits `fixtures/`. |
| `ui/packages/app/lib/api/client.ts` | EDIT | Per-request timing and attempt count for the acceptance audit, on the operator's explicit call. Counting stays at the ask; the added handle records what the ask COST, settling on the failure path too so an exhausted ladder is measured rather than lost. Inert unless the env gate is on. |
| `ui/packages/app/app/acceptance-audit/workspace-fetches/route.ts` | EDIT | Serves counts and timings in one body, so a measurement reads both consistently in one round trip. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/env-loader.ts` | EDIT | The publishable-key alias ran one way only, so a worktree whose sole env is the linked `.env.local` failed globalSetup while holding that exact value under the other name. Aliased both ways. |
| `ui/packages/app/playwright.acceptance.config.ts` | EDIT | The measurement resets an app-global counter, so it needs its own project ordered after `fetch-audit` rather than the `journeys` project this spec assumed. Not optional: sharing `journeys` would zero the counter under specs already counting. |
| `ui/packages/app/tests/status-badge-matrix.test.tsx` | CREATE | The status matrix made executable: what each of the three surfaces renders for each status, and the union-completeness assertion. |
| `ui/packages/app/tests/retry-ladder-bound.test.ts` | CREATE | The worst-case render wait computed from `RETRY_DEFAULTS` rather than typed into prose. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/view-data.test.ts` | EDIT | The overlap claim the diagram makes: the thread read is in flight before the fleet read is awaited. The file already existed — it proves the reads are ISSUED early; the added case proves one can also SETTLE while the fleet read is still pending. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (each route template is spelled once, in `AUDITED_PATH`), MSID (no milestone identifier in any source file or in the architecture page), TST-NAM (measurement test names carry no milestone), FLL (the measurement spec splits before the cap), NDC (no helper written for a caller that does not exist).
- `dispatch/write_ts_adhere_bun.md` — every new file here is TypeScript; the FILE SHAPE DECISION happens at PLAN.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the page is contributor-facing architecture prose.
- `dispatch/name_architecture.md` — a new architecture page and five index rows are an architecture write; the existing pages win where they already answer.
- `dispatch/verify.md` — the numbers this spec publishes are claims; each names the command and run that produced it.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SPEC TEMPLATE | yes — this file | Authored from `docs/TEMPLATE.md`; `audits/spec-template.sh --staged` clean before commit. |
| Architecture consult | yes — new page plus index rows | `docs/architecture/` read before writing; `check-architecture-doc` grades every cited path, link and anchor inside `make lint-all`. |
| MILESTONE-ID | yes — a docs page and four test files | No `M195` string in any of them. `check_architecture_doc.sh` additionally refuses a milestone identifier that resolves only to `pending/`, which this spec is. |
| UI GATE / DESIGN TOKEN | yes — `.tsx` test file | The matrix test renders existing components and asserts their classes; it introduces no markup and no arbitrary value. |
| LENGTH (≤350/≤50/≤70) | yes — the measurement spec | Per-surface helpers move to a fixtures module before the cap, the way the acceptance directory already splits `fixtures/`. |
| LOGGING | no | No production log line is added; the audit is test-only and already gated. |
| SCHEMA GUARD / ZIG / ERROR REGISTRY | no | No `*.sql`, no `*.zig`, no new fallible path. |

## Prior-Art / Reference Implementations

- **Reference:** `ui/packages/app/tests/e2e/acceptance/chat-single-fetch.spec.ts` — reset, navigate, read the audit snapshot, assert counts per route template. The new lane is this pattern applied to four surfaces.
- **Reference:** `ui/packages/app/tests/e2e/acceptance/dashboard-performance.spec.ts` — the `EventSource` audit shim and the `performance` reads the wall's connection claim is already asserted with; reuse the shim rather than a second one.
- **Reference:** `docs/architecture/data_flow.md` §D. WATCH — the existing ASCII-diagram voice for a live path. The new page matches it, so the set reads as one document.

## Sections (implementation slices)

### §1 — One chat load, drawn end to end

Finding 8 is a question, so the deliverable is an answer a person can read: an ASCII sequence diagram from the click to the first live frame, with the concurrency model beside it — which reads overlap, which cannot, and what the live tail rides. **Implementation default:** the page lives in `docs/architecture/`, because the lint gate there already grades cited paths, links and anchors, so a drawing that drifts from the code fails a command rather than aging quietly.

- **Dimension 1.1** — the page carries a sequence diagram whose hops are the ones the code performs, in order — credential resolve, the thread read started beside the fleet read, `getFleet` and billing awaited together, the thread awaited, server HTML, the dynamic thread chunk, one per-fleet `EventSource` on the same-origin proxy, first frame — each hop citing the file that performs it → Test `test_chat_load_stages_are_measured_and_attached` — **DONE**
- **Dimension 1.2** — the diagram's overlap claim is pinned where it is easiest to break: the thread read is in flight before the fleet read is awaited, so a slow fleet read never serialises the transcript behind it → Test `the thread read is in flight before the fleet read is awaited` — **DONE**

### §2 — Every request the wall makes, listed

Finding 9 asks whether the Fleets dashboard is a one-time load. The answer is a counted inventory, not a reading: one row per request, what issues it, and whether it repeats. The premise the finding arrived with — one connection per live fleet — is what the inventory tests rather than assumes.

- **Dimension 2.1** — the page carries the wall's request inventory: one row per request a load issues, with its route template, its issuer (server render, route handler, or `EventSource`), and whether it repeats; every count read from the server-side audit → Test `test_fleets_wall_request_inventory_is_measured` — **PARKED** (operator-acked; the wall's counter data path is being rewritten and any inventory taken now is stale on arrival)
- **Dimension 2.2** — the audited path set covers every read the wall's load issues, so a read added later is counted rather than invisible; an unaudited read on the load path fails the pin → Test `the audited path set covers every wall read` — **PARKED** (operator-acked; the wall's counter data path is being rewritten and any inventory taken now is stale on arrival)
- **Dimension 2.3** — a settled wall issues no repeating read: across an idle window the audited total does not move, and the only open connection is the single workspace stream → Test `test_the_settled_wall_issues_no_repeating_read` — **PARKED** (operator-acked; the wall's counter data path is being rewritten and any inventory taken now is stale on arrival)

### §3 — One status, one colour, or a named disagreement

Finding 12 asks whether ACTIVE is the same colour everywhere. Three surfaces render a fleet's status and none of them agree today. **Read at source during EXECUTE, and sharper than the finding assumed:** the wall tile colours a dot (`bg-info` installing, `bg-pulse` only when the stream is genuinely live, `bg-muted-foreground` otherwise) and uses eyebrow text only for catching-up and snapshot states; the chat strip tints dot and label together via `bg-current`, and only for `active`; and the detail header gives a status element to **exactly one** status — `installing` — rendering a lifecycle control (`KillSwitch` for active/paused/stopped, `FleetConfig` for killed) in that slot for the other four. So on the detail header the question has no answer for ACTIVE at all: it shows no status colour. `Badge` carries two variant families — tinted (`border-x/20 bg-x/10 text-x`) and solid (`bg-x text-on-pulse`) — and status rendering draws from neither consistently. This Section records that, exactly, and leaves the choice of one mapping to the spec that changes it.

- **Dimension 3.1** — the page carries a status matrix: for each status, what the wall tile, the detail header and the chat strip render, which `Badge` variant family it comes from, and which pairs disagree → Test `the status matrix is what each surface renders` — **DONE**
- **Dimension 3.2** — the matrix is complete by construction: every member of the status union has a row, so a sixth status cannot ship without one → Test `every fleet status has a matrix row` — **DONE**

### §4 — The Secrets wait, attributed

Finding 13 timed a spin at about two seconds. The page is a dynamic server component that awaits two upstream reads together and renders behind its own loading file; the per-render `cache()` around each read dedupes inside one render and never across visits. This Section measures which of the two reads the wait belongs to.

- **Dimension 4.1** — measured: one Secrets visit pays exactly two upstream reads, issued together, and pays them again on the next visit — the render cache does not span navigations → Test `test_secrets_visit_pays_two_upstream_reads` — **DONE**. **Corrected during EXECUTE:** "two" is the PAGE's cost. A navigation also pays the dashboard layout's workspace-switcher read, which is audited, so the audited total for one visit is 4. The Dimension's claim holds per template; the assertion was changed to count the pair rather than the total.
- **Dimension 4.2** — the page records a p50 and a p95 for the Secrets navigation and for each of its two reads over a fixed sample of visits, and names which read is the slower → Test `test_secrets_stage_timings_are_measured_and_attached` — **DONE**. **Resolved:** the operator authorised instrumenting the transport, so the per-read pair is measured and the slower read is named in an attachment beside the table.

### §5 — The Runners wait, attributed and bounded

Finding 14 reads as a page that never returns and asks whether Postgres connections are the cause. It does return: the route is a dynamic server component that awaits a scope check and a list read behind a loading file, and the client's policy allows three attempts with a per-attempt timeout and a ladder deadline, so the worst case is a long wait rather than a hang. **Whether the upstream slowness is pool acquisition is unproven and stays a hypothesis until measured** — `PoolTimedOut` is the same answer for a saturated pool and for a datastore that is not answering, and raising the pool size against the second one adds connections a server already cannot take.

- **Dimension 5.1** — measured: the Runners navigation returns, and its wait is attributed across the scope check, the credential resolve, each list attempt, and each retry sleep, with the observed attempt count recorded per navigation → Test `test_runners_navigation_returns_and_its_wait_is_attributed` — **DONE**. **Implemented** after the operator authorised transport instrumentation. One stage needed no instrumentation and is settled by reading: `hasScope` resolves from session claims (`lib/auth/platform.ts:32`), so the scope check costs no upstream call and is eliminated as the wait rather than left open.
- **Dimension 5.2** — the worst case is arithmetic rather than anecdote: computed from the retry defaults — the attempt ceiling, the per-attempt timeout, and the ladder deadline — so changing any one of them moves the assertion. **Corrected during EXECUTE:** the bound is the ladder deadline itself, not the deadline plus one attempt timeout. Two clamps make it a true total — `retry.ts` `#withinDeadline` refuses a sleep that would end past the deadline, and `client.ts` `attemptTimeoutMs` gives a late attempt only what remains of it — so no attempt outlives the deadline by its own ceiling → Test `the render wait is bounded by the declared retry ladder` — **DONE**
- **Dimension 5.3** — the page names which candidate cause the measurement implicates and which stay unresolved: pool acquisition, the upstream query, a cold instance, or the auth hop. Pool acquisition is claimed only where the datastore layer's own classification says pool rather than datastore; everything else is recorded as unresolved with the measurement that would settle it → Test `the Runners verdict is accepted` — **IN_PROGRESS** (the verdict table is written into the page's §The Runners wait; it needs the operator's sign-off, which is the whole content of this manual test)

## Interfaces

```
No endpoint, command, flag or behaviour change. The surfaces this touches:

  Artefact  docs/architecture/dashboard_load.md   diagram · concurrency model · request
                                                  inventory · status matrix · measured tables
  Index     docs/architecture/README.md           five question -> anchor rows, one per finding
  Audited   /v1/workspaces/{workspace_id}/fleets       added to AUDITED_PATH
            /v1/workspaces/{workspace_id}/onboarding
            /v1/workspaces/{workspace_id}/secrets
            /v1/tenants/me/billing
            /v1/tenants/me/provider
            /v1/fleets/runners
  Snapshot  /acceptance-audit/workspace-fetches   existing, test-only, env-gated
  Lane      make acceptance-e2e                   the measurement's only runner
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| A cold instance is published as a p50 | the first navigation pays compile and connection cost the rest do not | every published figure comes from a fixed sample of navigations, with the first sample reported separately; a single-sample number is never published |
| The audit counter carries another test's reads | the counter is module-global, keyed once per server process | each measurement resets before it navigates and fails on a non-zero pre-count rather than subtracting one |
| The audit is off against a deployed target | the env gate is unset, so every record call is a no-op and counts read zero | the reset call's answer is asserted before the first navigation; a disabled audit fails loudly instead of publishing zeros |
| A wait is blamed on Postgres connections | one timeout is the answer to both a full pool and an absent datastore | the page claims pool acquisition only where the datastore layer's classification says pool; otherwise the cause is recorded unresolved with the measurement that would settle it |
| A retry-constant change silently moves the worst case | the three constants and the number they imply live apart | the bound is computed from the exported defaults inside the pin, so any constant change moves the assertion |
| The live lane is unreachable | the acceptance lane needs a deployed target and its sign-in fixtures | the affected rows record the skip and its reason; the spec does not advance on inferred numbers |

## Invariants

1. No published figure comes from one sample — the measurement computes each percentile from a fixed sample count and fails when fewer samples were collected.
2. The request audit never runs in production — every record call is behind the env gate, pinned by the existing audit unit test.
3. Every status the union declares has a matrix row — the matrix test iterates the status union, so a sixth status fails until its row exists.
4. Every path, link and anchor the page cites resolves — enforced by `check-architecture-doc` inside `make lint-all`, not by review.
5. The retry bound is computed, never typed — the pin derives it from the exported defaults.
6. The page names no unshipped milestone identifier — the architecture gate accepts only identifiers resolving to `done/` or `active/` outside the roadmap page.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | never; the only counter added is test-only and env-gated | route template and count, no identifiers | no raw email, token or secret value is recorded or attached | `the audited path set covers every wall read` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | e2e | `test_chat_load_stages_are_measured_and_attached` | A fixed sample of chat loads of one seeded fleet: the thread-read count equals the sample count, the per-turn detail count is 0, at most one `EventSource` is live at a time, and the per-stage table is attached to the run |
| 1.2 | unit | `the thread read is in flight before the fleet read is awaited` | A fleet read held open resolves the thread read first → the thread promise settles while the fleet read is pending; a change that awaits the fleet read first fails it |
| 2.1 | e2e | `test_fleets_wall_request_inventory_is_measured` | One wall load with a seeded live fleet → the audited snapshot names exactly the templates the inventory table lists, with matching counts; an extra template fails the row |
| 2.2 | unit | `the audited path set covers every wall read` | Each route template the wall's load issues maps to an audited key; an unmapped path returns null and fails the assertion |
| 2.3 | e2e | `test_the_settled_wall_issues_no_repeating_read` | After the load settles, an idle window → the audited total is unchanged and the live connection count is 1 |
| 3.1 | unit | `the status matrix is what each surface renders` | Each status rendered through the three surfaces → the variant, family and element the matrix records; a changed variant fails |
| 3.2 | unit | `every fleet status has a matrix row` | The status union iterated → one matrix row each; a status with no row fails with its name |
| 4.1 | e2e | `test_secrets_visit_pays_two_upstream_reads` | One Secrets visit → exactly 2 audited reads, one per template; a second visit adds 2 more, proving the render cache does not span navigations |
| 4.2 | e2e | `test_secrets_stage_timings_are_measured_and_attached` | A fixed sample of visits → p50 and p95 for the navigation and for each read, attached; fewer samples than declared fails rather than publishing |
| 5.1 | e2e | `test_runners_navigation_returns_and_its_wait_is_attributed` | One Runners navigation → the view renders, and the attached record carries a duration per stage plus the observed attempt count |
| 5.2 | unit | `the render wait is bounded by the declared retry ladder` | The exported retry defaults → the computed worst case equals `RETRY_DEFAULTS.deadlineMs`; the pin also asserts that value is strictly under `deadlineMs + DEFAULT_REQUEST_TIMEOUT_MS`, so removing `attemptTimeoutMs`'s remaining-budget clamp fails it |
| 5.3 | manual | `the Runners verdict is accepted` | Procedure: walk the page's §Runners with the operator who raised finding 14, naming each candidate cause and its evidence. Required person: that operator. Evidence: a dated verdict line in Discovery listing the accepted causes and the unresolved ones |
| 2.1 | e2e | `test_fleets_wall_request_inventory_is_measured` | Negative: the audit disabled → the reset assertion fails the test rather than reporting a zero-count inventory |
| 4.1 | e2e | `test_secrets_visit_pays_two_upstream_reads` | Negative: a non-zero pre-count → the test fails instead of subtracting the earlier reads |
| 5.1 | e2e | `test_runners_navigation_returns_and_its_wait_is_attributed` | Negative: an upstream that answers slowly enough to exhaust the ladder → the recorded attempt count is the attempt ceiling and the failure is surfaced, never rendered as an empty view |
| 3.1 | unit | `the status matrix is what each surface renders` | Negative: a surface rendering a variant the matrix does not carry → the assertion names the surface and the variant |

Regression rows: `the audited path set covers every wall read` is the regression guard — it fails if a later read joins the load path unaudited, which is the only way this inventory can rot. Idempotency rows: `test_the_settled_wall_issues_no_repeating_read` and `test_secrets_visit_pays_two_upstream_reads` are the pair — one proves a settled surface repeats nothing, the other proves a repeated visit repeats exactly what it should.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The page answers all five findings and the index points at each answer (§1–§5) | `grep -c 'dashboard_load.md' docs/architecture/README.md` | `6` | P0 | |
| R2 | Each measured surface publishes a p50 and a p95 (§1, §2, §4, §5) | `grep -cE '^\| (Chat load\|Fleets wall\|Secrets\|Runners) \|' docs/architecture/dashboard_load.md` | `4` | P0 | |
| R3 | The inventory and the stage tables come from a passing live run (§1–§5) | `make acceptance-e2e` | exit 0 | P0 | |
| R4 | The page's three pinned claims hold (§1, §3, §5) | `make test-unit-all` | `every fleet status has a matrix row`, `the render wait is bounded by the declared retry ladder` and `the thread read is in flight before the fleet read is awaited` all pass | P0 | |
| R5 | The unresolved causes are named and accepted (§5) | walk `docs/architecture/dashboard_load.md` §Runners with the operator who raised finding 14 | a dated verdict line in Discovery listing accepted and unresolved causes | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration suite passes | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Lint clean, including the architecture citation gate | `make lint-all` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**R1's expected value was 5 and is 6.** Five question→anchor rows, one per finding, plus the file-table row every page in `docs/architecture/` carries. Dropping the file row to hit the original number would have made this the only page in the set missing from its own directory table.

**Command source rule:** every S-row Verify command is copied verbatim from `.oracle/orly.json` (`conform`, `verify.*`) — the same set `orly gate` runs, so the rubric and the mechanical Pull Request gate grade one boundary. R3's lane is spec-specific: the measurement is live-only and rides an existing target rather than a new one.

**Grading protocol (VERIFY):** run each Verify command verbatim; grade only from its output. Graded = ✅/❌ plus the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell returns to EXECUTE. R5 is P1: an unavailable reviewer is recorded with the reason, and the unresolved causes stay unresolved rather than being resolved by the agent.

## Dead Code Sweep

N/A — no files deleted. The spec adds one page, five index rows, one measurement lane and three pins; it replaces nothing. The audited path set is extended, not rewritten, so no key loses its last reader.

## Out of Scope

- **Every fix M194_001's Pull Request is already making**, so the two streams cannot collide: the tile-counter streaming fix that binds the tile's metrics to the server snapshot rather than the stream, the blocked-approval call to action, the action-button standardisation, the latest-outcome correctness fix, and the fleet-name-versus-agent-slug change. This spec measures; that one changes behaviour.
- **The readiness contract** — a fleet refusing to report active while it cannot fire. Out of scope in M194_001 for the same reason, and unchanged here.
- **Any fix this investigation implies.** Choosing a single status colour, collapsing a read, moving a wait off the render path, or retuning the retry ladder each belong to the successor spec this one's measurements will be quoted in. A measurement that arrives with its own fix is a fix nobody reviewed.
- **New observability families.** The daemon exports no HTTP request-duration histogram today; adding one is a backend change with its own census row and review profile, not a line in a docs stream.
- **Server-side tracing of the upstream query.** If §5 implicates the datastore, proving which statement is slow needs the integration lane and the daemon, and takes its own spec.

---

## Product Clarity (authoring record)

1. **Successful user moment** — the operator asks "why is Runners slow?", opens one page, and reads the answer with the number and the command that produced it. No source reading, no guessing.
2. **Preserved user behaviour** — everything. No route, component, read or connection changes; the only production file touched is a test-only audit whose gate is already off.
3. **Optimal-way check** — the direct shape is to measure the real surfaces through the harness that already counts server-side reads, and to write the answer where the architecture set is already gated. The gap to the unconstrained optimum is that a request-duration metric on the API would answer §5 without a browser; that is a backend change with a different review profile, and it is named in Out of Scope.
4. **Rebuild-vs-iterate** — iterate. The audit, the connection shim, the acceptance lane and the citation gate all exist; this composes them.
5. **What we build** — one page, five index rows, one measurement spec, three pins, one extended path set.
6. **What we do NOT build** — no fix, no new make target, no new metric family, no second measurement harness, no dashboard change.
7. **Fit with existing features** — it compounds with the acceptance lane, which becomes the place a latency claim is graded. The thing it must not destabilise is that lane: the measurement must not leave audit state behind for the specs that follow it.
8. **Surface order** — measurement first, page second. A page written before the numbers exist is the guessing this spec replaces.
9. **Dashboard restraint** — nothing new is shown to a user. No timing is surfaced in the product; the numbers live in the repository until a fix earns a surface.
10. **Confused-user next step** — the page's §Runners names the unresolved causes and, for each, the measurement that would settle it. A reader who disagrees runs the command in the rubric.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one Section per finding, each ending in an artefact and at least one mechanically graded claim. The two questions (§1, §2) produce drawings pinned by counts; the three waits (§3, §4, §5) produce measured tables and pins.
- **Alternatives considered:** folding the measurements into M194_001's open Pull Request — rejected, the budget is one Pull Request per milestone and that diff is already a behaviour change on a security-adjacent path. Measuring by hand in a browser and pasting numbers into the Pull Request body — rejected, an unrepeatable number cannot be regraded and rots on the first deploy. Adding an API request-duration histogram first — rejected as the entry point: it is the better instrument and the wrong scope, so it is named in Out of Scope rather than smuggled in.
- **Patch-vs-refactor verdict:** neither — this is a **measurement**. It deliberately changes no behaviour, which is what lets its numbers serve as the baseline the successor spec is graded against. The larger question it will raise — whether a dynamic server render behind a retry ladder is the right shape for an admin list at all — belongs to that spec, with these numbers in hand.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked plus the decision.

| Date | Consult | Outcome |
|---|---|---|
| Sep 10, 2026 | Premise check — finding 9's "one stream per live fleet" | Read before authoring, and it did not hold: the wall already opens one EventSource per workspace and demultiplexes per fleet, with an acceptance test asserting at most one live connection. The finding's question stands; its premise is corrected here rather than encoded, and §2 measures the inventory instead of assuming it. |
| Sep 10, 2026 | Architecture consult — where the answer lives | `docs/architecture/` over a spec appendix or a Pull Request body: the citation gate there grades paths, links and anchors on every lint run, so a drawing that drifts from the code fails a command. Five index rows keep it reachable by question. |
| Sep 10, 2026 | Gate-flag triage — finding 14's stated cause | Surfaced before authoring: the report names Postgres connections, the code names a bounded retry ladder, and the datastore layer explicitly refuses to conflate a full pool with an absent datastore. Recorded as a hypothesis with the measurement that settles it, never as a finding. |
| Sep 10, 2026 | Sequencing against M195_002 — resolved at CHORE(open) | This line arrived as an unresolved `git stash pop` conflict committed to `main` in `d05c0ff81`. Settled against the two Files Changed tables rather than the prose: they intersect in **zero paths** — M195_002 owns the `rustd/` frame publishers and the wall components, this spec owns docs plus four test files and measures through `ui/packages/app/lib/acceptance/workspace-fetch-audit.ts`. The "must land first" side justified itself with a shared `FleetWall.tsx` and `useWorkspaceStream.ts`; this spec touches neither, so that reason does not hold. Parallel, with §2 deferred. |
| Sep 10, 2026 | Gate-flag triage — the retry ladder's stated worst case | This spec's 5.2 expected the bound to be the deadline PLUS one attempt timeout. The code refuses that by construction, in two places that say so in comments: `ui/packages/app/lib/api/retry.ts:174-178` will not take a sleep that ends past the deadline, and `ui/packages/app/lib/api/client.ts:38-46` hands a late attempt only the remaining budget (`attemptTimeoutMs`), wired at `client.ts:295`. The bound is `deadlineMs` = 20 000 ms, not 30 000 ms. Spec amended; the pin asserts the gap to the unclamped ceiling so the clamp cannot be removed silently. |
| Sep 10, 2026 | Premise check — what the three status surfaces render | Read before writing the matrix, and the finding's picture did not survive it. The detail header renders a status element for `installing` alone; for active/paused/stopped it renders `KillSwitch` and for killed `FleetConfig`, so ACTIVE has no colour there to agree or disagree with. Its fourth branch — the default `<Badge>{status}</Badge>` — is unreachable for every member of the five-status union. **Corrected later the same day: that is NOT dead code.** `FleetDetail.status` is typed `string` (`ui/packages/app/lib/types.ts:55`), not `FleetStatus`, so the branch is a live forward-compatibility fallback for any status the API returns that this client does not know. Removing it would route an unknown status into the `KillSwitch` arm — offering a kill control for a state the dashboard cannot interpret. It stays. |
| Sep 10, 2026 | Architecture consult — where the measurement lane runs | This spec assumed the lane "joins the journeys project with no config edit". It cannot. `playwright.acceptance.config.ts` already isolates `workspace-fetch-dedupe.spec.ts` in its own `fetch-audit` project ordered **strictly last**, with the comment "the audit reset touches an app-global counter, so it must outlast BOTH the wall chain and the operator chain". The measurement resets that same counter, so it takes its own `dashboard-latency` project depending on `fetch-audit`. Sharing `journeys` would have zeroed the counter under specs already counting — a silent wrong number, not a failure. |
| Sep 10, 2026 | Credential gate — the live lane cannot run from this worktree | `tests/e2e/acceptance/global-setup.ts` fails loud on four names. Two resolve from the linked `ui/packages/app/.env.local` (`NEXT_PUBLIC_API_URL`, `CLERK_SECRET_KEY`); **`CLERK_PUBLISHABLE_KEY` and `CLERK_WEBHOOK_SECRET` are absent**, and no worktree-root `.env` exists to carry them. Their fetch locations are named by global-setup's own error text (`op://ZMB_CD_DEV/clerk-dev/{publishable-key,secret-key,webhook-secret}`). The agent does not resolve them — `op read` at runtime is on the operating model's no-override forbidden list — so the run is handed to the operator with the command. Every measured figure stays unpublished until that run happens; none is inferred. |
| Sep 10, 2026 | Measured — finding 14's stated cause does not survive its own numbers | Runners was reported as a page that never returns. Over five navigations it returns at p50 1057 ms (p95 1299 ms), of which the upstream list read is p50 367 ms. **The observed attempt count is 1 on every navigation** — the retry ladder never climbed, so no attempt ever timed out and `PoolTimedOut` never fired. Pool acquisition is therefore not implicated: the hypothesis is refused by measurement rather than left open. The ~690 ms the read does not account for is render and transfer. |
| Sep 10, 2026 | Measured — a Secrets visit costs more than the page's own pair | The first assertion read `total` and got 4 where the Section claims 2. The page's pair is correct — one `workspaceSecrets`, one `tenantProvider` — and the extra two are the dashboard LAYOUT's workspace-switcher read (`/v1/tenants/me/workspaces`), which fires on every navigation and is already audited. Asserting `total` charged the layout's cost to the page. Now asserted per template, with the layout read named as a separate surface's cost. |
| Sep 10, 2026 | Reinvention caught — attachment persistence | A first pass added an `fs.writeFile` helper to persist stage tables, on the belief the harness dropped them. It does not: `playwright.acceptance.config.ts` already wires `html` + `json` reporters whenever `CI` is set, and four existing specs attach evidence through `testInfo.attach` alone. The defect was the invocation — a bare local run gets the `line` reporter. Helper reverted; the lane runs with `CI=1` when the numbers are the point. |
| Sep 10, 2026 | Measured — the layout read needs no fix | A candidate fix (dedupe the dashboard layout's workspace-switcher read) was selected on the belief it fired repeatedly per navigation. Measured across three surfaces in one interleaved window: **1 read per navigation on every surface**. `lib/workspace.ts` already wraps it in React `cache()` for exactly this reason, and it works. No defect, no fix. The earlier count of 2 came from the first navigation after sign-in; whether a full document load renders the root layout and the `[workspaceId]` guard in passes `cache()` cannot span is a HYPOTHESIS, recorded unresolved with the measurement that would settle it — a per-navigation count taken on a cold load versus a client transition. |
| Sep 10, 2026 | Rejected fix — removing the header's default badge | Selected as a safe dead-code removal on the agent's claim of "provably zero behaviour change", and withdrawn before any edit. The claim rested on `fleet.status` being the closed `FleetStatus` union; it is `string`. The branch is the only thing standing between an unrecognised status and a `KillSwitch`, so removing it is a behaviour change in the dangerous direction. Recorded because the approval was obtained on wrong information, not because the reviewer erred. |

- **Metrics review** — events added, extra events found during `/review`, analytics or funnel playbook update, or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an operator-acked verbatim quote here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is incomplete scope, not deferral, and blocks CHORE(close) until the item lands or the quote is captured.

> Indy (2026-09-10 21:37 IST): "Start now, §2 deferred" — context: §2 (the wall's request inventory, Dimensions 2.1–2.3) is held until M195_002 lands, because that spec removes the wall's separate counter fetch and any inventory measured before it is stale by construction. §1, §3, §4 and §5 proceed now. §2 stays `IN_PROGRESS` at CHORE(close) rather than `DONE`.

> Indy (2026-09-10 23:05 IST): "Okay lets instrument and fix the fix needed" — context: authorises per-request timing and attempt counting in `ui/packages/app/lib/api/client.ts`, a production transport file outside the Files Changed table as originally written. Unblocks Dimension 5.1 in full and 4.2's per-read half. The instrumentation is inert unless `AGENTSFLEET_E2E_AUDIT` is set, and wraps rather than replaces a caller's own attempt callback.

> Indy (2026-09-10 22:05 IST): "Run against dev" — context: the acceptance lane seeds and deletes fleets on the shared `api-dev.agentsfleet.net` deployment and signs in with real Clerk credentials; the operator chose that over a local backend, a write-free skip, or parking the measured dimensions. The lane seeds under the `dash-latency-` prefix and tears down in `afterEach`, matching the existing acceptance specs.
