<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M136_001: Live Slack and GitHub reviewer proof

**Prototype:** v2.0.0
**Milestone:** M136
**Workstream:** 001
**Date:** Jul 20, 2026
**Status:** IN_PROGRESS — resumed on Indy's Aug 15, 2026 call after Pull Request (PR) #606 repaired the development acceptance lanes ("merged to main go")

> **Disposition at close.** **Delivered:** the acceptance harness (green locally), the deployed provider prerequisites, and a unit-coverage batch. **Deferred: §1–§5 entire** — no Dimension marked DONE and no Acceptance Rubric row graded, because the live pass (Slack mention, GitHub delivery, replay, Live Wall) never ran. §4 replays §3's exact delivery, so the five sections are one execution and move together to the follow-up agent; the work-order prompt is in PR #600's Session notes.
> **Resumed:** the follow-up owns one ordered development-environment pass across §1–§5. No provider mutation begins until the runner heartbeat advances and the workspace, repository, Fleet, grant, and control-Fleet tuple resolves uniquely.
**Priority:** P0 — the flagship reviewer scenario remains incomplete until real provider authorization and replay safety pass.
**Categories:** DOCS, INFRA
**Batch:** B1 — starts after the development runner is online
**Branch:** `feat/m136-live-connector-proof-followup`
**Test Baseline:** unit=3907 integration=638
**Depends on:** M135_002 (online runner with advancing heartbeat); M135_001 (provider bags, callback routes, and registration grants); M133_001 (workspace-multiplexed Live Wall, visually accepted with exhaustive deployed proof delegated here)
**Provenance:** human-directed successor to M135_001 after the Jul 20, 2026 scope decision
**Canonical architecture:** `docs/architecture/scenarios/github-pr-reviewer.md` §Remaining proof punch list

---

## Overview

**Goal (testable):** A real Slack mention succeeds and one signed GitHub delivery creates exactly one reviewer event and review while replay creates neither again; the same run proves the Live Wall counter and the `github-pr-reviewer` tile update over one workspace stream.
**Problem:** Provider applications and callbacks are deployment-ready, but Slack workspace authorization and the real repository reviewer path lack release-grade evidence, and the development runner must be online before the fleet can execute. M133's wall was visually accepted after deployed artifacts showed live Fleet tiles, but its automated `live-counter` and `pulse-wall` projects were not scheduled because earlier acceptance journeys failed first.
**Solution summary:** After M135_002 proves runner liveness, complete the existing browser-mediated Slack and GitHub authorization flows, prove Slack independently at workspace level, run `github-pr-reviewer` against the dedicated proof repository, and replay the exact GitHub delivery. During that real run, observe the wall before install, after activation, during delivery, and after reconnect: the live count changes exactly once, one workspace stream remains open, the reviewer tile alone shows the new activity, and replay does not add another event or pulse. Keep provider material in vaulted handles and record only redacted identifiers and counts.

## PR Intent & comprehension handshake

- **PR title (eventual):** test(connectors): prove live reviewer integrations
- **Intent (one sentence):** A release operator can trust that real Slack and GitHub integrations execute once, use only declared grants, and remain replay-safe.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.
- **Restatement:** One development run must prove the real Slack workspace event and GitHub review, preserve the original delivery identifiers for replay, and show the Live Wall routes and backfills that activity once.
- **ASSUMPTIONS I'M MAKING:** 1. The deployed target tuple is discovered read-only and must be unique. 2. The existing provider browser flows remain the authorization surface. 3. Any runner, ownership, grant, or repository mismatch stops before the first provider mutation. 4. Indy's Aug 15, 2026 scope decision adds the setup defects that block the live proof to this Pull Request (PR); §1–§5 remain open until that fix is deployed.

## Implementing agent — read these first

