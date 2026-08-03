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

# M157_001: A detected incident ends as one approved draft PR, opened by a fleet that could not have opened it unapproved

**Prototype:** v2.0.0
**Milestone:** M157
**Workstream:** 001
**Date:** Aug 01, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — customer/operator-facing; the product wedge and the Forge the Future 2026 hackathon entry are the same build
**Categories:** API, INFRA, OBS, SKILL
**Batch:** B1 — single workstream; Sections sequence by dependency
**Branch:** feat/m157-incident-draft-pr
**Test Baseline:** unit=3390 integration=539
**Depends on:** M156_001 (runner leases must complete on the restored dev fleet before any end-to-end proof here can run)
**Provenance:** LLM-drafted (Claude Opus 5, adversarially reviewed by Codex CLI 0.146.0, Aug 01, 2026; redesigned against Indy's restated use case Aug 03, 2026)
**Canonical architecture:** `docs/architecture/scenarios/production-deploy-repair.md` (this spec proves its 🔨 rows; its §6 statuses flip here)

---

## Overview

**Goal (testable):** A regression in a customer's instrumented workload is detected by a scheduled sweep over the customer's own Grafana, diagnosed with cited evidence, and — after exactly one human approval that names what it is approving — becomes exactly one draft Pull Request (PR) opened by a repairer fleet that holds no path to open it unapproved.

**Problem:** An operational incident today produces a diagnosis at best. Nothing owns the step from "we know the cause" to "a reviewable fix exists", so code-caused incidents fall into limbo between the on-call person who found the cause and the repository where the fix belongs. The architecture scenario documents this repair path and marks its write half unproven (🔨).

**Solution summary:** Two fleets and the approval gate that already ships. An **investigator** fleet wakes on a cron sweep, reads the customer's Grafana (Elastic second), correlates against repository history, and — when the cause is code-shaped and the repair is a revert of a suspect commit — ends its lease by messaging a **repairer** fleet. The investigator holds no GitHub credential, so it cannot write. The repairer's incoming event hits the approval gate, which binds *before a lease is issued* (`fleet/approval_gate.zig:1-7`): the event parks, Slack carries an approval naming the proposed action, its evidence, and its blast radius, and the repairer's lease is issued only on approval. The repairer then opens one draft PR through the GitHub HTTP API using a token minted for the declared repositories alone. The human gate is made true by removing `approval_resolve` from the tenant credential grant, so a machine can trigger a repair but only a human can approve one.

**Credentials (milestone gate — enumerate before any M157_002+):**

| Credential | Fetch location | Exists? |
|---|---|---|
| Grafana service-account token | plain workspace secret; demo Grafana provisioned by the §6 playbook | missing |
| Elastic Cloud endpoint + API key | plain workspace secret (second data plane, after Grafana) | missing |
| GitHub App (contents write, PR create, scoped per repository) | platform-configured installation | exists |
| Slack connector | existing workspace connector | exists |
| Model provider key for runs | existing tenant-provider mechanism (vault) | exists |
| Tenant API key for the investigator → repairer hop | dashboard `POST /v1/api-keys` | exists |
| AWS account for EC2 provisioning | 1Password `op://ops` (human-created) | missing |

Grafana, Elastic, and Fly are **plain workspace secrets**, not connectors — there is no `api_key` archetype (`docs/architecture/connectors.md` §Archetypes). Slack, Jira, and GitHub are the connectors. Onboarding is two shapes, not one.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m157): an incident becomes an approved draft PR, and only a human can approve it
- **Intent (one sentence):** An operator whose deploy regressed wakes up to a Slack diagnosis and an approval that says exactly which commit will be reverted in which repository — and clicking it produces one draft PR, where clicking nothing produces nothing.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/AUTH.md` §Scope catalogue + §Provisioning grants + §Flow 3 — the auth dispatch fires on §1; `scopes.zig`'s grant is the file being changed.
2. `src/agentsfleetd/fleet/approval_gate.zig` — the gate checks policy **before a lease is issued** and parks the event; `requestNewGate` is where `ActionDetail` is built and where §2 threads its blank fields.
3. `src/agentsfleetd/fleet_runtime/approval_gate.zig` — `GateDecision`, `GateStatus`, `requestApproval`, and the `.auto_approve` fallthrough at line 96 that makes gating opt-in.
4. `docs/architecture/capabilities.md` §1–§3 — `TRIGGER.md` policy shape, the tool bridge (`${secrets.NAME.FIELD}` substitution, minted GitHub tokens), and the per-lease ExecutionPolicy.
5. `tests/fixtures/fleetbundle/zoho-sprint-daily-summarizer/` — the scheduled-bundle frontmatter shape (`x-agentsfleet: triggers/tools/credentials/network/budget`) the §4 bundles mirror.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/auth/scopes.zig` | EDIT | Split `DefaultGrant`: a machine-credential variant carrying every tenant capability except `approval_resolve`; `.tenant` stays as-is for the human signup claim |
| `src/agentsfleetd/auth/middleware/tenant_api_key.zig` | EDIT | Construct the `agt_t` principal from the machine grant instead of `.tenant` |
| `src/agentsfleetd/auth/scopes_test.zig` | EDIT | The two grants differ in exactly one member; the human claim is unchanged |
| `src/agentsfleetd/http/handlers/approvals/resolve_integration_test.zig` | CREATE | A tenant API key is refused at the resolve route; a user principal is not |
| `src/agentsfleetd/fleet/approval_gate.zig` | EDIT | Thread `gate_kind` / `proposed_action` / `evidence` / `blast_radius` into `ActionDetail` from the triggering event |
| `src/agentsfleetd/fleet/approval_gate_integration_test.zig` | CREATE | The parked gate carries a populated detail; the Slack message names the action |
| `src/agentsfleetd/credentials/integration_github.zig` | EDIT | Mint body carries `repositories` + `permissions` instead of `""` |
| `src/agentsfleetd/fleet/repair_proposal.zig` | DELETE | Superseded — approval binds a bounded run, not bytes; no daemon apply exists to re-validate against |
| `src/agentsfleetd/fleet/repair_proposal_test.zig` | DELETE | Tests of the deleted kernel |
| `src/agentsfleetd/fleet/repair_bounds.zig` | DELETE | Apply-time diff bounds with no apply site |
| `src/agentsfleetd/fleet/repair_bounds_test.zig` | DELETE | Tests of the deleted kernel |
| `src/agentsfleetd/tests.zig` | EDIT | Drop the two deleted module registrations |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | Retire `UZ-REPAIR-001..005` — every one names an apply-service failure that no longer has a site |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | Retire the `UZ-REPAIR-*` constants |
| `src/agentsfleetd/errors/gen_error_codes.zig` | EDIT | Retire the REPAIR category copy (its comptime coverage gate pairs with the family) |
| `library/incident-responder/SKILL.md` | EDIT | Becomes the investigator: read-only, no repair authorship, ends by naming a repair intent |
| `library/incident-responder/TRIGGER.md` | EDIT | Drop the `github` credential and `api.github.com` — the investigator must not be able to write |
| `library/incident-repairer/SKILL.md` | CREATE | The revert rung: given a suspect commit, open one draft revert PR; nothing else |
| `library/incident-repairer/TRIGGER.md` | CREATE | `api` trigger, `http_request` only, `github` credential, `api.github.com` allowlist, **non-empty `gates.rules`** |
| `library/incident-repairer/bundle_gate_test.zig` | CREATE | The shipped repairer bundle carries a gate rule — an omitted rule auto-approves |
| `docs/AUTH.md` | EDIT | Record that machine credentials cannot resolve approvals, and why |
| `docs/architecture/scenarios/production-deploy-repair.md` | EDIT | Describe the gate-bound two-fleet design; flip proven rows |
| `bench/incident-response/` | KEEP | §5 landed and is frozen; its rubric rows are rewritten to claim only detection |
| `playbooks/demo/forge-2026/` | CREATE | EC2 + collector + Grafana bring-up, failure injection, replay proof |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — NDC (no dead code at write time — the `UZ-REPAIR-*` family and the repair kernel go out in this diff, not later), NLR (touch-it-fix-it), NLG (no legacy framing pre-2.0.0 — the scenario doc describes the gate-bound design as *the* design), UFS (gate status strings, bundle names shared verbatim across surfaces), ORP (orphan sweep), FLL (length caps).
- `~/Projects/dotfiles/dispatch/write_auth.md` → **`docs/AUTH.md` before any edit to `auth/scopes.zig`** — §1 is a scope-grant change and carries the AUTH review profile.
- `~/Projects/dotfiles/dispatch/write_zig.md` — pg-drain, tagged-union results, errdefer, cross-compile; all new daemon code.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` + `~/Projects/dotfiles/docs/LIFECYCLE_PATTERNS.md` — structured events, init/deinit lifecycles; `~/Projects/dotfiles/dispatch/write_shell.md` — playbook scripts, and vault-reading playbook scripts pass both vault gates (`check-vault-gate-parity`).
- Schema conventions: **not applicable** — this workstream adds no table and no migration, so no `schema/embed.zig` slot is claimed and no collision with M154's rebuild exists.
- REST guidelines: not applicable — no new public HTTP route; approval rides existing gate surfaces.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | cross-compile both linux targets; linux test graph clean |
| PUB / Struct-Shape | yes | FILE SHAPE DECISION for the edited `ActionDetail` surface |
| File & Function Length | yes | detail threading stays inside `requestNewGate`'s budget or splits |
| UFS | yes | gate kinds, bundle names, mint permission keys as named constants |
| LOGGING / LIFECYCLE / ERROR REGISTRY | yes | structured events; the retired `UZ-REPAIR-*` family leaves the registry coverage gate green |
| SCHEMA GUARD | no | no schema file, no migration array edit |
| UI Substitution / DESIGN TOKEN | no | no UI files in scope |

## Prior-Art / Reference Implementations

- **Reference:** `tests/fixtures/fleetbundle/{platform-ops,github-pr-reviewer,zoho-sprint-daily-summarizer}` — bundle prose style, constraint framing, scheduled frontmatter; the §4 bundles are their production-grade siblings.
- **Reference:** `docs/architecture/connectors.md` §GitHub App + §Bounded outbound — daemon-side token minting and armed vendor calls; the mint narrowing extends this, never re-invents it.
- **Reference:** `src/agentsfleetd/fleet_runtime/approval_gate_async.zig` — the re-poll shape that lets a parked event resume without a blocked thread; §2 adds no parallel machinery.
- **Benchmark:** `bench/incident-response/` — landed in this workstream; measures detection only.

## Sections (implementation slices)

### §1 — Machines trigger, humans approve

`grantMembers(.tenant)` closes `approval_resolve` into every `agt_t` key at comptime (`auth/scopes.zig:99-110`), and `core.api_keys` carries no per-key scope column. Any fleet holding a tenant key — including the investigator, which needs one to message the repairer — can therefore resolve the very gate that guards the repairer. The gate is not a human gate until that is false.

**The grant is shared, so it must be split rather than trimmed.** `grantMembers(.tenant)` feeds two consumers: `defaultScopes(.tenant)` at `auth/middleware/tenant_api_key.zig:119` (machine `agt_t` principals) **and** `defaultClaim(.tenant)` at `http/handlers/auth/identity_events_clerk.zig:41`, which becomes `DEFAULT_SIGNUP_SCOPES` written to Clerk `publicMetadata` at signup and read back as the human tenant owner's `scopes` claim. Deleting the scope from the shared list would strip approval rights from every new tenant owner. `DefaultGrant` therefore gains a third variant for the machine credential, carrying every tenant capability except `approval_resolve`; the human signup claim keeps `.tenant` unchanged.

The two consumers also differ in when the change bites, and the spec relies on both behaviours. `defaultScopes` is evaluated at principal construction on every request, so **existing `agt_t` keys lose the scope immediately with no backfill**. `defaultClaim` is persisted per-user at signup, so existing owners keep what was already written and only new signups read the current constant — which is why the human list must not change at all.

- **Dimension 1.1** — **DONE** — The machine grant carries every tenant capability except `approval_resolve` → Test `test_machine_grant_excludes_approval_resolve`
- **Dimension 1.2** — **DONE** — The human signup claim still carries `approval:resolve`; the two grants differ in exactly that one granted member → Test `test_signup_claim_retains_approval_resolve`
- **Dimension 1.3** — A tenant API key is refused at the approval-resolve route; a user principal still passes → Test `test_api_key_cannot_resolve_approval`
- **Dimension 1.4** — Blast radius is empty: no in-repo caller resolves an approval with a machine credential → Test `test_no_machine_approval_callers` (repo grep, asserted)

### §2 — The approval names what it is approving, and the token reaches one repository

`requestNewGate` builds `ActionDetail` with `gate_kind`, `proposed_action`, `evidence`, and `blast_radius` left blank — the code comment records them as designed-but-unthreaded. A human approving a repair sees none of it. This Section threads them from the triggering event so the Slack approval states the repository, the suspect commit, the evidence that implicated it, and that the outcome is one draft PR. Separately, `integration_github.zig` mints installation tokens with `.body = ""`, which yields the App's full permissions across **every** repository in the installation; the mint body carries `repositories` and `permissions` derived from the fleet's declared binding instead.

- **Dimension 2.1** — A parked gate carries a populated `proposed_action`, `evidence`, and `blast_radius` → Test `test_gate_detail_is_populated`
- **Dimension 2.2** — The Slack approval message names repository, commit, and outcome → Test `test_slack_approval_names_the_action`
- **Dimension 2.3** — The mint request body pins `repositories` to the fleet's binding and `permissions` to contents + pull-requests → Test `test_mint_body_is_repository_scoped`
- **Dimension 2.4** — A fleet with no declared repository binding gets no mintable GitHub token (fail closed) → Test `test_unbound_fleet_mints_nothing`

### §3 — The write lives behind the gate, structurally

The investigator holds no GitHub credential and no `api.github.com` allowlist entry, so its inability to open a PR is a property of its policy rather than of its prompt. The repairer's only trigger is the `api` event the investigator sends, and its bundle declares a non-empty `gates.rules` — because `approval_gate.zig:96` falls through to `.auto_approve` when nothing matches, an omitted rule silently yields an autonomous agent holding a write token. Approval authorises **one bounded repairer run**, not specific bytes; the draft PR is the review surface where the diff is read.

- **Dimension 3.1** — The investigator bundle declares no GitHub credential and no GitHub host → Test `test_investigator_cannot_reach_github`
- **Dimension 3.2** — The shipped repairer bundle declares a non-empty gate rule → Test `test_repairer_bundle_declares_a_gate`
- **Dimension 3.3** — A repairer event without an approved gate yields no lease and no PR → Test `test_unapproved_event_opens_no_pr`
- **Dimension 3.4** — Denial and deadline expiry resolve terminally; the repairer never runs → Test `test_denied_or_timed_out_never_runs`

### §4 — The crew investigates, diagnoses, and proposes exactly one repair class

`library/incident-responder/` (investigator) wakes on a cron sweep, queries the customer's Grafana, correlates with recent repository history, posts a diagnosis to Slack, and — only when the cause is code-shaped and the repair is a revert of an identified commit — messages the repairer with repository, commit, and evidence. `library/incident-repairer/` opens one draft revert PR through the GitHub HTTP API and does nothing else: the reverted-to code was already green in Continuous Integration (CI), so no model authors any line of the change. Config-in-repo diffs and narrow patches are later rungs. Truth living only in a vendor console is recommended with a link, never written.

- **Dimension 4.1** — A seeded regression yields a structured finding citing a real Grafana response digest, never an invented identifier → Test `eval_detection_cites_evidence`
- **Dimension 4.2** — The finding names the failing service and the correlated commit range → Test `eval_finding_names_service_and_commit`
- **Dimension 4.3** — Provider-outage and data-shaped incidents stay diagnosis-only: no repair intent sent → Test `eval_noncode_incidents_stay_diagnosis_only`
- **Dimension 4.4** — The repairer's PR is a revert of the named commit and touches nothing else → Test `eval_repair_is_a_revert`
- **Dimension 4.5** — Cold install of both bundles onto a fresh workspace succeeds with declared credentials and hosts → Test `test_cold_install_from_library`

### §5 — Data-plane credentials and library publication use only existing mechanisms

Grafana and Elastic keys are plain workspace secrets (never registry entries, per `connectors.md`), reaching the run only as `${secrets.NAME.FIELD}` placeholders substituted at the tool bridge. `ctx.policy` is read by `buildHttpRequest` and no other builder (`tool_builders.zig:183`), so substitution and `network.allow` bind `http_request` alone — both bundles are `http_request`-only and no runner checkout is built. Bundles publish through the existing admin library flow (`draft` → `public`, content-addressed snapshot).

- **Dimension 5.1** — Grafana/Elastic secrets stay placeholders in prompt and logs; raw bytes appear only in the egress request → Test `test_data_plane_secrets_stay_placeholders`
- **Dimension 5.2** — A host outside a bundle's allowlist is refused for that bundle's leases → Test `test_undeclared_host_refused`
- **Dimension 5.3** — Onboard → publish → workspace-visible → installable, via the existing admin flow → Test `test_bundles_publish_and_list`

### §6 — The benchmark is honest by construction

`bench/incident-response/` seeds an instrumented corpus and injects incidents from seed manifests split into disjoint calibration and evaluation sets. The threshold baseline is tuned on calibration only, then frozen by config hash. Detection scores only when a structured result names the affected service and incident class within tolerance — "anomaly found" scores zero. **This harness measures detection over a synthetic corpus; it does not exercise the crew, the gate, or the write path, and no rubric row claims that it does.**

- **Dimension 6.1** — **DONE** — The injector is reproducible: identical seed manifest → identical corpus hash → Test `test_injector_deterministic`
- **Dimension 6.2** — **DONE** — Calibration and evaluation manifests are disjoint; scoring refuses a mixed corpus → Test `test_seed_manifests_disjoint`
- **Dimension 6.3** — **DONE** — The baseline is frozen: scoring refuses a baseline whose config hash drifted after calibration → Test `test_baseline_frozen`
- **Dimension 6.4** — **DONE** — Scoring requires service + class within tolerance; unstructured claims score zero → Test `test_scoring_requires_service_and_class`
- **Dimension 6.5** — **DONE** — The report emits the full metric set, including variance, cost, and threshold-win cases → Test `test_report_metrics_complete`

### §7 — The demo topology runs on AWS and the stage proof is replay-safe

A playbook stands up a small multi-service instrumented workload on EC2, Grafana receiving its telemetry, an `agentsfleet-runner` host, failure-injection scripts, and both fleets installed by hand. The stage proof: inject a held-out regression live, watch detection → diagnosis → Slack approval naming the commit → one draft revert PR, then replay the same investigator message and show the second run parks on its own approval rather than opening a second PR.

- **Dimension 7.1** — The playbook's check mode is idempotent: two consecutive runs both exit clean → Test `playbook_check_idempotent`
- **Dimension 7.2** — An injected failure traverses the collector → Grafana and is detected end-to-end on the live stack → Test `e2e_injected_failure_detected`
- **Dimension 7.3** — Approving once produces exactly one draft PR on the live stack → Test `e2e_single_pr_on_approval`
- **Dimension 7.4** — A replayed investigator message parks a second gate and opens no second PR without a second approval → Test `e2e_replay_parks_not_writes`

## Interfaces

```
Scope grant (auth/scopes.zig): DefaultGrant gains a machine-credential variant whose
  members are the .tenant set minus .approval_resolve. defaultScopes(<machine>) feeds
  tenant_api_key.zig; defaultClaim(.tenant) still feeds identity_events_clerk.zig's
  DEFAULT_SIGNUP_SCOPES unchanged. The scope itself is untouched — only which
  credential source is granted it.
