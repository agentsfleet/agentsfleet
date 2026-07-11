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

# M102_005: Agent-identity-proxy tail — App-webhook ingress, connector command-line interface, and documentation truth

**Prototype:** v2.0.0
**Milestone:** M102
**Workstream:** 005
**Date:** Jul 05, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing App-webhook receipt, `agentsfleet connector` command-line interface (CLI), and documentation truth. The grant gate shipped in M102_006.
**Categories:** API, CLI, DOCS
**Batch:** B1 — spec + canonical architecture truth first; §1 ingress; §3 CLI; operator documentation and changelog after behavior is verified. §2 shipped in M102_006.
**Branch:** `feat/m102-agent-identity-tail`
**Test Baseline:** unit=2486 integration=290 via `make _lint_zig_test_depth`
**Depends on:** M102_001 (merged, Pull Request (PR) #458 — mint broker + GitHub connect) and M102_006 (DONE — grant-gated lease and mint).
**Provenance:** LLM-drafted (Claude, Jul 05, 2026) — carved from M102_001's Discovery log + a live main-tree audit this session that found §6 grant enforcement absent (mint calls `broker.mint` with no grant check; `secrets_resolve.mintableId` classifies without a grant read).
**Canonical architecture:** `docs/AUTH.md` + `docs/architecture/{connectors,data_flow,runner_fleet,user_flow}.md`. Introduces no new trust plane — inbound verification uses the platform App secret and token minting reuses the runner-token (`agt_r`) plane.

---

## Overview

**Goal (testable):** `POST /v1/ingress/{provider}` verifies a GitHub App signature before reading the payload, resolves `installation.id → workspace`, and fans Pull Request or failed Actions events only to active fleets whose GitHub trigger explicitly matches the repository and event and whose GitHub grant is approved; each delivery reaches each fleet once; the registry-wide `agentsfleet connector` CLI reports live provider state; and both documentation homes explain the App path without deleting the manual per-fleet fallback.

**Problem:** GitHub connect currently stores only an encrypted installation handle, so there is no reverse installation-to-workspace map; the App webhook is disabled by the operator playbook and has no ingress; fleet triggers have no repository subscription, so App-wide traffic would fan out too broadly; `github-pr-reviewer` cannot pass its end-to-end Pull Request test; connector state has no CLI surface; and the internal and operator docs mix the old manual-hook model with the shipped App credential model.

**Solution summary:** make the GitHub callback atomically store the vault handle plus a `core.connector_installs` reverse map; add bounded `repositories` to GitHub webhook triggers; activate the App webhook and verify it through the existing provider registry; route by installation, repository, event, and approved grant with per-fleet replay slots; add registry-wide connector status commands; preserve the old per-fleet GitHub and Slack ingress behavior; then update architecture, playbook, scenario, and operator documentation.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m102): agent-identity-proxy tail — App-webhook ingress, connector CLI, docs sweep`
- **Intent (one sentence):** finish the agent-identity proxy — an installed GitHub App's events actually reach the right fleet, the operator can see connector state from the CLI, and the docs stop lying about the webhook model.
- **Handshake (PLAN, Jul 11, 2026):** finish the App path without weakening existing connector behavior. **ASSUMPTIONS I'M MAKING:** the manual GitHub route remains supported; App ingress ignores GitHub triggers without `repositories`; multiple explicitly subscribed fleets may receive the same event; replay protection is per delivery and fleet; Slack keeps its specialized ingress; Jira and Linear gain no event ingress here; connector CLI state covers the live registry; both documentation homes ship with the behavior.

## Implementing agent — read these first

1. `docs/v2/done/M102_001_P1_API_CLI_UI_AGENT_IDENTITY_PROXY_GITHUB_APP.md` — the parent; its §5/§6/§7/§8 dimension text, Interfaces, Failure Modes, and **Invariant 3** are the blueprint. This workstream implements exactly its deferred tail. **Do not edit it.**
2. `docs/AUTH.md` §"credential boundary", §"Runner token", §"Webhook auth" — the boundary + the `agt_r` plane + the existing verifier. **Auth-flow file — read before any ingress code.**
3. `src/agentsfleetd/fleet_runtime/webhook_verify.zig` — `PROVIDER_REGISTRY` (SLACK/GITHUB/LINEAR verify descriptors + the compile-time dup guard); the ingress extends the descriptor with `routing_key_path` and seeds github's `installation.id`.
4. `src/agentsfleetd/http/handlers/connectors/github/callback.zig` + `connectors/slack/callback.zig` — GitHub currently lacks the reverse map; mirror Slack's existing `core.connector_installs` upsert without storing credentials in the table.
5. `docs/v2/done/M102_006_P0_API_GRANT_GATED_MINT_LEASE.md` — the approved-grant predicate App ingress uses when selecting fleets.
6. `cli/src/commands/grant.ts` — the existing integration-grant CLI; `connector` mirrors its command→handler→errors split, structured-JSON errors, and `--json` rendering (7 Pillars).
7. `docs/v2/reviews/m102-doc-shape-review.md` — the C1–C9 contradiction map + "Docs to update" list the §4 sweep absorbs verbatim.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/connectors/github/callback.zig` · `connectors/registry_integration_test.zig` | EDIT | atomically store the GitHub vault handle and `installation_id → workspace` connector-install row; prove reconnect updates both |
| `src/agentsfleetd/fleet_runtime/config_{types,helpers}.zig` + tests | EDIT | parse, own, bound, and validate optional webhook `repositories`; App ingress requires an explicit GitHub repository match |
| `src/agentsfleetd/fleet_runtime/webhook/{normalizer/github.zig,normalizer/github_test.zig}` | EDIT | normalize `pull_request` and failed `workflow_run` App events with repository identity |
| `src/agentsfleetd/http/handlers/webhooks/ingress.zig` | CREATE | generic `POST /v1/ingress/{provider}`; verify first, route by installation + repository + event + approved grant, then per-fleet dedup and `XADD` |
| `src/agentsfleetd/fleet_runtime/webhook_verify.zig` | EDIT | extend descriptors with routing metadata; seed GitHub's `installation.id`; retain compile-time validation |
| `src/agentsfleetd/http/routes.zig` · `route_table_invoke*.zig` · matchers | EDIT | register the bearer-less `/v1/ingress/{provider}` route + dispatch (mirror `slack_events` / the connectors table) |
| `src/agentsfleetd/errors/{error_registry,error_entries}.zig` | EDIT | typed App-ingress refusal codes and hints; no secret in any frame |
| `cli/src/commands/connector.ts` · `cli/src/services/connectors.ts` | CREATE | `agentsfleet connector list`/`status`; disconnected is a successful state with a hint, while transport/API failure is structured and non-zero |
| `cli/src/commands/index.ts` (or the command registry) | EDIT | register the `connector` command |
| `docs/AUTH.md` · `docs/architecture/{README,capabilities,connectors,data_flow,high_level,roadmap,runner_fleet,user_flow}.md` · `docs/architecture/scenarios/gh-pr-reviewer.md` | EDIT | full admin-key → workspace install → repository subscription → fleet wake → token-mint walkthrough; preserve manual and Slack paths |
| `playbooks/operations/github_app_registration/001_playbook.md` | EDIT | activate `/v1/ingress/github`; store `webhook_secret` with the App identity; configure Pull Request and Actions events |
| `~/Projects/docs/{fleets/connectors.mdx,fleets/webhooks.mdx,fleets/authoring.mdx,cli/agentsfleet.mdx,changelog.mdx}` | EDIT | operator-facing App connection, repository binding, CLI output, fallback, and release entry on a dedicated docs branch |
| _colocated tests (Zig `test {}` · `*.test.ts` · doc-grep tests)_ | CREATE/EDIT | one test per Dimension below |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **RULE CFG** (provider behavior is registry data), **VLT** (App secrets stay in vault), **PRI/NTP** (webhook and CLI input are hostile), **ECL/EMS/TGU** (typed outcomes and errors), **JCL** (stable CLI JSON shape), **UFS** (route, provider, field, and repository literals are named), **NDC/NLR/ORP**.
- **`docs/AUTH.md`** — auth-flow: the ingress rides the existing verifier (mirror the boundary, add no new plane).
- **`dispatch/write_zig.md`** — tagged-union results, multi-step `errdefer`, pg-drain (the ingress routing queries pg), file ≤350 / fn ≤50, cross-compile both linux targets.
- **`dispatch/write_ts_adhere_bun.md`** — §3 CLI: handler purity (no `console.log`/`process.exit` in handlers), output-as-a-service, structured-JSON errors.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the `/v1/ingress/{provider}` route lives under `src/agentsfleetd/http/handlers/**`.
- No schema Data Definition Language — the existing `core.connector_installs` and `core.integration_grants` tables supply the required mappings.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — ingress handler, verifier descriptor | tagged-union results; `errdefer`; `conn.query().drain()`; cross-compile both linux targets |
| PUB / Struct-Shape | yes — ingress result, extended `VerifyConfig` | shape verdict per new/changed pub surface; tagged-union ingress result |
| File & Function Length (≤350/≤50/≤70) | yes — ingress handler | keep the ingress handler thin (registry does the per-provider work) |
| UFS (repeated/semantic literals) | yes — route path, provider ids, routing-key path | named constants in one module per side; tests import them |
| UI Substitution / DESIGN TOKEN | no — CLI + backend + docs only | — |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | ERROR REGISTRY yes (`UZ-WH-*`); LOGGING yes (VLT on ingress frames); SCHEMA no | register codes with `hint()`; log only non-secret status/host/installation-presence |