1. `docs/architecture/scenarios/github-pr-reviewer.md` — exact external proof still open.
2. `docs/architecture/connectors.md` — installation, repository subscription, grant, mint, and replay boundaries.
3. `docs/architecture/runner_fleet.md` — online heartbeat prerequisite and execution boundary.
4. `tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md` — GitHub-only provider dependency.
5. `playbooks/operations/slack_app_registration/001_playbook.md` — workspace OAuth and signed event verification.
6. `docs/architecture/data_flow.md` — one workspace stream, Fleet-tagged frames, reconnect backfill, and wall routing contract.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/active/M136_001_P0_DOCS_INFRA_LIVE_CONNECTOR_PROOF.md` | EDIT | Record the resumed branch, setup-blocker scope, current baseline, and ungraded live state. |
| `docs/v2/pending/M166_001_P1_INFRA_VERIFICATION_CRITICAL_PATH.md` | CREATE | Specify a separate reduction of the repository verification critical path. |
| `docs/AUTH.md` | EDIT | Document reconnect after external GitHub installation, internal binding drift, and identity-bound callback completion. |
| `docs/architecture/connectors.md` | EDIT | Pin installation discovery, identity-bound callback completion, Disconnect ownership, and retry behavior. |
| `docs/architecture/scenarios/github-pr-reviewer.md` | EDIT | Keep the live reviewer flow aligned with the dashboard callback and same-identity completion boundary. |
| `docs/architecture/scenarios/github-pr-reviewer.md` | EDIT | Mark external proof complete only after every live rubric row passes. |
| `playbooks/operations/{slack,github,zoho,jira,linear}_app_registration/001_playbook.md` | EDIT | Move every provider registration to the authenticated dashboard callback URL. |
| `public/openapi/paths/connectors.yaml` | EDIT | Document connector deletion on the existing workspace connector resource. |
| `public/openapi.json` | REGENERATE | Keep the bundled public API description aligned with the connector resource. |
| `src/agentsfleetd/http/handlers/connectors/connect.zig` | EDIT | Start GitHub connection with user authorization so an existing installation can be discovered. |
| `src/agentsfleetd/http/handlers/connectors/{binding_tx,disconnect}.zig` | CREATE | Serialize callback and Disconnect writers, then remove one workspace connector binding and vault handle idempotently. |
| `src/agentsfleetd/http/handlers/connectors/sql.zig` | CREATE | Own generic connector delete and serialization statements. |
| `src/agentsfleetd/http/handlers/connectors/{state,callback,oauth2}.zig` | EDIT | Bind signed state to the starter identity, relay provider returns through the dashboard, and complete only under the same authenticated identity. |
| `src/agentsfleetd/http/handlers/connectors/registry.zig` | EDIT | Register the GitHub user-authorization connection hook. |
| `src/agentsfleetd/http/handlers/connectors/github/{connect,callback,ownership,spec,sql}.zig` | EDIT | Discover an accessible existing installation, preserve claimed-install ownership proof, and share workspace binding statements. |
| `src/agentsfleetd/http/handlers/connectors/{slack,zoho,jira,linear}/callback.zig` | EDIT | Commit each provider callback's vault and routing state under the same workspace/provider writer guard as Disconnect. |
| `src/agentsfleetd/http/handlers/connectors/slack/sql.zig` | EDIT | Keep Slack's install routing statement in its provider SQL module. |
| `src/agentsfleetd/http/handlers/connectors/{registry,github/callback,oauth_providers_integration_test,slack/oauth_callback_integration_test}.zig` | EDIT | Prove OAuth-first connect, identity-bound completion, drift repair, Disconnect idempotency, and no cross-workspace mutation. |
| `src/agentsfleetd/http/handlers/connectors/binding_tx_integration_test.zig` | CREATE | Prove one workspace/provider writer waits until the current binding transaction ends. |
| `schema/{551_connector_installs_delete_privilege.sql,embed.zig}` and `src/agentsfleetd/db/pool_test.zig` | CREATE / EDIT | Grant and prove the runtime DELETE needed by Disconnect and GitHub drift repair. |
| `src/agentsfleetd/integration_tests.zig` | EDIT | Include the connector-writer integration proof in the repository integration lane. |
| `src/agentsfleetd/http/route_{matchers,matchers_connectors,scopes,table,table_invoke_connectors,template,router,routes}.zig` | EDIT | Keep legacy API callback relay separate from authenticated callbacks completion. |
| `ui/packages/app/app/api/connectors/[provider]/callback/route.ts` | CREATE | Relay a provider return with the current dashboard session token. |
| `ui/packages/app/tests/connector-callback-route.test.ts` | CREATE | Prove token forwarding and refuse unauthenticated or cross-origin completion. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/integrations/{connector-actions.ts,components/connector-rows.tsx}` | EDIT | Expose exact `Connect` and `Disconnect` actions and refresh status after deletion. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/InstallStates.tsx` | EDIT | Send GitHub setup to Integrations while other missing secrets still use Secrets. |
| `ui/packages/app/lib/{api/connectors.ts,workspace-routes.ts}` | EDIT | Add connector deletion and the named Integrations route. |
| `ui/packages/app/tests/{connector-actions,integrations-connectors,fleets-install-states}.test.ts` | EDIT | Pin action forwarding, labels, deletion, and provider-aware setup routing. |
| `cli/test/acceptance/fixtures/{constants,seed,template-ops}.ts` | EDIT | Install a credential-free, label-free steer probe on any online development runner. |
| `cli/test/acceptance/steer-live.spec.ts` | EDIT | Use the steer probe instead of the runner-constrained platform operations Fleet. |
| `cli/test/template-ops.unit.test.ts` | EDIT | Prove the steer probe carries no placement, credential, tool, or provider-trigger gate and allows only its inference host. |
| `tests/fixtures/fleetbundle/steer-probe/{SKILL,TRIGGER}.md` | CREATE | Provide the minimal manual Fleet used only by live steer acceptance. |
| `ui/packages/app/tests/e2e/acceptance/_smoke.spec.ts` | EDIT | Name the runner readiness and advancing-heartbeat proof before every mutating project. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/preflight.ts` | EDIT | Add bounded same-runner heartbeat comparison with redacted failure output. |
| `ui/packages/app/tests/release-gate-suite-config.test.ts` | EDIT | Pin offline readiness and same-runner heartbeat advancement without a deployed dependency. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC, NLR, NLG, PRI, ORP, WAUTH, IDMP, OAE, HXX, RAD, HGD, CNX, and TXN: no stale setup path, untrusted provider payload, orphaned marker, secret-bearing evidence, cross-workspace mutation, duplicate connection, or partial deletion.
- **`dispatch/write_documentation.md` and `docs/DOCUMENTATION_RULES.md`** — operator steps remain literal, redacted, and independently verifiable.
- **`dispatch/name_architecture.md`** — connector, grant, event, review, and replay terminology remains canonical.