ActionDetail (fleet_runtime/approval_gate.zig), populated by fleet/approval_gate.zig:
  { tool, action, params_summary, gate_kind, proposed_action, evidence,
    blast_radius, timeout_ms } — proposed_action names repo + commit + "one draft PR";
  evidence carries the Grafana query reference and commit range; blast_radius names
  the repository. Never diff bytes, never secret material.
GitHub mint (credentials/integration_github.zig): POST /app/installations/{id}/access_tokens
  body { repositories: [<fleet binding>], permissions: { contents: "write",
  pull_requests: "write" } } — absent binding → no mint (fail closed).
Investigator → repairer edge: POST /v1/workspaces/{ws}/fleets/{repairer}/messages with a
  tenant API key (scope fleet:write). The message carries repository, suspect commit,
  and evidence links — 8 KiB cap, so identifiers and links only, never file contents.
Repair rung: revert only. The repairer resolves the suspect commit and opens a draft PR
  reverting it. No model-authored source lines exist in the diff.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Machine attempts approval | A fleet or external service resolves a gate with a tenant key | Route refuses on scope; structured log; the gate stays pending until a human decides |
| Repairer bundle without a gate rule | Hand-install omits `gates.rules` | Bundle test fails at build; on a live workspace the omission is visible as an auto-approved run in the activity stream |
| Denied / timed out | Human denies, or the gate deadline passes | Terminal status; the repairer's lease is never issued; diagnosis artifacts remain |
| Unbound repository | Repairer fleet declares no repository binding | Mint refuses; the run reports it could not authenticate rather than reaching a wrong repository |
| Duplicate investigator message | Retried or double-delivered steer | Each message parks its own gate; a second PR requires a second human approval. No caller idempotency key exists — the gate is the bound |
| Data plane unreachable or secret missing | Grafana down mid-sweep, or a declared credential absent | Finding degrades honestly (names what it could not read); no repair intent sent; existing stop-the-tool-call codes in the activity stream |
| Upstream write failure | GitHub rejects branch or PR creation | The repairer's tool call fails with the vendor's response class; the run reports it; nothing partial is claimed as done |
| Seed drift | Benchmark run over a corpus whose hash mismatches the manifest | Harness refuses to score; names both hashes |