## Prior-Art / Reference Implementations

- **Ingress (§1)** — mirror the existing GitHub per-fleet dedup ordering and Slack connector-install lookup. The GitHub callback does not yet write the reverse map; this workstream adds it using the existing generic table.
- **Connector CLI (§3)** — mirror `cli/src/commands/grant.ts` (the existing integration-grant command) + the "7 Pillars" (command→handler→errors split; handler purity; output-as-a-service; structured-JSON errors with a reconnect suggestion; auto-JSON when piped).
- **Docs sweep (§4)** — the review companion `docs/v2/reviews/m102-doc-shape-review.md` is the source of truth for what to change (its "Docs to update" list).

## Sections (implementation slices)

### §1 — Generic App-webhook ingress (`/v1/ingress/{provider}`, data-driven)

The App's one webhook posts to the bearer-less generic route. Verification precedes every payload read. GitHub's descriptor extracts `installation.id`, repository full name, event header, and delivery header; the installation resolves the workspace through `core.connector_installs`. Matching fleets must be active, have an approved GitHub grant, and declare a GitHub webhook trigger whose `repositories` contains the exact repository and whose `events` admits the event. A missing repository list is ignored by the App route and remains valid for the manual per-fleet route. Deduplication is per provider delivery and fleet, and a failed enqueue releases only that fleet's slot.