## Applicable Gates

§5's proof is automation, not observation, so the TypeScript dispatch fires on the acceptance specs. Provider, secret, repository, and runner mutations remain governed by their operational safety rules.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| TypeScript FILE SHAPE DECISION | yes — one new acceptance spec | shape verdict at PLAN before the file lands |
| File & Function Length (≤350/≤50/≤70) | yes — the wall specs grow a lifecycle walk | the reviewer walk is its own spec file rather than growth on the two existing ones |
| UFS (repeated/semantic literals) | yes — seed prefixes, stream paths, and timeouts repeat | named constants per file, sharing the fixture vocabulary the acceptance suite already uses |
| UI Substitution / DESIGN TOKEN | yes — connector actions change | existing `Button`, `Alert`, `StatusPill`, and `DashboardRow` primitives; no new token values |
| ZIG / PUB / LIFECYCLE / SCHEMA / LOGGING | yes — connector handlers and routing change | Hx handler shape, externally consumed `pub` only, shared SQL statements, existing structured connector log vocabulary |
| MILESTONE-ID | yes — the new spec is net-new source | the file header cites `M136_001` |

## Prior-Art / Reference Implementations

- **Reference:** `docs/architecture/scenarios/github-pr-reviewer.md` — local datastore proof defines the event, grant, mint, and replay behavior the external run must match.

