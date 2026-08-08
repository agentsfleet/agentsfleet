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

# M157_002: The repairer fleet ships an approved fix as one draft PR

**Prototype:** v2.0.0
**Milestone:** M157
**Workstream:** 002
**Date:** Aug 08, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the north star's write half; operators today retype the responder's prose fix by hand
**Categories:** API, DOCS, SKILL
**Batch:** B1 — single workstream, no parallel sibling
**Branch:** feat/m157-repairer-draft-pr
**Test Baseline:** unit=3493 integration=587
**Depends on:** M157_001 (read half — merged in PR #588 at `629319d0d`), M154_001 (schema renumber — the 8xx history layer this spec's slot 830 sits in)
**Provenance:** LLM-drafted (claude-fable-5, Aug 08, 2026) — design settled interactively with Indy this session; Discovery carries the verbatim quotes
**Canonical architecture:** `docs/architecture/scenarios/production-deploy-repair.md` §4 — rewritten by this spec's §5 to the fleet-writes design

---

## Overview

**Goal (testable):** A failed `workflow_run` event, approved by a human at the gate, leases the repairer fleet whose single run authors the fix from files it read at the verified head and opens exactly one draft Pull Request (PR); without that approval no write-scoped token is ever minted, and no minted token can ever touch `.github/workflows/**`.
**Problem:** the shipped read half ends with a repair intent in prose — a human reads Slack and retypes the fix by hand. The north star (amended by Indy this session) says the fleet finds the fix in the code and sends the PR itself.
**Solution summary:** the repairer returns as a fleet member. The approval gate parks any write-capable bundle's event **by kind, unconditionally** — a human approves the action before the run leases. The credential mint gains a write arm fenced to one repository with `contents: write` + `pull_requests: write`, never `workflows`, one hour. The repairer reads code via the contents endpoint at the verified head, authors the fix in-context, pushes over the Git Data API through plain `http_request` — no checkout, no git tooling — and opens one draft PR. The human approves the bytes where bytes are best reviewed: the PR itself. A slim webhook-driven linkage table records incident → PR → deploy result, replacing the deferred verifier member.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m157): the repairer fleet ships an approved fix as one draft PR
- **Intent (one sentence):** a code-shaped production regression becomes a reviewable draft PR authored by the fleet, with a human approving the write before it happens and the merge after reading the actual diff.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `library/incident-responder/SKILL.md` — the sibling bundle to mirror: endpoint tables, `${secrets.*}` placeholder discipline, the grounding rule, the wrap-up rules. The repairer inherits all of it and adds the write recipe.
2. `src/agentsfleetd/fleet/approval_gate.zig` — the gate-ref-before-policy ordering and the `.auto_approve` fallthrough; §1's kind-parking lands ahead of BOTH the rules walk and the no-gates-config early return.
3. `src/agentsfleetd/http/handlers/runner/credentials_mint_scope.zig` + `src/agentsfleetd/credentials/integration_github_body.zig` — lease-scope resolution (Invariant-2 ownership, repository binding, refuse-on-malformed) and the token request body the write arm extends.
4. `docs/architecture/scenarios/production-deploy-repair.md` — the canonical scenario; §4 currently argues daemon-side apply and is rewritten by §5 of this spec.
5. `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` + the `schema/8xx` neighbours (800 events, 810 approval gates, 820 memory) — the history layer slot `830` joins; since M154 the migration version IS the slot number and `cmd/migration_policy_test.zig` refuses anything below the floor.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `library/incident-repairer/SKILL.md` | CREATE | The repairer bundle: read telemetry/history/code, author the fix, push one branch, open one draft PR |
| `library/incident-repairer/TRIGGER.md` | CREATE | Wakes on failed `workflow_run` webhook events and manual steer — disjoint from the responder's scheduled sweeps |
| `src/agentsfleetd/fleet/approval_gate.zig` | EDIT | Park-by-kind: a write-access fleet's event parks unconditionally, before rules and before the no-gates-config return |
| `src/agentsfleetd/fleet/approval_gate_park.zig` | CREATE | The park tail shared by the rules path and the kind path (extracted for FLL headroom) |
| `src/agentsfleetd/fleet_runtime/approval_gate_slack.zig` | EDIT | Card gains the write blast-radius line (repository, branch budget); `evidence_json` moves off the backtick-closable code span (carried finding folds here) |
| `src/agentsfleetd/fleet_runtime/approval_gate_constants.zig` | EDIT | The write park kind + its blast-radius copy — `RepositoryAccess.write` in fleet config (shipped in M157_001) is the kind's source of truth, no new declaration needed |
| `schema/811_fleet_approval_gates_event_binding.sql` | CREATE | Additive migration: gate rows gain `event_id` + `stated_binding` — the durable record the write mint checks |
| `schema/embed.zig` | EDIT | Register 811 in the embed + migration array |
| `src/agentsfleetd/fleet_runtime/sql.zig` | EDIT | `INSERT_GATE` writes the two new columns |
| `src/agentsfleetd/fleet_runtime/approval_gate_db.zig` | EDIT | `recordGatePending` gains the event id and serializes the stated binding |
| `src/agentsfleetd/fleet_runtime/repository_binding_json.zig` | CREATE | One JSON spelling of a binding: park-side serialize + mint-side match |
| `src/agentsfleetd/fleet_runtime/config.zig` | EDIT | Test discovery for the new module |
| `src/agentsfleetd/http/handlers/fleets/create_grants.zig` | EDIT | Install-time gates record a null event id |
| `src/agentsfleetd/http/handlers/runner/credentials_mint.zig` | EDIT | Write arm: refuses without an approved write-kind gate row for the lease's event |
| `src/agentsfleetd/http/handlers/runner/credentials_mint_write_gate.zig` | CREATE | The durable-row approval check (status + kind + stated-binding match) |
| `src/agentsfleetd/http/handlers/runner/credentials_mint_scope.zig` | EDIT | `LeaseScope` gains the lease's event id |
| `src/agentsfleetd/http/handlers/runner/sql.zig` | EDIT | Lease-scope select gains `event_id`; new write-gate select |
| `src/agentsfleetd/credentials/integration_github_body.zig` | EDIT | Permission constants published for the response-side verify (write body itself shipped in M157_001; no `workflows` path exists) |
| `src/agentsfleetd/credentials/integration_github_reach.zig` | EDIT | `verifyPermissions`: response permissions must match the request; write/admin extras refuse, ambient read grants pass |
| `src/agentsfleetd/credentials/integration_github.zig` | EDIT | The mint refuses a token whose stated permissions mismatch |
| `src/agentsfleetd/credentials/integration_github_mint_body_test.zig` | EDIT | Fixture responses state their permissions |
| `src/agentsfleetd/credentials/testing.zig` | EDIT | Fake exchange responses echo permissions per access level |
| `src/agentsfleetd/http/handlers/runner/credentials_mint_integration_test.zig` | EDIT | Write-gate fixtures (append-only-safe) + the two refusal tests; drain fix in `dropWriteBackBlock` (RULE NLR) |
| `src/agentsfleetd/errors/gen_error_codes.zig` | EDIT | Public copy for the REPAIR category |
| `src/agentsfleetd/http/handlers/webhooks/github.zig` | EDIT | Two narrow arms: a repair-branch `pull_request` opened → linkage insert; a completed `workflow_run` on a linked branch → deploy stamp |
| `schema/830_repair_pr_links.sql` | CREATE | Incident → PR → deploy-result linkage; single-concern; 8xx history layer |
| `schema/embed.zig` | EDIT | Register 830 in the embed + migration array |
| `src/agentsfleetd/state/repair_pr_links.zig` | CREATE | Store: insert, lookup by branch/event, deploy-status transition; no content UPDATE surface |
| `src/agentsfleetd/state/repair_pr_links_test.zig` | CREATE | Unit + database tests for the store |
| `src/agentsfleetd/errors/error_entries_runtime.zig` | EDIT | New `UZ-REPAIR-*` refusal codes (unapproved write mint, binding mismatch, duplicate linkage), each with a negative test |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | The `UZ-REPAIR-010/011` code constants |
| `src/agentsfleetd/fleet/gate_release_integration_test.zig` | EDIT | Kind-park positive controls: parks with no gates config; parks when rules say auto-approve; approval releases and the mint write arm opens |
| `src/agentsfleetd/http/webhook_http_integration_test.zig` | EDIT | Linkage arms proven over the real router |
| `docs/architecture/scenarios/production-deploy-repair.md` | EDIT | §4 rewritten: the fleet writes behind the gated mint; daemon-apply prose removed; status table updated |
| `docs/v2/active/M157_001_P1_API_INFRA_OBS_SKILL_INCIDENT_TO_APPROVED_DRAFT_PR.md` | EDIT | Discovery gains the design-pivot record; spec moves `active/` → `done/` — its remaining scope IS this spec |
| ~~`public/openapi/components/schemas.yaml`~~ | — | Dropped at EXECUTE: the write arm keys off the fleet's binding, not a request field — no wire change, and the runner plane is not in the public OpenAPI |

`.github/workflows/**` is NOT in this table: §6 produces a findings report; any workflow edit requires Indy's explicit in-session approval per the CI/CD guard.

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — NDC (no dead code: the write arm ships wired to the gate check, never dormant), NLR (touch-it-fix-it: the `evidence_json` code-span fix rides the card edit), NLG (the scenario rewrite describes the fleet-writes design as *the* design — no "previously the daemon" framing), UFS (park-kind string, permission names, `UZ-REPAIR-*` codes, branch-name prefix shared verbatim across gate/mint/webhook/bundle), ORP (orphan sweep), FLL (length caps — mint and webhook handlers are near budget; split arms into siblings as the shipped `credentials_mint_scope.zig` precedent does), RULES.md #23 (JSON-escape all user-supplied card fields).
- `~/Projects/dotfiles/dispatch/write_zig.md` — pg-drain (`conn.query` → `.drain()`), tagged-union results, errdefer placement, cross-compile both linux targets; all daemon edits.
- `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` + STS/NSQ/SGR/ITF — single-concern files ≤100 lines; no static strings in DDL (deploy-status values are app-level named constants). **The additive model is in force** (the M154 rebuild landed): shipped slots are frozen, changes are new numbered `IF NOT EXISTS`-guarded files — hence `811` for the gate columns rather than editing shipped `810`.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` + `~/Projects/dotfiles/docs/LIFECYCLE_PATTERNS.md` — structured refusal events; init/deinit lifecycles on the new store.
- REST guidelines: no new public route; the mint field rides the existing runner-plane endpoint — document in OpenAPI, name no new path.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — daemon edits throughout | cross-compile x86_64-linux + aarch64-linux; pg-drain audit via `make check-pg-drain` |
| PUB / Struct-Shape | yes — new store + capability type | shape verdict per new pub surface at PLAN |
| File & Function Length (≤350/≤50/≤70) | yes — mint + webhook handlers near cap | new arms land as sibling files, mirroring the `credentials_mint_scope.zig` split |
| UFS (repeated/semantic literals) | yes | park-kind, permissions, branch prefix, status strings, `UZ-REPAIR-*` as named constants; cross-surface identifiers verbatim |
| UI Substitution / DESIGN TOKEN | no | no UI files in scope |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | structured events; store lifecycle; codes registered with negative tests; 830 + embed + migration array in one commit |

## Prior-Art / Reference Implementations

- **Reference:** `library/incident-responder/SKILL.md` — the bundle grammar, endpoint tables, grounding rule; the repairer is its write-capable sibling, not a new genre.
- **Reference:** `src/agentsfleetd/http/handlers/webhooks/github.zig` — the filter/dedup/normalize arm pattern the two linkage arms extend.
- **Reference:** `schema/810_*` + `820_*` history-layer files — the single-concern shape slot 830 mirrors.

## Sections (implementation slices)

### §1 — The gate parks the write kind, unconditionally

A bundle that may request a write mint declares it (fleet-config capability, §Files `config_types.zig`); the gate parks that bundle's events before consulting rules and even when no gates config exists. Rule-parking is unsafe by construction — `.auto_approve` is the no-match fallthrough and rules are `fleet:write`-PATCHable — so the kind check cannot live in rules. The card states the write blast radius as daemon fact: repository (from the binding), budget (one branch, one draft PR).

- **Dimension 1.1** — DONE — write-capable bundle parks with NO gates config present → Test `test_write_kind_parks_without_gates_config`
- **Dimension 1.2** — DONE — write-capable bundle parks when rules would auto-approve → Test `test_write_kind_ignores_rule_fallthrough`
- **Dimension 1.3** — DONE — card carries repository + budget as daemon-derived lines; `evidence_json` renders code-span-safe → Test `test_card_write_radius_and_span_safety`
- **Dimension 1.4** — DONE — approval releases the lease and the run proceeds, proven inside both write-kind tests via the shared park→approve→release harness (`runParkApproveRelease` asserts lease ownership by fleet+event)

### §2 — The write mint, fenced

The mint's write arm issues only against durable rows: the gate row parked for the lease's event must be `approved`, of the repository-write kind, and its recorded `stated_binding` (a new column the park writes — the reach the card told the human) must still match the fleet's CURRENT binding. No Redis on the check path, so a cache loss can only withhold a token, never widen one. The token body names `contents: write` + `pull_requests: write` for exactly the bound repository; no code path emits `workflows`. Reach verification compares the response's `permissions` to the request AND `repositories` to the binding.

**Amended at EXECUTE (recorded in Discovery):** the plan's "lease-stamped `execution_policy.repository_binding` cross-check" was unimplementable — that stamp is wire-only (no lease column) and its runner-side reader died with the fetch path. The real drift hole is approval-to-mint (a `fleet:write` PATCH rebinding between the human's answer and the mint), closed by persisting the stated binding on the gate row at park time. The wire stamp remains reader-less; recorded honestly rather than given a ceremonial consumer.

- **Dimension 2.1** — DONE — approved gate for the lease's event → write token with exactly the two permissions → Test `test_mint_rechecks_revoked_grant` (the file's one successful write mint, now gated)
- **Dimension 2.2** — DONE — no gate row, or a pending one → refusal `UZ-REPAIR-010`, broker never reached → Test `test_write_mint_refuses_unapproved`
- **Dimension 2.3** — DONE — stated-binding/config mismatch → refusal `UZ-REPAIR-011` → Test `test_write_mint_refuses_binding_drift`
- **Dimension 2.4** — DONE — reach verify fails a response whose `permissions` exceed or miss the request; ambient read grants tolerated → Test `test_reach_verifies_permissions`

### §3 — The repairer bundle

`incident-repairer` mirrors the responder's discipline and adds: read file contents at the head it verified this run (`GET /repos/{owner}/{repo}/contents/{path}`); author the complete corrected files in-context; push via Git Data API over `http_request` (blobs → trees → commit → ref); branch named from the incident event id so a replay finds the ref taken and reports a duplicate instead of pushing twice; open exactly one draft PR carrying cause, evidence, changed files, and forward rationale. Forward-only; no checkout; no git tooling; a partial read ends the run diagnosis-only. Wakes on failed `workflow_run` events and manual steer — the responder keeps the scheduled sweeps, so the picker's choice is deterministic by event type.

- **Dimension 3.1** — DONE — SKILL.md ships the write recipe, branch/PR budget, forward-only and grounding rules → Test `test_repairer_bundle_frontmatter_and_rules` (plus `test_repairer_token_is_write_scoped_and_workflow_free`, driving the shipped binding through the real mint)
- **Dimension 3.2** — DONE — TRIGGER.md matches webhook incidents and not schedule sweeps; neither member declares the other's trigger type → Test `test_repairer_trigger_disjoint_from_responder`
- **Dimension 3.3** — DONE — crew-folder proof: both shipped bundles, read from `library/` as the folder picker hands them up, onboard through `POST /v1/workspaces/{ws}/fleet-libraries` into two distinct catalog rows → Test `test_crew_folder_two_member_onboard`

### §4 — The linkage: incident → PR → deploy result

Slot 830 stores one row per shipped repair: workspace, fleet, incident event id, repository, branch, PR number/URL, deploy status + stamped-at. Insert-only; deploy status is the single mutable column. Two webhook arms feed it: a `pull_request` opened whose head matches the repair-branch prefix inserts the row; a completed `workflow_run` on a linked branch stamps the result. This replaces the deferred verifier member: "did the fix work" is a column, not a model run. Implementation default: statuses are app-level named constants (STS) — `pending / deploy_ok / deploy_failed`.

- **Dimension 4.1** — DONE — migration 830 registers and applies (proven by every integration run's migrate-from-empty; ascending-slot policy holds with 830 in the 8xx layer) → migration policy tests + `make test-integration` reset lane
- **Dimension 4.2** — DONE — store insert/lookup/transition; content frozen by SCHEMA TRIGGER (mirroring 810's, including the sanctioned purge switch), not store discipline → Test `test_repair_link_store_immutability`
- **Dimension 4.3** — DONE — PR-opened arm inserts exactly one row, produces NO fleet event (the crew's own PR echo can never wake it), duplicate refused with `UZ-REPAIR-012` → Test `test_pr_opened_arm_inserts_once`
- **Dimension 4.4** — DONE — completed `workflow_run` on a linked branch stamps status without waking the fleet (a FAILED run on the crew's own branch is a stamp, not an incident); unknown branch acknowledged, nothing recorded → Test `test_deploy_stamp_and_unknown_branch_noop`

### §5 — Documentation reconciled in the same diff

Scenario §4 is rewritten to the fleet-writes design (NLG: described as *the* design), its status table updated to what this spec ships; M157_001's Discovery records the pivot with Indy's quotes and the spec moves to `done/` — its residual scope is this workstream.

- **Dimension 5.1** — DONE — scenario doc carries no daemon-apply claim and no revert language (R6 grep: zero matches) → Test rubric R6 (grep gate)
- **Dimension 5.2** — DONE — M157_001 in `done/` (Status: DONE) with the Aug 08 pivot Discovery entry carrying Indy's verbatim quotes → Test rubric R2 diff review

### §6 — Workflow secrets audit (report-only)

Enumerate every workflow triggered by `pull_request`, list the secrets each mounts, and record the findings + minimization proposal in PR Session Notes and Discovery. Edits to `.github/workflows/**` happen only on Indy's explicit approval; none are assumed by this spec.

- **Dimension 6.1** — the audit report exists in Session Notes with per-workflow secret inventory → Test rubric R8

## Interfaces

```
POST /v1/runners/me/credentials/mint — wire shape UNCHANGED ({lease_id, integration}).
  The write arm keys off the fleet's binding access, not a request field.
  Write refusals: 403 UZ-REPAIR-010 (no approved write-kind gate for the lease's
  event) · 403 UZ-REPAIR-011 (stated binding no longer matches config).
Gate row: gains event_id + stated_binding (schema 811, additive); park records both;
  the write mint reads durable rows only — no Redis on the check path.
Webhook arms: no new routes — POST /v1/webhooks/{fleet_id}/github gains two accept arms
Repair branch name: {prefix}/{incident event id} — prefix a named constant shared by bundle
  prose, webhook matcher, and tests (UFS)
schema 830: repair_pr_links — insert-only; deploy_status sole mutable column
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unapproved write mint | run requests write without an approved kind ref | refuse + `UZ-REPAIR-*`; structured event; run continues read-only |
| Binding drift | fleet config PATCHed after lease stamped | mint refuses on stamp≠config; code in activity stream |
| Permissions overreach | GitHub echoes more/less than requested | reach verify fails; token discarded; refusal logged |
| Workflows-path push | model attempts `.github/workflows/**` edit | GitHub refuses — token lacks the permission; repairer reports the refusal, never retries |
| Duplicate repair | replayed event / second run for same incident | branch ref exists → create-ref fails → repairer reports duplicate; linkage insert refuses on event id |
| Stale base | head moved between read and push | push lands on the derived branch regardless; the PR diff shows the drift to the human reviewer |
| Injected telemetry | hostile log line steers the run | blast radius is structural: one repo, two permissions, no workflows, one hour, one branch budget; bundle rules refuse off-allowlist hosts |
| Webhook replay | GitHub redelivery | existing delivery-id dedup already single-enqueues; linkage arms are idempotent by event id / branch |
| Partial read | contents/telemetry endpoint unreachable | bundle rule: diagnosis-only run; no push follows a partial read |

## Invariants

1. A write-capable bundle never leases without a recorded human approval — kind check in `gateCheck` ahead of rules AND the no-gates return; integration-proven both ways.
2. No minted token ever carries `workflows` — the body builder has no emitting code path; negative unit test pins the built JSON.
3. The write arm issues only against an approved write-kind gate ref for that lease's event — runtime check in the mint handler.
4. Token repository == lease-stamped binding == fleet-config binding at mint time — runtime equality check, refuse on any mismatch.
5. `repair_pr_links` content columns are insert-only — the store exposes no content UPDATE; deploy_status transition is the sole mutator.
6. The daemon never merges and never deploys — no code path calls merge; unchanged from M157_001.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| gate write-kind parked | ops | write-capable bundle's event parks | fleet id, event id, repository | no tokens, no secret names | `test_write_kind_parks_without_gates_config` |
| write mint issued / refused | ops | mint write arm resolves | outcome, refusal code, repository | token never logged | `test_write_mint_issues_after_approval` |
| repair PR linked | product | PR-opened arm inserts | repository, PR number, incident id | no diff content | `test_pr_opened_arm_inserts_once` |
| deploy result stamped | product | workflow_run arm transitions status | status, repository, PR number | none beyond ids | `test_deploy_stamp_and_unknown_branch_noop` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_write_kind_parks_without_gates_config` | fleet with no `gates` config + write-capable bundle → event parks, card queued |
| 1.2 | integration | `test_write_kind_ignores_rule_fallthrough` | rules that auto-approve → write kind still parks |
| 1.3 | unit | `test_card_write_radius_and_span_safety` | card JSON carries repository + budget lines; backtick-bearing evidence stays inert |
| 1.4 | integration | `test_approved_write_kind_releases_lease` | approve → lease released, owned by fleet+event (extends shipped positive control) |
| 2.1 | integration | `test_write_mint_issues_after_approval` | approved event → 200; body has exactly contents:write + pull_requests:write |
| 2.2 | integration | `test_write_mint_refuses_unapproved` | no ref → 4xx with the named code; no token row |
| 2.3 | integration | `test_write_mint_refuses_binding_drift` | stamp≠config → 4xx; code logged |
| 2.4 | unit | `test_reach_verifies_permissions` | echoed permissions ⊃ or ⊄ requested → verify fails |
| 3.1 | unit | `test_repairer_bundle_frontmatter_and_rules` | frontmatter parses; write recipe, budget, forward-only strings present |
| 3.2 | unit | `test_repairer_trigger_disjoint_from_responder` | webhook event → repairer; schedule event → responder |
| 3.3 | e2e | `test_crew_folder_two_member_onboard` | two subfolders → two onboard calls → two catalog rows → both pickable |
| 4.1 | integration | `test_migration_830_registered` | migrate from empty passes; policy floor accepts 830 |
| 4.2 | integration | `test_repair_link_store_immutability` | content UPDATE has no store surface; status transition works once |
| 4.3 | integration | `test_pr_opened_arm_inserts_once` | repair-branch PR event → one row; replay → refusal code, still one row |
| 4.4 | integration | `test_deploy_stamp_and_unknown_branch_noop` | linked branch run → status stamped; unknown branch → no row, no error |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Write kind parks past both holes (§1) | `zig build test-integration-bin -Dtest-filter=write_kind && zig-out/bin/agentsfleetd-integration-tests` | both park tests' OK lines | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the table | P0 | |
| R3 | Unapproved mint cannot write (§2) | `zig build test-integration-bin -Dtest-filter=write_mint && zig-out/bin/agentsfleetd-integration-tests` | refusal tests' OK lines | P0 | |
| R4 | No token names workflows | `rg -n '"workflows"' src/agentsfleetd/credentials/` | 0 matches in emitting code (test fixtures exempt) | P0 | |
| R5 | Linkage round-trips (§4) | `zig build test-integration-bin -Dtest-filter=repair_link && zig-out/bin/agentsfleetd-integration-tests` | insert + stamp OK lines | P0 | |
| R6 | Scenario doc carries the shipped design (§5) | `rg -in 'daemon applies\|git revert\|rolls? back' docs/architecture/scenarios/production-deploy-repair.md` | 0 matches | P0 | |
| R7 | Crew uploads through the shipped endpoint (§3) | e2e `test_crew_folder_two_member_onboard` | OK line | P1 | |
| R8 | Workflow-secrets audit recorded (§6) | PR Session Notes section present | per-workflow inventory listed | P1 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

### Behaviour evals

- **Grounding rule:** every identifier the repairer cites or pushes against — commit, head, file path, trace id — is a value an upstream returned this run; no push follows a partial read.
- **Golden set:** `bench/incident-response` seeds — extend with one repair-shipping case and one injection nightmare (hostile log line steering toward an off-budget write). The set only grows.
- **Ship threshold:** grounding 100% · 0 critical failures on the injection nightmare (run refuses or stays inside the one-branch budget). Each threshold is one rubric-adjacent bench line cited in Session Notes.
- **Fallback:** low confidence or partial read → diagnosis-only run (named degradation); a fabricated identifier or off-budget write attempt is a P0 ❌.

## Dead Code Sweep

N/A — no files deleted. (`execution_policy.repository_binding` gains its consumer in §2 rather than deletion; pickup decision 3 resolved as fold.)

## Out of Scope

- **Verifier fleet member** — deferred (Indy-acked, Discovery); replaced by §4's linkage columns.
- **Proposer as a separate member** — the repairer authors in its own run; a split member returns only if context budgets demand it.
- **Daemon-side apply / proposal kernel** (hash-bound approval, immutable proposal bytes) — superseded by this design; the scenario rewrite records it neutrally.
- **`.github/workflows/**` edits** — §6 reports; edits are a separate Indy-approved change.
- **Vercel Log Drain intake · EgressScope enforcement** — unchanged from M157_001's exclusions.
- **Bidi/zero-width hardening on picker-loaded bundle bodies · `purgeFleetRedisState` byevent refs** — recorded findings, untouched here.
- **Structural branch-name enforcement** — the budget is bundle-instructed + linkage-refused, not daemon-blocked; accepted with the design.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Slack shows "repair PR #N opened" minutes after the operator taps Approve on the gate card, and the PR diff is the fix they would have typed.
2. **Preserved user behaviour** — responder sweeps, diagnoses, Jira/Slack surfaces, approvals list, and every read-only flow ship unchanged; merge stays fully human.
3. **Optimal-way check** — direct: one gate decision + one PR review, both on surfaces that already exist. The unconstrained optimum (auto-verified canary rollback) contradicts the forward-only frame and human-merge rule.
4. **Rebuild-vs-iterate** — iterate: every mechanism extends a shipped seam (gate, mint, webhook, library upload). Verdict: patch-shaped; no refactor wanted.
5. **What we build** — one bundle, one gate kind, one mint arm, two webhook arms, one table, one doc rewrite.
6. **What we do NOT build** — verifier/proposer members, proposal kernel, workflow edits, Vercel intake (each: Out of Scope).
7. **Fit with existing features** — compounds the M148 assigned-isolation substrate and M157_001's gate/mint hardening; must not destabilize the responder's diagnosis-only path.
8. **Surface order** — API-first; the only UI is the existing approvals card/dashboard, which gains one factual line.
9. **Dashboard restraint** — no repair dashboard until linkage rows exist in production; the approvals list and PR link are the only surfaces.
10. **Confused-user next step** — the card's refusal/duplicate codes (`UZ-REPAIR-*`) name the reason in the activity stream; the PR body carries cause + evidence; no ticket surface needed.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six slices along the trust chain — gate kind → fenced mint → bundle → linkage → docs → audit — because each is independently provable against a shipped seam.
- **Alternatives considered:** (a) daemon-side apply with hash-bound proposals (the Aug 06 Discovery plan) — rejected by Indy this session: PR review is the byte-approval surface, and the kernel machinery duplicates it; (b) two-run design (read run → approve → fresh write run) — rejected: the gate already resolves between runs, so one approved run with mid-run mint is strictly simpler and keeps authoring and pushing in the same context.
- **Patch-vs-refactor verdict:** this is a **patch** because every touched surface exists and keeps its shape; the one new concept (park kind) is a guard, not an architecture.

## Discovery (consult log)

- **Consults** — Architecture: fleet-writes vs daemon-applies settled by Indy, `> Indy (2026-08-08 02:20): "Well i think we need to use the repairer as a fleet which does the check on the code using null claw llm and send a the fix as PR, why do we need the daemon to do the fix"`; risk acceptance `> Indy (2026-08-08 02:35): "yes its  a PR and not merged, so its safe to send a PR"` — context: same-repo PR branches execute CI pre-review; accepted, mitigated by §2 fencing + §6 audit; north star amended `> Indy (2026-08-08 02:50): "north start is to send the PR as well after finding the fix in the code"`.
- **EXECUTE findings (Aug 08)** — (1) `RepositoryAccess{read,write}` already ships in fleet config (M157_001's grant split) — the park kind needed no new declaration. (2) The planned lease-stamp cross-check was unimplementable (wire-only stamp, no lease column); replaced by the stronger durable check: gate rows gain `event_id` + `stated_binding` (schema 811) and the write mint compares the approved reach against current config — the wire `execution_policy.repository_binding` REMAINS reader-less; carried honestly for the runner-side egress double-check when that lands. (3) Schema model is additive post-M154 — shipped slots frozen; 811 is a new file, and §4's linkage keeps slot 830 (ascending order holds). (4) Touch-it-fix-it: `dropWriteBackBlock` exec'd while its own SELECT was open (ConnectionBusy) — residue trigger poisoned every vault write in the mint suite; fixed with the drain-correct shape.
- **Metrics review** — four ops/product events declared above; no analytics/funnel playbook update required (operator-plane signals only, no user funnel touched).
- **Skill-chain outcomes** — (populated at CHORE(close): `/write-unit-test`, `/write-integration-test`, gstack `/review`, `kishore-babysit-prs`.)
- **Deferrals** — `> Indy (2026-08-08 02:30): "and verified is not needed? for now."` — context: verifier fleet member deferred; §4's webhook-driven linkage carries the "did the fix work" signal until the member regrows.