- **Dimension 1.1** — connect callback writes both rows; signed GitHub Pull Request and failed `workflow_run` deliveries route only to fleets matching workspace, repository, event, and approved grant; bad signature or unknown installation writes nothing → Test `test_ingress_routes_installation_repository_event`
- **Dimension 1.2** — adding a provider is a registry descriptor; the `/v1/ingress/{provider}` route + dispatch carry no per-provider branch (a fake provider descriptor verifies + routes with no handler edit) → Test `test_ingress_registry_is_data_driven`
- **Dimension 1.3** — a GitHub trigger without `repositories` receives no App delivery; manual per-fleet GitHub and specialized Slack ingress remain green; retries deliver once per matching fleet and recover a failed fan-out leg → Test `test_ingress_repository_binding_and_replay`

### §2 — MOVED to M102_006 (grant-gated mint + lease)

The integration-grant enforcement formerly specified here is carved out to `M102_006_P0_API_GRANT_GATED_MINT_LEASE.md` (P0 — ships ahead of this tail). Section numbering below is preserved so cross-references stay stable.

### §3 — `agentsfleet connector` CLI ops

`agentsfleet connector list`/`status` renders the live connector registry and existing status endpoints for GitHub, Slack, Zoho, Jira, and Linear. Human and JSON output distinguish `connected`, `not_connected`, `reconnect_required`, and platform `unconfigured`; status succeeds even when disconnected and includes the next action. Transport/API failure emits structured JSON and exits non-zero. The CLI never invents state.