## Sections (implementation slices)

### §0 — Live-setup blockers are repaired before external proof

The development proof exposed common drift: an external provider authorization can survive a datastore rebuild while the `agentsfleet` workspace handle and reverse-routing row do not. Every connector gets the same `Connect` / `Disconnect` surface. OAuth providers rerun their existing authorization callback; GitHub additionally discovers an accessible existing App installation because its install URL otherwise strands the browser on GitHub settings. Repair the connection and fleet-install paths before resuming §1.

- **Dimension 0.1** — `Connect` authorizes the GitHub user and restores the unique accessible existing installation to the selected workspace → Test `test_github_connect_restores_existing_installation`
- **Dimension 0.2** — `Disconnect` removes the `agentsfleet` vault handle and reverse-routing row, is safe to retry, and does not uninstall the external GitHub App → Test `test_connector_disconnect_is_idempotent`
- **Dimension 0.3** — a GitHub-dependent Fleet links to workspace Integrations; other custom secrets still link to Secrets → Test `test_github_install_gate_routes_to_integrations`
- **Dimension 0.4** — live steer installs a label-free and credential-free probe so any online development runner can lease and process it → Test `test_steer_probe_has_no_runner_or_secret_gate`
- **Dimension 0.5** — every provider callback and Disconnect commits all connector rows under one workspace/provider writer guard → Test `integration: connector writers wait on the shared workspace provider lock`
- **Dimension 0.6** — a provider return completes only for the identity that started its signed state; another authenticated identity cannot consume the state or bind a provider account to the starter workspace → Test `integration: completion rejects a different identity without consuming the starter state`

### §1 — Preconditions are live facts

Verify the deployed prerequisites and that M135_002 reports an online runner whose `last_seen_at` advances before provider proof begins.

- **Dimension 1.1** — readiness fails before external mutation when the runner is not online → Test `test_connector_proof_requires_online_runner`
- **Dimension 1.2** — two bounded reads observe `last_seen_at` advance → Test `test_runner_last_seen_advances`

### §2 — Slack is independently proven

Authorize the intended Slack workspace through the existing browser flow. Because the reviewer bundle declares GitHub only, Slack remains a workspace proof and is never injected into that fleet.

- **Dimension 2.1** — status identifies the intended Slack workspace as connected → Test `test_slack_workspace_connection_status`
- **Dimension 2.2** — one signed mention produces one accepted workspace event without secret output → Test `test_slack_signed_mention_once`

### §3 — GitHub reviewer executes exactly once

Use the existing installation, selected proof repository, installed fleet, and approved GitHub grant.

- **Dimension 3.1** — one signed delivery creates one fleet event and one fleet-authored review through a short-lived installation token → Test `test_github_reviewer_posts_once`
- **Dimension 3.2** — the fleet receives no Slack material because its trigger does not declare Slack → Test `test_reviewer_declared_connectors_only`

### §4 — Replay closes the architecture marker — RESUMED

> **PARKED (Indy, Aug 14, 2026, verbatim):** "i want to take it later". Joins
> §5 in the follow-up agent's work order.
> **RESUMED (Indy, Aug 15, 2026, verbatim):** "merged to main go".
>
> **§4 cannot be run standalone.** It replays *the exact delivery* from §3 and
> needs the event and review identifiers recorded during that run, so it is the
> back half of one live execution, not an independent slice. §1–§3 are in the
> same unrun state, so the follow-up prompt covers §1→§5 as a single live pass
> against the deployed development environment.