## Invariants

1. No machine credential can resolve an approval, and no human loses the ability to — the machine grant excludes `approval_resolve` while the signup claim retains it, asserted by a set-difference unit test and by a route-level integration test.
2. The investigator cannot write to GitHub — its bundle declares no GitHub credential and no GitHub host, and `network.allow` is the authoritative gate (`PolicyHttpRequestTool.hostInAllowlist`).
3. No repairer lease is issued for an event whose gate is not approved — the existing pre-lease check is the only path, and this workstream adds no bypass.
4. Every parked approval names its proposed action, evidence, and blast radius — a blank `ActionDetail` field is a test failure, not a display default.
5. A minted GitHub token reaches only the repositories the fleet declared — the mint body pins them, and an unbound fleet mints nothing.
6. Raw secret bytes never appear in prompt, result, or logs — existing tool-bridge substitution re-asserted by test for the new credential names.
7. Benchmark evaluation incidents never inform tuning — calibration and evaluation manifests are disjoint by construction and the scorer enforces it.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `repair_intent_sent` | ops | Investigator messages the repairer | investigator fleet id, repo, commit, evidence kinds | no secrets, no file contents | `eval_finding_names_service_and_commit` |
| `repair_approval_requested` | ops | Gate parks a repairer event | gate action id, repo, commit | detail fields only, no payloads | `test_gate_detail_is_populated` |
| `repair_approval_resolved` | ops | Gate resolves approve/deny/timeout | action id, resolution, actor kind | no actor PII beyond existing gate fields | `test_denied_or_timed_out_never_runs` |
| `repair_pr_opened` | ops | Repairer opens the draft PR | repo, pr url, reverted commit | no diff bytes | `e2e_single_pr_on_approval` |
| `machine_approval_refused` | ops | A machine credential is refused at the resolve route | principal mode, route | no key material | `test_api_key_cannot_resolve_approval` |
| `benchmark_run_completed` | ops | Harness finishes a scored run | corpus hash, metric summary | aggregate numbers only | `test_report_metrics_complete` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_machine_grant_excludes_approval_resolve` | machine-grant set lacks `approval_resolve`, retains the other ten tenant capabilities |
| 1.2 | unit | `test_signup_claim_retains_approval_resolve` | `defaultClaim(.tenant)` still contains `approval:resolve`; set difference against the machine grant is exactly that one member |
| 1.3 | integration | `test_api_key_cannot_resolve_approval` | `agt_t` bearer at resolve route → refused; user JWT → accepted |
| 1.4 | unit | `test_no_machine_approval_callers` | repo grep for machine-credential approval resolution → zero hits |
| 2.1 | integration | `test_gate_detail_is_populated` | parked gate row carries non-empty proposed_action/evidence/blast_radius |
| 2.2 | unit | `test_slack_approval_names_the_action` | built message contains repo, commit sha, and the draft-PR outcome string |
| 2.3 | unit | `test_mint_body_is_repository_scoped` | mint request body JSON carries the declared repo and both permission keys |
| 2.4 | unit | `test_unbound_fleet_mints_nothing` | fleet with null repositories → mint refused, no token returned |
| 3.1 | integration | `test_investigator_cannot_reach_github` | investigator policy + api.github.com → tool call refused |
| 3.2 | unit | `test_repairer_bundle_declares_a_gate` | shipped TRIGGER.md parses to a non-empty `gates.rules` |
| 3.3 | integration | `test_unapproved_event_opens_no_pr` | repairer event, gate pending → no lease issued, fake GitHub sees zero calls |
| 3.4 | integration | `test_denied_or_timed_out_never_runs` | deny and deadline expiry → terminal, repairer lease never issued |
| 4.1 | eval | `eval_detection_cites_evidence` | seeded regression → finding cites a returned Grafana digest |
| 4.2 | eval | `eval_finding_names_service_and_commit` | traced failure → service + commit range named |
| 4.3 | eval | `eval_noncode_incidents_stay_diagnosis_only` | provider-outage seed → no repair intent message sent |
| 4.4 | eval | `eval_repair_is_a_revert` | repairer output diff equals the revert of the named commit |
| 4.5 | e2e | `test_cold_install_from_library` | fresh workspace + published entries → both installed, scheduled, policy attached |
| 5.1 | integration | `test_data_plane_secrets_stay_placeholders` | prompt/log capture free of raw bytes for grafana/elastic names |
| 5.2 | integration | `test_undeclared_host_refused` | non-allowlisted host → tool call refused with existing code |
| 5.3 | integration | `test_bundles_publish_and_list` | onboard → draft → public → visible + installable |
| 6.1 | unit | `test_injector_deterministic` | same manifest twice → identical corpus hash |
| 6.2 | unit | `test_seed_manifests_disjoint` | overlapping ids → scorer refuses |
| 6.3 | unit | `test_baseline_frozen` | baseline hash drift post-calibration → scorer refuses |
| 6.4 | unit | `test_scoring_requires_service_and_class` | unstructured "anomaly" claim → score 0 |
| 6.5 | unit | `test_report_metrics_complete` | report contains every §6 metric incl. threshold-win cases |
| 7.1 | e2e | `playbook_check_idempotent` | check mode twice → both exit 0 |
| 7.2 | e2e | `e2e_injected_failure_detected` | live injected failure → detection event within sweep bound |
| 7.3 | e2e | `e2e_single_pr_on_approval` | one approval → exactly one draft PR |
| 7.4 | e2e | `e2e_replay_parks_not_writes` | replayed message → second gate pending, PR count stays 1 |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A machine credential cannot approve, and the human signup claim still can (§1) | `make test-integration TEST_FILTER='approval'` then `make test-unit-all TEST_FILTER='grant'` | exit 0 both; `test_api_key_cannot_resolve_approval` and `test_signup_claim_retains_approval_resolve` listed as pass | P0 | |
| R2 | The approval names its action and the token is repository-scoped (§2) | `make test-unit-all TEST_FILTER='gate_detail\|mint_body'` | exit 0, both tests listed by name | P0 | |
| R3 | No unapproved event produces a write (§3) | `make test-integration TEST_FILTER='repairer'` | exit 0, `test_unapproved_event_opens_no_pr` listed as pass | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R5 | Cold install of both published bundles on a fresh workspace (§4–§5) | `make test-integration TEST_FILTER='cold_install'` | exit 0 | P0 | |
| R6 | Detection benchmark is reproducible — **detection only, not the crew** (§6) | `make bench-incident SEED_MANIFEST=eval` run twice | identical corpus hash line both runs | P1 | |
| R7 | The retired repair kernel leaves no orphan | `rg -n 'repair_proposal\|repair_bounds\|UZ-REPAIR' src/ \|\| true` | zero hits | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

**Test Delta note (VERIFY):** this diff **deletes 13 registered tests** with the repair kernel. The Test Delta row is measured against the baseline *after* subtracting those; a flat or negative raw count is expected and is not, on its own, grounds to return to EXECUTE. The added tests in §1–§4 must exceed 13 for the delta to be genuinely positive.

### Behaviour evals

- **Grounding rule:** every finding cites only identifiers and digests actually returned by Grafana or the repository — a fabricated identifier is a P0 ❌.
- **Golden set:** `bench/incident-response/seeds/` — 12 evaluation incidents + 12 clean windows across four classes (obvious spike, slow burn, cross-service failure, deployment-correlated regression — the nightmare case) plus non-code seeds for 4.3.
- **Ship threshold:** grounding 100% · structured detection pass ≥ 75% on the evaluation set · 0 critical failures on the deployment-correlated regression class.
- **Fallback:** low confidence or unreadable data plane → diagnosis-only (named degradation, no repair intent); fabricated output is a P0 ❌.

## Dead Code Sweep

Four files and one error family leave in this diff, all of them substrate for the daemon-side apply that no longer exists:

| Path | Lines | Why it goes |
|---|---|---|
| `src/agentsfleetd/fleet/repair_proposal.zig` | 295 | Canonical hashing bound approved bytes to applied bytes; no daemon apply site remains to compare at |
| `src/agentsfleetd/fleet/repair_proposal_test.zig` | 280 | Tests of the above |
| `src/agentsfleetd/fleet/repair_bounds.zig` | 131 | Apply-time diff-path bounds; the repairer writes through the vendor API, so no diff is inspectable before the write |
| `src/agentsfleetd/fleet/repair_bounds_test.zig` | 114 | Tests of the above |
| `UZ-REPAIR-001..005` | — | Every code names an apply-service failure (stale base, bounds, duplicate, upstream, malformed proposal) with no site left to raise it |

Removal is verified by rubric R7 rather than by inspection.

## Out of Scope

- **Fleet identity and provenance — M157_002.** First-class `agt_a` fleet keys (`AuthMode.fleet_key` + middleware branch, per `docs/AUTH.md:362`), `actor=chain:<fleet_id>` on machine hops, a hop cap, and a caller idempotency key on `POST /messages`. None is a prerequisite for this workstream's guarantee; each is independently valuable.
- Repair rungs beyond revert — config-in-repo diffs and narrow patches need their own bounds story and their own spec.
- Chat-to-fleets authoring of a crew — prove the shape by hand first.
- Automatic merge, deploy, rollback, or any write beyond the one draft PR.
- Elastic as a data plane — Grafana is first; Elastic follows once the loop is proven.
- Jira post-mortem tickets — optional and later.
- Website retheme around this wedge — separate spec after the loop is proven live.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator opens Slack to a diagnosis naming the failing service and the suspect commit, and an approval that says "revert `abc123` in `owner/repo`, opens one draft Pull Request". They click Approve and GitHub shows exactly that.
2. **Preserved user behaviour** — Existing fleets, triggers, approvals, and the platform-ops diagnosis flow are untouched; a workspace that installs neither bundle sees one change only: a tenant API key can no longer resolve an approval.
3. **Optimal-way check** — Direct: the loop reuses the gate, the mint, the tool bridge, the library, and the message edge. The three code changes are each a single-purpose correction to a mechanism that already exists. Gap to optimal: approval authorises a bounded *run*, not specific bytes, so the human approves an intent and reviews the result on the draft PR. That is the honest shape of a gate that binds before a lease.
4. **Rebuild-vs-iterate** — Iterate; every needed substrate exists. The one thing that was genuinely missing — a reason the investigator *cannot* write — comes from credential separation across two fleets rather than from new code.
5. **What we build** — One scope-grant correction, one approval-detail threading, one mint narrowing, two bundles, a demo playbook, and the scenario-doc flip.
6. **What we do NOT build** — Everything in Out of Scope; notably no daemon-side apply service, no proposal record, and no second approval mechanism. **The repairer is a model run holding a write credential, deliberately.** That is bounded by three things and no others: it cannot run without an approved gate, its token reaches only the declared repositories, and its only rung is a revert of code CI already passed.
7. **Fit with existing features** — Compounds with approvals, connectors, library, schedules, and the event log. Must not destabilize: the approval gate — a populated `ActionDetail` must never change gate outcomes, only what the human reads.
8. **Surface order** — Runtime + bundles first; the dashboard shows nothing new beyond a richer gate card. Command-line and web surfaces unchanged.
9. **Dashboard restraint** — No new approval UI: the existing gate card carries the newly-populated detail fields; the diff is reviewed on the draft PR, where code review already lives.
10. **Confused-user next step** — A refused machine approval names the scope it lacked; a degraded sweep says what it could not read; a repairer that cannot mint says which repository binding is missing.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven Sections in one Workstream: the human gate made true (§1) before the approval is made legible and the token bounded (§2), before the structural separation that relies on both (§3); the bundles (§4), publication (§5), benchmark (§6), and topology (§7) prove it. One PR carries the provable loop.
- **Alternatives considered:** (a) **Daemon-side deterministic apply with a content-addressed proposal record** — the original §1/§2, partially built and removed here. It offers a strictly stronger guarantee (approved bytes are shipped bytes) but requires a proposal table, a store, an apply service, and a report-path hook, and it does not match the crew shape the product needs. Superseded, not refuted. (b) **A method-and-path allowlist at the egress boundary** (`PolicyHttpRequestTool`) to bound what the repairer may call — rejected as unnecessary once credential separation puts the write in a different fleet; branch and tree endpoints permit arbitrary branch content anyway, so the bound it buys is "cannot land", which the gate already provides. (c) **Routing the investigator → repairer hop through Slack** — rejected: `slack/events.zig` resolves `(team, channel)` to the channel's *resident* fleet, so the repairer would wake on every human message in the channel, handing a prompt-injection path to the fleet holding the write token. (d) **Collapsing investigator and repairer into one fleet** — rejected: one fleet holds one credential set across both leases, so "read-only first, write second" would be prompt-shaped rather than structural.
- **Patch-vs-refactor verdict:** this is a **patch** — three small corrections to shipped mechanisms plus two markdown bundles. Solution-size matches problem-size, and the deletion of the apply substrate makes the diff net-simpler.

## Discovery (consult log)

- **Consults** — Architecture: `scenarios/production-deploy-repair.md` is rewritten in the same diff to describe the gate-bound two-fleet design (`name_architecture` satisfied). Adversarial review: Codex CLI session (Aug 01, 2026) — verdict GO on the problem pick; its finding that the approval gate binds at lease rather than at tool time is now the load-bearing design fact rather than a caveat.
  - > Indy (2026-08-01): "How about we focus on the logs fo grafana, elastic and git to push a PR? since that is my need now as well?" — context: choosing the wedge this spec implements.
  - > Indy (2026-08-01): "yes pressure test the pick with codex" → "go" — context: adversarial review accepted; spec authoring authorized.
- **Metrics review** — six operator events (table above); no product-analytics funnel touched.
- **Skill-chain outcomes** — empty at reconciliation; populated at VERIFY/REVIEW/CHORE(close).
- **Deferrals** — M157_002 (fleet identity and provenance), Indy-acked below.

### Aug 03, 2026 design session — the use case restated, and the shape redesigned

Design-only session. Four reviews ran: `plan-ceo-review`, `plan-eng-review`, an independent
adversarial subagent, and Codex CLI 0.146.0 (`gpt-5.6-sol`, reasoning effort xhigh).

**Indy restated the use case: autonomous observability patching.** The signals are the
**customer's**, in the customer's Grafana and Elastic, plus Fly.io logs — not agentsfleet's
own telemetry. A customer signs up, connects their tools, and a composite of fleets (a
"crew") watches their logs, senses an outage or anomaly, ascertains what class of repair
applies, and produces a reviewable Pull Request (PR). Grafana is the first source; Elastic
second. A Jira ticket carrying the post-mortem is optional and later.

**Settled by Indy (do not re-litigate):**

- Everything is a fleet in a sandbox. No new "broker" abstraction.
- Security surface of a fleet holding write credentials: **accepted**; see the correction
  below on how large that surface actually was.
- All fleets reach the language model; no per-member gating.
- `github-pr-reviewer` stays as the one GitHub-sourced template; other members are set up
  manually via Command-Line Interface (CLI) or API for now.
- Chat-to-fleets authoring is **later**. Prove the shape by hand first.
- No normalized schema for any crew artifact; a generated markdown document is acceptable.
- M154 is **not** a blocker for this work — confirmed again on Aug 03 after a false alarm
  from the reconciling agent; see the correction below.

**Verified in code — cite these rather than re-deriving:**

| Finding | Evidence |
|---|---|
| A tenant API key carries `approval_resolve`, `apikey_admin`, `secret_write`, `workspace_admin` from a **comptime-pinned** grant; `core.api_keys` has no scopes column. **A fleet holding a tenant key can resolve its own approval.** §1 fixes this by splitting the grant. | `auth/scopes.zig:99-110`, `auth/middleware/tenant_api_key.zig:119` |
| `grantMembers(.tenant)` has **two** consumers, not one: `defaultScopes` → machine `agt_t` principals, and `defaultClaim` → `DEFAULT_SIGNUP_SCOPES` written to Clerk `publicMetadata` at signup, which becomes the human tenant owner's `scopes` claim. Trimming the shared list would have stripped approval rights from every new tenant owner. The reconciling agent proposed exactly that before checking the second consumer. | `auth/scopes.zig:118-125`, `http/handlers/auth/identity_events_clerk.zig:41` |
| The GitHub App installation token is minted with `.body = ""` — **no `repositories`, no `permissions`** — so any fleet declaring the `github` credential receives full App permissions across every repository in the installation for an hour. Found Aug 03; neither prior review considered it. §2 fixes it. | `credentials/integration_github.zig:72-77` |
| The approval gate checks policy **before a lease is issued** and parks the event; there is no mid-run hold. `ActionDetail`'s `gate_kind`/`proposed_action`/`evidence`/`blast_radius` are built blank, and the code comment records them as designed-but-unthreaded. §2 threads them. | `fleet/approval_gate.zig:1-7,112-127` |
| `GateDecision` falls through to `.auto_approve` when no rule matches, and `GatePolicy.rules` defaults to empty — **gating is opt-in per fleet**. A bundle shipped without `gates.rules` is an autonomous agent. §3 tests against it. | `fleet_runtime/approval_gate.zig:96`, `config_gates.zig:67-68` |
| `ctx.policy` is read by `buildHttpRequest` and **no other builder** — `${secrets.NAME.FIELD}` substitution and `network.allow` bind `http_request` only; `git`/`shell` get the placeholder literally and no egress allowlist. Write through the GitHub HTTP API; **no runner checkout is needed and none should be built.** | `src/runner/engine/tool_builders.zig:183` |
| Slack events resolve `(team, channel)` to the channel's **resident** fleet, materializing on miss — they do not route to an arbitrary fleet by trigger source. Routing the crew handoff through Slack would wake the repairer on every human message in the channel. | `http/handlers/connectors/slack/events.zig:26,177` |
| The runner deletes the per-lease workspace after every run, so fleets **cannot hand each other files**; messages cap at 8 KiB. Evidence travels as links and identifiers. | `src/runner/daemon/lease_run.zig:99`, `http/handlers/fleets/messages.zig:30` |
| `POST /v1/workspaces/{ws}/fleets/{id}/messages` (scope `fleet:write`) is the only fleet-to-fleet edge. It has **no caller idempotency key** and machine hops log `actor=steer:`, losing parent-fleet provenance. No hop bound exists. Deferred to M157_002; the gate bounds the consequence in the meantime. | `http/route_template.zig:75`, `http/route_scopes.zig:153` |
| `POST /fleet-libraries` accepts `source_kind:"upload"` with **inline** `skill_markdown` + `trigger_markdown`, content-addressed with `ON CONFLICT DO NOTHING`. Applying a crew therefore needs **zero** daemon code — it is a CLI loop over shipped endpoints. | `http/handlers/fleet_bundles/resolve.zig:81` |
| Only Slack, Jira, GitHub (plus Linear, Zoho) are connectors. **Grafana, Elastic, and Fly are plain workspace secrets** — no `api_key` archetype (dropped M108_002). Onboarding is two shapes. | `docs/architecture/connectors.md` §Archetypes |
| `agentsfleet connector` is read-only (`list`, `status`); every *connect* is a dashboard action. `agentsfleet fleet update <id> --from <path>` rewrites a live fleet's markdown — that is the hand-setup path. | `cli/src/commands/connector.ts:1`, `cli/src/program/cli-tree-fleet.ts:37-46` |
| `chain` is documented as a trigger type but rejected by the parser; `delegate`/`spawn` are registered but built inert. Trigger types are `webhook`, `cron`, `api`. | `capabilities.md:44,55` vs `fleet_runtime/config_types.zig:87` |

**Corrections recorded against this session's own output:**

- The reconciling agent initially reported M154 as *blocking* the credential work because
  `core.api_keys` is rebuilt on that branch. Indy pushed back and was right: a renamed
  schema file is a loud git conflict, not a blocker, and the design that landed needs **no
  schema change at all**, so the overlap is a single line in `auth/scopes.zig`.
- The same agent proposed routing the crew handoff through Slack and an egress
  method/path allowlist. Both were withdrawn under adversarial review — see Decomposition
  alternatives (b) and (c).
- Indy's Aug 02 acceptance of "a fleet holding write credentials" was given without anyone
  knowing the minted token covered **every repository in the installation**. The acceptance
  stands; §2 shrinks the surface it was granted over.

### Aug 03, 2026: 03:14 PM — decisions taken

  - > Indy (2026-08-03): "Crew supersedes §1/§2" — context: the multi-fleet shape replaces the single-fleet deterministic apply; the proposal/hash/bounds kernel is deleted in this diff.
  - > Indy (2026-08-03): "Keep it" — context: the detection benchmark stays, against Codex's recommendation to cut it with the pivot. Its rubric row is rewritten to claim detection only.
  - > Indy (2026-08-03): "I dont see a reason for a blocker with M154, why is it a blocker" — context: the M154 escalation was withdrawn; the final design touches no schema.
  - > Indy (2026-08-03): "I dont want to invent too many and get stuck, so i wanna use what we have built with few tweaks, can we not ask the agent to ask for approval to send a PR?" — context: the design pivot to gating the repairer's incoming event rather than building any new approval mechanism. This is the origin of §1–§3.
  - > Indy (2026-08-03): "can there be an auto approval or dangerouslyAccept.. type of as we;;" — context: confirmed already present — `.auto_approve` is the fallthrough when no gate rule matches, so unattended operation is the default and the gate is the opt-in. Dimension 3.2 guards the footgun.
  - > Indy (2026-08-03): "yes" — context: authorising this reconciliation, and with it the deferral of fleet identity and provenance work to M157_002.