- **Dimension 3.1** — list/status reflects every live registry provider with stable human and JSON shapes → Test `test_cli_connector_status`
- **Dimension 3.2** — disconnected state returns a reconnect hint without failing the successful status read; daemon/API failure returns structured JSON and exits non-zero → Test `test_cli_connector_status_errors`

### §4 — Docs sweep (absorb C1–C9)

Reconcile both documentation homes with the completed flow. The App path replaces manual registration as the default, while the manual per-fleet route remains documented as a custom fallback. The docs distinguish the platform private key, platform webhook secret, workspace installation handle, reverse routing row, per-fleet repository/event subscription, approved grant, and short-lived token. `github-pr-reviewer` remains explicitly unproven until its repository-bound Pull Request integration test passes.

- **Dimension 4.1** — no architecture doc still asserts the user-`gh api …/hooks` manual registration for the App path → Test `test_docs_no_manual_gh_hook_for_app` (grep-based)
- **Dimension 4.2** — internal docs and the GitHub App playbook carry the full admin → workspace → installation → repository → fleet → event → mint flow, and `github-pr-reviewer` names its test state truthfully → Test `test_docs_github_app_flow_and_reviewer_state`
- **Dimension 4.3** — operator docs explain connector status, explicit repository binding, manual fallback, and platform App identity versus a user Personal Access Token (PAT) → Test `test_docs_repo_github_connector_truth`

## Interfaces

```
Generic App-webhook ingress (bearer-less; provider signature is the trust anchor):
  POST /v1/ingress/{provider}   # provider="github"; one platform App secret; payload.installation.id
      -> verify/router registry (one descriptor per provider; extends webhook_verify.PROVIDER_REGISTRY)
      -> verify signature -> installation -> workspace
      -> repository + event + approved grant -> matching fleet(s)
      -> per-delivery/per-fleet dedup -> XADD fleet:{id}:events
      | rejected{ bad_sig | unmapped_installation | no_matching_subscription }
  # the per-provider slack_events route (/v1/connectors/slack/events) is unchanged; its migration onto
  #   the generic ingress is Out of Scope.

Connector CLI (structured JSON on --json / when piped; non-zero only when the status request fails):
  agentsfleet connector list            -> [{ provider, state: connected|not_connected|reconnect_required|unconfigured, hint? }]
  agentsfleet connector status github   -> { provider, state, hint? }
  # disconnected -> state + hint, exit 0; request failure -> structured error, exit != 0
```

Ingress results are tagged unions; the route path, provider ids, and `installation.id` routing path are named constants shared verbatim with tests (RULE UFS). No existing endpoint shape is repurposed.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Ingress bad signature | tampered/unsigned App webhook payload | typed `rejected{bad_sig}` (`UZ-WH-*`); no `XADD`; logged, no fleet woken |
| Ingress unmapped installation | `installation_id` maps to no workspace (uninstalled/never-connected) | typed `rejected{unmapped_installation}`; no `XADD`; no leak of which ids exist |
| Ingress unbound repository | no active, granted fleet explicitly subscribes to repository + event | acknowledge with zero `XADD`; no broad workspace fan-out |
| Partial fan-out failure | one fleet enqueue fails after another succeeds | release only failed fleet's dedup slot; provider retry completes missing leg without duplicating the successful leg |
| Ingress new provider added | a descriptor added for a provider with a known signature family | routes with zero `/v1/ingress` handler edits (`test_ingress_registry_is_data_driven`) |
| CLI reads a disconnected connector | connector status is `not_connected` or `reconnect_required` | successful state response with a connect/reconnect hint; no fabricated failure |
| CLI daemon unreachable | control-plane down during `connector list` | structured-JSON transport error + non-zero exit; no partial/fabricated state |
| Token leak to logs | logging an ingress payload or connector state | never logged or returned (VLT); only non-secret provider/state/installation-presence appears |