Replay the exact delivery only after recording original event and review identifiers.

- **Dimension 4.1** — replay creates no second fleet event → Test `test_github_replay_no_event`
- **Dimension 4.2** — replay creates no second review and only then closes the architecture marker → Test `test_github_replay_no_review`

### §5 — The real reviewer proves the Live Wall — RESUMED

> **PARKED (Indy, Aug 14, 2026, verbatim):** "just make it parked i will ask
> another agent after this PR is merged to implement/execute/ run the scenario
> via playwright, so record in your dimension towards that effect". No §5 code
> or tests ride this PR; Dimensions 5.1–5.4 below stay specified as the
> follow-up agent's work order, to be driven end to end via Playwright against
> the merged branch.
> **RESUMED (Indy, Aug 15, 2026, verbatim):** "merged to main go".

Use the same `github-pr-reviewer` Fleet and delivery from §3 rather than synthetic wall-only data. Record the workspace's live count before installation, after the reviewer becomes active, after the first delivery, and after replay.

- **Dimension 5.1** — activating `github-pr-reviewer` increases the exact `{N} live` wall counter by one; stopping it decreases the counter by one; resuming it restores the count → Test `test_live_counter_tracks_real_reviewer_lifecycle`
- **Dimension 5.2** — with the reviewer and at least one control Fleet visible, the wall opens exactly one workspace events stream and no per-tile Fleet streams → Test `test_wall_uses_one_workspace_stream_for_multiple_fleets`
- **Dimension 5.3** — the signed GitHub delivery updates/pulses the `github-pr-reviewer` tile while the control Fleet does not show that delivery → Test `test_reviewer_delivery_routes_to_only_its_tile`
- **Dimension 5.4** — disconnect/reconnect keeps last-known tiles visible, backfills the reviewer event, and does not duplicate its activity; replay also leaves the event/review counts and tile activity unchanged → Test `test_wall_reconnect_and_replay_do_not_duplicate_reviewer_activity`

## Interfaces

```text
GET  /v1/fleets/runners
GET  /v1/workspaces/{workspace_id}/connectors/{provider}
DELETE /v1/workspaces/{workspace_id}/connectors/{provider}
POST /v1/connectors/{provider}/callback
POST /v1/ingress/slack
POST /v1/ingress/github

Provider credentials remain platform bag -> workspace handle -> approved fleet grant.
Evidence records redacted resource identifiers, delivery identifiers, and counts only.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Runner stale | service exists but heartbeat does not advance | Stop before provider mutation; report runner as offline. |
| Consent denied | operator or provider declines OAuth | Preserve prior connector state and record no connected claim. |
| External/internal drift | provider authorization survives while the workspace binding is absent | Rerun provider authorization; for GitHub, discover the unique accessible installation; restore both internal records. |
| Ambiguous installations | the GitHub user can access more than one installation for this App | Refuse to bind an arbitrary installation; preserve disconnected status. |
| Shared provider URL | a different person opens the starter's copied provider authorization URL | The dashboard relays the returning session; the backend rejects a mismatched signed-state identity before nonce consumption, code exchange, or connector write. |
| Disconnect retry | connector handle or routing row is already absent | Return 204 with no external uninstall and no partial state. |
| Wrong target | grant does not match the intended workspace or repository | Stop before delivery or review and preserve ownership. |
| Provider unavailable | Slack or GitHub request fails | Bounded failure with redacted diagnostics; retry remains idempotent. |
| Duplicate delivery | GitHub retries the same delivery id | Existing replay boundary creates no second event or review. |

## Invariants

1. Raw provider and runner credentials never enter evidence — gitleaks and redacted status-only commands enforce the boundary.
2. Slack is not granted to `github-pr-reviewer` unless its bundle declares Slack — the grant resolver rejects undeclared provider access.
3. Architecture status becomes complete only after original and replay counts are observed — rubric R4 gates the marker edit.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `connector_readiness_proof` | ops | each provider or replay proof completes | provider, redacted resource id, outcome, duration | no email, token, code, signature, or payload body | `test_connector_evidence_redaction` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 0.1 | integration | `test_github_connect_restores_existing_installation` | user authorization with one accessible installation writes the workspace handle and route, then returns to Integrations |
| 0.2 | integration | `test_connector_disconnect_is_idempotent` | two deletes return 204; handle and routing row stay absent; no provider uninstall call occurs |
| 0.3 | unit | `test_github_install_gate_routes_to_integrations` | missing GitHub renders `Connect` to `/w/{workspace_id}/integrations`; a non-connector secret renders `Add token` to Secrets |
| 0.4 | unit | `test_steer_probe_has_no_runner_or_secret_gate` | steer bundle declares no tags, credentials, tools, or provider triggers and allows only the Fireworks inference host required for execution |
| 0.5 | integration | `integration: connector writers wait on the shared workspace provider lock` | one transaction holds `(provider, workspace_id)` while a second writer waits; releasing the first permits the second to proceed |
| 0.6 | integration | `integration: completion rejects a different identity without consuming the starter state` | a different authenticated identity receives 400 `UZ-CONN-002`; the starter identity can still consume the same state and complete the connector binding |
| 1.1 | end-to-end | `test_connector_proof_requires_online_runner` | stale runner prevents external mutation |
| 1.2 | end-to-end | `test_runner_last_seen_advances` | bounded reads show advancing `last_seen_at` |
| 2.1 | end-to-end | `test_slack_workspace_connection_status` | connector status matches intended workspace |
| 2.2 | end-to-end | `test_slack_signed_mention_once` | signed mention is accepted exactly once |
| 3.1 | end-to-end | `test_github_reviewer_posts_once` | signed delivery yields one event and one review |
| 3.2 | integration | `test_reviewer_declared_connectors_only` | reviewer has GitHub but no undeclared Slack grant |
| 4.1 | end-to-end | `test_github_replay_no_event` | replay leaves event count unchanged |
| 4.2 | end-to-end | `test_github_replay_no_review` | replay leaves review count unchanged before marker closes |
| 5.1 | end-to-end | `test_live_counter_tracks_real_reviewer_lifecycle` | reviewer active changes `{N} live` to `{N+1} live`; stop returns it to `{N}`; resume restores `{N+1}` |
| 5.2 | end-to-end | `test_wall_uses_one_workspace_stream_for_multiple_fleets` | reviewer plus control Fleet produce one `/workspaces/{ws}/events/stream` connection and zero wall-owned `/fleets/{id}/events/stream` connections |
| 5.3 | end-to-end | `test_reviewer_delivery_routes_to_only_its_tile` | one signed delivery changes the reviewer tile once and leaves the control tile unchanged |
| 5.4 | end-to-end | `test_wall_reconnect_and_replay_do_not_duplicate_reviewer_activity` | reconnect restores the reviewer event exactly once; replay changes neither provider counts nor wall activity |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R0 | Setup blockers no longer prevent the live pass | repository tests plus deployed `Connect` / `Disconnect` retry | existing GitHub installation reconnects; Disconnect clears internal status; reviewer setup returns to Integrations; live steer processes | P0 | |
| R1 | Runner heartbeat is fresh | `agentsfleet runner list --json | jq -e '.[] | select(.status == "online" and .last_seen_at != null)'` | exit 0 across two reads with advancing `last_seen_at` | P0 | |
| R2 | Slack workspace is connected | `agentsfleet connector list --workspace "$WORKSPACE_ID" --json | jq -e '.[] | select(.provider == "slack" and .status == "connected")'` | exit 0 plus one redacted mention event id | P0 | |
| R3 | Real Pull Request is reviewed exactly once | `gh pr view "$PROOF_PR" --json reviews,comments` | exactly one fleet-authored review | P0 | |
| R4 | Replay is idempotent and architecture proof closes | `rg -n 'External .github-pr-reviewer. repository test.*✅' docs/architecture/scenarios/github-pr-reviewer.md` | one match after unchanged event and review counts | P0 | |
| R5 | Live counter tracks the real reviewer lifecycle | browser evidence for the proof workspace before activation, active, stopped, and resumed | exact sequence `{N} live` → `{N+1} live` → `{N} live` → `{N+1} live` | P0 | |
| R6 | Multiple Fleet tiles use one workspace stream | browser network evidence filtered to `/events/stream` with reviewer and control Fleet visible | exactly one workspace stream; zero wall-owned per-Fleet streams | P0 | |
| R7 | Real delivery routes once to the reviewer tile | side-by-side wall evidence before delivery, after delivery, after reconnect, and after replay | reviewer changes once; control Fleet unchanged; reconnect/replay add no duplicate | P0 | |
| S1 | Repository checks pass | `make lint-all` | exit 0 | P0 | |
| S2 | No secrets | `gitleaks detect --no-banner` | exit 0 | P0 | |
| S3 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Runner credential provisioning or heartbeat repair — M135_002.
- Adding Slack to the GitHub reviewer bundle when it is not declared.
- Merging the proof Pull Request or changing unrelated repositories.
- Production deployment, release, or tag creation.
- Reopening M133 implementation work unless the real reviewer proof demonstrates a reproducible wall defect.

---

## Product Clarity (authoring record)

1. **Successful user moment** — one real Pull Request receives one fleet-authored review and a replay changes nothing.
2. **Preserved user behaviour** — connector authorization and fleet grant approval remain the existing browser and app flows.
3. **Optimal-way check** — external evidence is required because local tests cannot prove consent, ownership, or review posting.
4. **Rebuild-vs-iterate** — iterate on shipped plumbing; no connector rewrite is justified.
5. **What we build** — redacted Slack mention, GitHub review, and replay evidence plus the architecture marker update.
6. **What we do NOT build** — new providers, pasted tokens, a Slack fleet dependency, or runner provisioning.
7. **Fit with existing features** — compounds with integration grants, short-lived provider tokens, fleet events, and runner execution.
8. **Surface order** — UI-first for consent, CLI and provider interfaces for repeatable observation.
9. **Dashboard restraint** — connected and completed claims appear only after provider and replay status confirm them.
10. **Confused-user next step** — readiness identifies the stale runner, disconnected provider, missing grant, or mismatched repository before mutation.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one successor owns both external providers and replay evidence because together they close one reviewer scenario.
- **Alternatives considered:** folding proof into M135_001 was rejected after runner availability blocked execution; claiming registration as proof was rejected as false evidence.
- **Patch-vs-refactor verdict:** this is a **patch** because the architecture is implemented and locally tested; only external binding and evidence remain.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage:
- **Metrics review** —
- **Skill-chain outcomes** —
- **Live-setup defect scope (Aug 15, 2026).** Indy asked for the defects in this PR so M136 can continue after deployment: "Just add these in your PR these are the testing bugz" and later "Orly I need a PR for this so we could keep the M136_001 and continue the test." The GitHub correction is explicit: "bug 1 is on GH not slack slack connection worked if you notice in my integrations". External authorization can survive outside `agentsfleet` for any provider after datastore teardown. Every connector therefore gets the same retry surface. GitHub additionally needs installation discovery because its existing-install path does not return through the callback. `Disconnect` clears only `agentsfleet` state and never revokes or uninstalls the external provider app.
- **The harness runs green locally (Aug 11, 2026) — corrected.** An earlier note in this spec claimed local proof was blocked on Vercel deployment protection. That was wrong, and the correction matters because it would send a pickup agent chasing a secret they do not need. Setting `BASE_URL=https://app-dev.agentsfleet.net` routes the browser at the deployed dashboard, which sits behind Vercel Single Sign-On (`302` to `vercel.com/sso-api`, `set-cookie: _vercel_sso_nonce`) — hence three browser-side failures. Omitting `BASE_URL` makes the config build and serve the app itself (`playwright.acceptance.config.ts:166-173`) while the API still points at `api-dev`. Run that way, **preflight passes 13 of 13**, teardown revokes its sessions, and the sweeps find nothing leaked. `VERCEL_BYPASS_SECRET` is needed only when driving the deployed dashboard.

  **Canonical local invocation:** `cd ui/packages/app && NEXT_PUBLIC_API_URL=https://api-dev.agentsfleet.net bunx playwright test --config=playwright.acceptance.config.ts` — note no `BASE_URL`.