## Invariants

1. **Adding an ingress provider adds no branch to the `/v1/ingress` dispatch** — the provider registry is data; `test_ingress_registry_is_data_driven` proves a fake descriptor routes with no handler edit; the compile-time dup guard in `PROVIDER_REGISTRY` holds.
2. **The ingress verifies the signature before any routing or side-effect** — verification precedes the routing-key read and the `XADD`; a bad signature produces zero `XADD` (`test_ingress_routes_installation_repository_event` negative leg).
3. **Ingress frames never carry a secret** — VLT; only provider/state/installation-presence appear in any log or response.
4. **App traffic never uses an implicit all-repositories subscription** — an explicit repository match and approved grant are required per fleet; the manual route remains fleet-addressed.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `ingress_rejected` (operator log) | ops | an inbound App webhook fails verify or maps to no workspace | provider, reason (bad_sig / unmapped), delivery id | no payload body, no signature bytes | `test_ingress_routes_by_installation_id` |

No product analytics or funnel change — this is operator/security-plane observability only; Discovery records "Metrics review: no analytics/funnel playbook update required" with that reason.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_ingress_routes_installation_repository_event` | callback stores both mappings; Pull Request and failed Actions events reach only exact repository/event/grant matches |
| 1.2 | integration | `test_ingress_registry_is_data_driven` | a fake provider descriptor verifies + routes with no `/v1/ingress` handler edit |
| 1.3 | integration | `test_ingress_repository_binding_and_replay` | missing repository binding is ignored; retry is once per fleet; manual GitHub + Slack regressions pass |
| 3.1 | e2e (cli) | `test_cli_connector_status` | `connector status --json` reflects live github state; shape stable |
| 3.2 | integration | `test_cli_connector_status_errors` | disconnected is a successful state with hint; transport/API error is structured and non-zero |
| 4.1 | unit (doc grep) | `test_docs_no_manual_gh_hook_for_app` | no arch doc asserts user-`gh api …/hooks` for the App path |
| 4.2 | unit (doc grep) | `test_docs_github_app_flow_and_reviewer_state` | internal docs + playbook cover the complete flow and reviewer test state |
| 4.3 | unit (doc grep) | `test_docs_repo_github_connector_truth` | operator docs cover repository binding, fallback, status, and credential ownership |

**Regression:** minting, static secrets, manual GitHub ingress, Slack ingress, and Jira/Linear credential behavior remain green. **Idempotency/replay:** callback reconnect upserts both mappings; App delivery dedup is per provider delivery and fleet; an enqueue failure releases only its own slot. **Integration coverage:** signed, tampered, unmapped, repository-miss, event-miss, grant-miss, multi-fleet, replay, partial failure, and CLI transport failures are deterministic.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | App webhook routes by installation + repository + event + grant; failures have no broad side effect (§1) | `make test-integration 2>&1 \| grep -E "ingress_routes_installation_repository_event\|ingress_repository_binding_and_replay"` | 2 pass lines | P1 | |
| R2 | Adding a provider needs no `/v1/ingress` handler edit (§1) | `make test-integration 2>&1 \| grep -E "ingress_registry_is_data_driven"` | pass line present | P1 | |
| R4 | `connector` CLI: registry-wide live state, reconnect hints, and structured request failures (§3) | `make test-unit-cli && make cli-acceptance` | exit 0 | P1 | |
| R5 | Internal + operator docs describe App routing, repository binding, fallback, and reviewer proof state (§4) | `make test 2>&1 \| grep -E "docs_no_manual_gh_hook_for_app\|docs_github_app_flow_and_reviewer_state\|docs_repo_github_connector_truth"` | 3 pass lines | P1 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks (allocator wiring touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — none deleted; this workstream is additive.**

**2. Orphaned references — grep after the changes; non-zero = stale.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| provider id literal | `grep -rn '"github"' src/agentsfleetd/http/handlers/webhooks --include='*.zig' \| grep -v const \| head` | only named-constant defs + imports (RULE UFS) |

## Out of Scope

- Migrating the existing provider-specific `slack_events` ingress (`/v1/connectors/slack/events`) onto the generic `/v1/ingress/{provider}` — a follow-up; §1 ships the generic route with the github descriptor and leaves slack's route untouched.
- New Slack, Zoho, Jira, or Linear event ingress. Their shipped connection and credential-mint behavior remains unchanged.
- Per-fleet cryptographic identity / Agent Auth wire-format alignment; Stripe Agentic Commerce Protocol; exact-action approval-hash binding — all v3 / named follow-ups per M102_001.
- Any mint-broker change. The GitHub callback change is narrowly approved to add the missing reverse mapping.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator connects GitHub and a teammate's Pull Request wakes the fleet: the App webhook lands (§1) and the operator can confirm "connected" from `agentsfleet connector status` (§3) — no silent drops (token gating ships first, in M102_006).
2. **Preserved user behaviour** — the shipped connect flow, the §1–§4 on-demand mint, static custom secrets, the existing `slack_events` ingress, and model routing all keep working.
3. **Optimal-way check** — the most direct finish: the ingress rides the existing verifier, the CLI is a renderer over the existing status endpoint. The gap to unconstrained-optimal (migrating slack onto the generic ingress, a fully registry-driven `PlatformSecrets`) is deferred — the abstraction lands, convergence later.
4. **Rebuild-vs-iterate** — iterate. Every surface exists; this wires the four deferred dimensions onto them. Verdict: targeted completion, not a refactor.
5. **What we build** — GitHub's two-row connect persistence, explicit repository subscriptions, generic App ingress, registry-wide connector status, and both documentation homes.
6. **What we do NOT build** — slack-route migration; new outbound connectors; per-fleet identity; any schema change (the grant table exists).
7. **Fit with existing features** — compounds with the shipped mint broker, the connect surface, and the approval inbox; must not destabilize the existing webhook verifier, the mint hot path, or static-secret resolution.
8. **Surface order** — canonical architecture docs first, then §1 ingress, §3 CLI, and finally operator documentation + changelog verified against the implementation.
9. **Dashboard restraint** — no new UI; the CLI reports only real connector state read from the live endpoint, never a fabricated "connected".
10. **Confused-user next step** — a fleet that can't reach GitHub sees a typed `reconnect_required` (or M102_006's `grant_required`) via the CLI structured error + suggestion, never a silent 401; the reconnect/grant path is the self-serve move.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three live Sections mapping to M102_001's deferred dimensions (§5.2/§5.4 → §1; §7 → §3; §8 → §4); the §6 grant enforcement (formerly §2 here) is carved out to M102_006 as its own P0 workstream and ships first.
- **Alternatives considered:** querying encrypted vault handles in reverse is invalid; broad workspace fan-out is unsafe; reusing Slack's specialized route would erase provider semantics. The chosen design uses the existing connector-install map, explicit repository subscriptions, and keeps Slack specialized.
- **Patch-vs-refactor verdict:** this is a **patch** (targeted completion) — it wires four deferred dimensions onto surfaces that already exist and introduces no new trust plane. The slack-ingress convergence and a registry-driven `PlatformSecrets` are the named follow-ups if the connector set grows.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
- **Origin (Indy + Orly, Jul 05, 2026):** carved from M102_001 on Indy's "Split + defer the tail" decision during post-merge cleanup. Live main-tree audit this session confirmed the split map: §1–§4 + §5-connect DONE; §5.2/§5.4 ingress, §6 grant enforcement, §7 CLI, §8 docs absent. The §6 finding (no grant read at mint or lease) drove the Jul 06, 2026 carve-out: that dimension now ships first as M102_006, per the approved incident-fleet office-hours design.
- **Jul 11, 2026 decisions:** Indy approved extending the GitHub callback to write `core.connector_installs`; required explicit repository binding for App traffic; requested the complete platform-admin-through-fleet walkthrough in `docs/architecture/**` and `~/Projects/docs/`; and required `github-pr-reviewer` to remain documented as unproven until its repository-bound integration test passes.