- **Parked (Aug 11, 2026) — priority, not blockage.** §5's acceptance specs are unwritten and §1–§4 external proof is unrun; the workstream is paused so M160_002 can be implemented first. The harness is proven working, so this resumes without further environment work.
  > Indy (2026-08-11): "I think if you are able to run the test i defer till we do the full login and so on? can we defer the tests and move on the 161 implementation? I would defer the test to CI" — context: M160_002 takes priority. This workstream stays IN_PROGRESS with no Section marked DONE.

- **Un-parked (Aug 12, 2026).** The parking above is lifted. This workstream shares a branch and a worktree with M160_002, so one Pull Request (PR) carries both, and CHORE(close) requires both specs in `done/` — §5's acceptance specs get written and §1–§4's external proof gets run rather than deferred.
  > Indy (2026-08-12): "Finish M136's §5 too" — context: asked directly whether to park M136 out of the shared PR's scope, finish it, or split the branches apart. Finishing was chosen.

- **The 350-line cap is a ceiling, not a target (Aug 16, 2026).** RULE FLL's 350
  lines is a hard refusal boundary, and a file sitting at 340 is not therefore
  correct. Split whenever the implementing agent's own rubric says two files are
  better than one — unit-test coverage, simplicity, optimisation, performance,
  concurrency, adversarial review. Two files of ~150 lines each is a good
  outcome, not an over-split.

  The measured case that prompted this: `src/lib/call_deadline/scheduler.zig`
  sat at **exactly 350** with eight dark lines and no room for the tests that
  would clear them. Splitting the clock and futex wait into
  `MonotonicBackend.zig` and moving three test-support fakes into
  `scheduler_test.zig` took the file to 291 lines and the module to **100%**
  coverage — three of those lines left the denominator as the test support they
  always were, five were driven. The cap was never the obstacle; treating it as
  a budget to spend was.

  > Indy (2026-08-16): "On the Cap gate i think there is a strict cap of 350
  > also if you feel with your rubric for unit testing coverage, better
  > simplicity, optimized code, performant, concurrency, adversarial think the
  > code could be broken to two files then you should even have 2 files of 150
  > lines or so in it."

  **Pending, not applied:** the durable home for this is RULE FLL in
  `~/Projects/dotfiles/dispatch/write_any.md` §File & Function Length Gate.
  That edit is **not** made here — dotfiles governance is in flight on Indy's
  side, and the agent must ask for approval before touching that checkout.
  Recorded in this spec so the decision is not lost while the rule text waits.

- **Deferrals** —
  > Indy (2026-07-20 22:23): "And move th 2,3,4 to the next milestone and read and move this milestone to done?" — context: live Slack authorization/signed mention and real GitHub review/replay proof move from M135_001 to this successor; runner activation remains M135_002 and is this workstream's prerequisite.
  > Indy (2026-07-26): "I think move this to DONE. I have eyeballed it, on fleets getting added, i will do an exhaustive check in M136_001" — context: M133 closes on direct visual acceptance; M136 inherits the unrun deployed `live-counter` and `pulse-wall` proof and must exercise them with the real `github-pr-reviewer` Fleet.
