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

# M157_003: The repair loop records every run, refuses an unproven one, and closes on a verified fix

**Prototype:** v2.0.0
**Milestone:** M157
**Workstream:** 003
**Date:** Aug 10, 2026
**Status:** PENDING
**Priority:** P1 — the deploy-result column an operator reads is absent on the route most installations use, and one approval funds unbounded mints
**Categories:** API, INFRA, SKILL
**Batch:** B1 — §1–§4 harden the shipped write path and are mutually independent; B2 — §5–§7 extend it and read B1's run rows
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** M157_002 (ships the write mint, `core.repair_pr_links`, and the `incident-repairer` bundle this workstream hardens)
**Provenance:** agent-generated (pre-spec, PR #591 Session notes — the four recorded deferrals, Indy-acked Aug 10, 2026)
**Canonical architecture:** `docs/architecture/scenarios/production-deploy-repair.md`

---

## Overview

**Goal (testable):** A repair Pull Request opened through the default GitHub App ingress records its incident linkage; every workflow run on a repair branch lands its own immutable result row keyed by workflow identity and run id; a repair PR whose author or installation does not match the incident's fleet records nothing; and one approved gate funds a bounded number of write mints.

**Problem:** An operator asks "did the fix actually work?" and the column is empty. On the ingress route that standard GitHub App installations use, no linkage row is ever written, so the incident, its repair Pull Request, and the deploy outcome are three unconnected facts. Where a row does exist, the outcome it shows is whichever workflow finished last — a lint run overwrites a deploy run — and a run that completes before the Pull Request opens shows nothing at all. Separately, one approval on one card funds every retry of that event.

**Solution summary:** Linkage moves into a shared arm both routes call, with the owning fleet resolved from the incident id in the branch name rather than from grant matching. The mutable outcome pair on `core.repair_pr_links` becomes append-only run rows carrying workflow identity and run id, so results accumulate instead of overwriting and an early run is retained. The arm verifies the Pull Request's author and installation belong to the incident's fleet before recording, and the write mint counts spends against its gate row. On top of that, a verifier crew member reads the run rows and closes the loop, Vercel deploy failures become a second intake, and a live-repo acceptance run proves the arc outside the harness.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m157): the repair loop records every run and closes on a verified fix
- **Intent (one sentence):** An operator can see whether a shipped repair actually fixed the incident, on the route their installation actually uses, without trusting a single overwritable column.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/webhooks/github_repair_link.zig` — the linkage arm as shipped; §1 lifts it to a shared call site and §2/§3 change what it writes and what it trusts.
2. `src/agentsfleetd/http/handlers/ingress/github.zig` — the ingress route: it resolves a workspace from the installation id, then fans out to fleets by grant match. §1's resolution is a different question and must not reuse `findTargets`.
3. `src/agentsfleetd/fleet_runtime/webhook/normalizer/github_app.zig` — where repair-branch traffic is dropped today so it cannot wake a fleet. §1 records before that drop; the drop itself stays.
4. `src/agentsfleetd/http/handlers/runner/credentials_mint_write_gate.zig` — the approval read §4 adds a spend count to; it deliberately reads only durable rows.
5. `schema/830_repair_pr_links.sql` — the immutability trigger and the purge switch the new run table must mirror.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/831_repair_run_results.sql` | CREATE | Append-only run rows: workflow identity, run id, conclusion, per linkage row |
| `schema/832_fleet_approval_gates_spend.sql` | CREATE | Spend counter and ceiling on the gate row |
| `schema/embed.zig` | EDIT | Register 831 and 832 in the embed and migration array |
| `src/agentsfleetd/state/repair_pr_links.zig` | EDIT | Stamp path retires; run-row insert and latest-result read replace it |
| `src/agentsfleetd/state/repair_run_results.zig` | CREATE | Store for the append-only run rows |
| `src/agentsfleetd/state/sql.zig` | EDIT | Statements for the new store, per the SQL Statement Modules rule |
| `src/agentsfleetd/http/handlers/webhooks/github_repair_link.zig` | EDIT | Records run rows; verifies provenance; loses the mutable stamp |
| `src/agentsfleetd/http/handlers/webhooks/repair_link_provenance.zig` | CREATE | Author, installation, and fork checks, split out to hold the length cap |
| `src/agentsfleetd/http/handlers/ingress/github.zig` | EDIT | Calls the linkage arm for repair-branch traffic before the normalizer drop |
| `src/agentsfleetd/http/handlers/ingress/repair_fleet_resolve.zig` | CREATE | Resolves the owning fleet from the incident id in the branch name |
| `src/agentsfleetd/http/handlers/runner/credentials_mint_write_gate.zig` | EDIT | Counts the spend and refuses past the ceiling |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | `UZ-REPAIR-013` (ceiling), `UZ-REPAIR-014` (provenance) |
| `src/agentsfleetd/errors/error_entries_runtime.zig` | EDIT | Runtime entries for the two new codes |
| `src/agentsfleetd/fleet_runtime/webhook/normalizer/vercel.zig` | CREATE | Vercel deploy-failure intake normalised to the incident envelope |
| `library/incident-verifier/SKILL.md` | CREATE | The verifier member: reads run rows, judges whether the incident cleared |
| `library/incident-verifier/TRIGGER.md` | CREATE | Its trigger, credentials, and read-only repository binding |
| `docs/architecture/scenarios/production-deploy-repair.md` | EDIT | The scenario gains the verifier and the second intake |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (the retired stamp path leaves no unused reader), **NLR** (the files touched here are cleaned as they are touched), **UFS** (workflow-identity field names, the two new error codes, and the verifier bundle name repeat across surfaces — name each literal once per file), **ORP** (orphan sweep after the stamp path retires), **FLL** (`github_repair_link.zig` and `ingress/github.zig` are both close to the cap; §1 and §3 split rather than grow them), **SCH** (two new single-concern schema files), **ITF** (immutability held by the schema, not by store discipline).
- `~/Projects/dotfiles/dispatch/write_zig.md` — every new daemon file: pg-drain, tagged-union results, `errdefer` placement, both Linux cross-compiles.
- `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` — `831` and `832` are new single-concern files in the history layer; no static strings in Data Definition Language (DDL); edited in place, never `ALTER`ed.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` + `~/Projects/dotfiles/docs/LIFECYCLE_PATTERNS.md` — structured events on both new paths; init/deinit lifecycles on the new store.
- `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` — no new public route; the Vercel intake rides the existing `/v1/ingress/{provider}` shape.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all new daemon code | pg-drain audit, tagged-union results, `zig build -Dtarget=x86_64-linux` and `-Dtarget=aarch64-linux` |
| PUB / Struct-Shape | yes — new stores and resolvers | shape verdict per new pub surface before the first call site lands |
| File & Function Length (≤350/≤50/≤70) | yes — `github_repair_link.zig` and `ingress/github.zig` are near the cap | provenance and fleet resolution land as their own files, never as growth |
| UFS (repeated/semantic literals) | yes — payload field names and error codes repeat | one named constant per literal per file; codes shared verbatim across runtime and registry |
| UI Substitution / DESIGN TOKEN | no | no UI files in scope |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes — all four | structured events on record and refuse paths; `UZ-REPAIR-013/014` registered with negative tests; 831/832 land with `embed.zig` and the migration array in one commit |

## Prior-Art / Reference Implementations

- **Reference:** `schema/830_repair_pr_links.sql` + `src/agentsfleetd/state/repair_pr_links.zig` — the append-only table with a purge switch and the store that reads it. `831` mirrors this shape exactly rather than inventing a second immutability idiom.
- **Reference:** `src/agentsfleetd/fleet_runtime/webhook/normalizer/github_app.zig` — the normaliser shape the Vercel intake mirrors: pure functions over a parsed payload, no input/output, returning an ignore reason or a normalised envelope.
- **Reference:** `library/incident-responder/` — the read-only crew member the verifier mirrors for frontmatter, credential declaration, and repository binding.

## Sections (implementation slices)

### §1 — Linkage on the route installations actually use

The linkage arm is reachable only from the per-fleet webhook handler, which receives its fleet from the Uniform Resource Locator (URL) path. Standard GitHub App installations deliver to the shared ingress route, where the workspace comes from the installation id and fleets are matched by grant — a match a repair Pull Request need not satisfy. So the linkage row, and therefore the deploy-result column, is absent exactly where most installations live. This slice gives ingress the same arm. The normaliser's existing drop stays: recording happens first, waking still never does.

**Implementation default:** resolution reads the incident event id out of the branch and looks up the fleet owning that event, because grant matching answers a different question and would silently link the wrong fleet.

- **Dimension 1.1** — a repair Pull Request opening on the ingress route records a linkage row → Test `test_ingress_repair_pr_records_linkage`
- **Dimension 1.2** — the fleet is resolved from the incident id, never from grant matching → Test `test_ingress_resolves_owning_fleet_not_grant_match`
- **Dimension 1.3** — an incident id naming no live event records nothing and is acknowledged → Test `test_ingress_unknown_incident_records_nothing`
- **Dimension 1.4** — repair traffic still never wakes a fleet on either route → Test `test_ingress_repair_traffic_never_wakes`

### §2 — Every run keeps its own result

`core.repair_pr_links` carries one mutable outcome pair, so the value an operator reads is whichever workflow finished last — a lint run overwrites a deploy run with no way to tell. A run completing before the Pull Request opens finds no row and is dropped entirely. This slice replaces the pair with append-only run rows carrying workflow identity and run id, so results accumulate, arrive in any order, and a replay changes nothing.

**Implementation default:** rows are keyed by run id and carry workflow identity; the operator-facing answer is the latest row for the deploy workflow, computed on read rather than stored, because a stored summary is the mutable column this slice exists to remove.

- **Dimension 2.1** — each completed run on a repair branch inserts its own row → Test `test_each_run_appends_its_own_result`
- **Dimension 2.2** — a run completing before the Pull Request opens is retained and links when the row arrives → Test `test_early_run_is_retained_not_dropped`
- **Dimension 2.3** — a redelivered run id inserts nothing the second time → Test `test_replayed_run_id_is_idempotent`
- **Dimension 2.4** — a non-deploy workflow never displaces the deploy answer → Test `test_lint_run_does_not_displace_deploy_result`
- **Dimension 2.5** — the retired mutable stamp path has no remaining caller → Test `test_stamp_path_has_no_production_caller`

### §3 — A repair Pull Request proves whose it is

The arm trusts the branch prefix alone: anything named `agentsfleet-repair/<id>` opening on a watched repository records a row against the fleet owning that id. Author, installation, and fork-versus-base go unchecked. Unguessable incident ids and first-writer-wins make this hard to exploit, but the assumption is unenforced, and an unenforced assumption is not a boundary. This slice checks the Pull Request's author and installation against the incident's fleet before recording, and refuses a head that is not the base repository.

- **Dimension 3.1** — a Pull Request whose installation does not own the incident records nothing → Test `test_foreign_installation_records_nothing`
- **Dimension 3.2** — a fork head is refused → Test `test_fork_head_is_refused`
- **Dimension 3.3** — the refusal is a typed code, not a silent drop → Test `test_provenance_refusal_returns_uz_repair_014`
- **Dimension 3.4** — the repairer's own Pull Request still records → Test `test_own_repair_pr_still_links`

### §4 — One answer funds a bounded number of mints

An approved gate is read fresh on every mint and always answers the same way, so every retry of an approved event mints another write token. Each token is short-lived, single-repository, and cannot name `workflows`, so the blast radius is bounded — but "one click, unbounded mints" is not what the card tells the person clicking it. This slice counts spends against the gate row and refuses past a declared ceiling.

**Implementation default:** the counter increments in the same transaction as the mint decision, because a count written afterwards can be lost by the failure that makes counting matter.

- **Dimension 4.1** — a mint increments the gate row's spend count → Test `test_mint_increments_spend_count`
- **Dimension 4.2** — a mint past the ceiling is refused with `UZ-REPAIR-013` → Test `test_mint_past_ceiling_refuses`
- **Dimension 4.3** — concurrent mints on one gate cannot exceed the ceiling → Test `test_concurrent_mints_respect_ceiling`
- **Dimension 4.4** — the card states the ceiling the approval funds → Test `test_card_states_spend_ceiling`

### §5 — The verifier member closes the loop

The crew diagnoses and repairs, then stops. Nothing reads the run rows and says whether the incident actually cleared, so the last judgement falls to whoever opens the dashboard. This slice adds a read-only crew member that wakes on a repair branch's deploy result, reads the incident's telemetry after the fix landed, and records a verdict. **Implementation default:** its repository binding is read, mirroring the responder, because a member whose job is judging must not be able to change what it judges.

- **Dimension 5.1** — the bundle onboards through the existing upload path → Test `test_verifier_onboards_as_crew_member`
- **Dimension 5.2** — its minted token carries no write permission → Test `test_verifier_token_is_read_only`
- **Dimension 5.3** — a deploy result wakes the verifier, and nothing else does → Test `test_verifier_wakes_on_deploy_result_only`

### §6 — Vercel deploy failures are a second intake

The repairer wakes on a failed GitHub workflow run. The application, agents, and website deploy through Vercel, where a failure produces no such run — so the deploys most likely to need a repair cannot start one. This slice normalises a Vercel deploy failure into the same incident envelope the GitHub intake produces.

- **Dimension 6.1** — a failed Vercel deploy produces the same incident envelope shape → Test `test_vercel_failure_normalises_to_incident`
- **Dimension 6.2** — a successful deploy produces no incident → Test `test_vercel_success_is_ignored`
- **Dimension 6.3** — an unsigned or missigned delivery is refused → Test `test_vercel_unsigned_delivery_refused`

### §7 — The arc is proven outside the harness

Every claim about the write path rests on fixtures and a mocked GitHub. Nothing has driven incident → approval → branch → draft Pull Request → run rows against a live repository, so the first real proof would otherwise be a customer's. This slice adds a live acceptance run against a disposable repository, run on demand rather than per commit.

- **Dimension 7.1** — the run drives the full arc against a live repository and asserts one draft Pull Request → Test `test_live_repair_arc_opens_one_draft_pr`
- **Dimension 7.2** — the run leaves no branch, Pull Request, or row behind → Test `test_live_acceptance_cleans_up`

## Interfaces

```
core.repair_run_results (schema 831)
  id, repair_link_id → core.repair_pr_links(id), workflow_name, workflow_run_id,
  conclusion, completed_at, created_at
  UNIQUE (repair_link_id, workflow_run_id)   -- replay lands nothing
  content columns immutable; DELETE refused except under the purge switch 830 honours
core.fleet_approval_gates (schema 832 adds)
  spend_count BIGINT NOT NULL DEFAULT 0, spend_ceiling BIGINT NOT NULL
Refusals (existing shape; typed, never silent)
  403 UZ-REPAIR-013  approval spend ceiling reached
  403 UZ-REPAIR-014  repair Pull Request provenance rejected
Ingress linkage — no new public route. The ingress path gains the linkage arm
for repair-branch deliveries, called before the normaliser's existing drop.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unknown incident on ingress | Branch names an event id no live fleet owns | Nothing recorded; delivery acknowledged with an ignore reason |
| Foreign installation | Pull Request opened by an installation not owning the incident | Nothing recorded; `UZ-REPAIR-014` |
| Fork head | Repair-named branch on a fork | Nothing recorded; `UZ-REPAIR-014` |
| Early run | Workflow completes before the Pull Request opens | Row retained, linked when the linkage row arrives |
| Replayed run | GitHub redelivers a completed run | Unique key absorbs it; nothing inserted, acknowledged |
| Ceiling reached | Retries of an approved event exhaust the ceiling | Mint refused; `UZ-REPAIR-013`; run continues read-only |
| Concurrent mints | Two leases mint against one gate at once | Counted in-transaction; the ceiling holds |
| Database unavailable | Pool acquire fails on the linkage path | Delivery fails closed; nothing partially recorded |
| Unsigned Vercel delivery | Missing or wrong signature | Refused before parsing; no incident raised |
| Live acceptance leftovers | Acceptance run aborts mid-arc | Cleanup is idempotent and reruns clean |

## Invariants

1. A linkage row is written for a repair Pull Request on either route, or on neither — enforced by both call sites entering the same arm, asserted by 1.1 and 3.4.
2. A run result is never overwritten — enforced by the 831 immutability trigger and the unique key, not by store discipline.
3. The operator-facing deploy answer is computed from run rows, never stored — enforced by the absence of a mutable outcome column after §2, asserted by 2.5's orphan sweep.
4. An approved gate cannot fund more mints than its ceiling — enforced by the counter incrementing in the mint's own transaction, asserted by 4.3 under concurrency.
5. The verifier can never write to a repository it judges — enforced by its declared read binding, asserted by 5.2 against the real mint.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `repair_run_recorded` | ops | A completed run on a repair branch inserts a row | fleet id, repository, workflow name, run id, conclusion | no token or payload material | `test_each_run_appends_its_own_result` |
| `repair_provenance_rejected` | ops | Author or installation fails the check | fleet id, incident id, reason | no author email or account detail | `test_foreign_installation_records_nothing` |
| `repair_mint_ceiling_reached` | ops | A mint is refused at the ceiling | fleet id, event id, spend count, ceiling | no token material | `test_mint_past_ceiling_refuses` |
| `repair_incident_verified` | ops | The verifier records a verdict | fleet id, incident id, verdict | no telemetry payload content | `test_verifier_wakes_on_deploy_result_only` |

No product analytics event changes — these are operator signals on an existing operator surface, so no analytics/funnel playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_ingress_repair_pr_records_linkage` | A repair Pull Request delivered to the ingress route leaves exactly one linkage row |
| 1.2 | integration | `test_ingress_resolves_owning_fleet_not_grant_match` | With a grant-matching fleet and a different owning fleet, the row names the owner |
| 1.3 | integration | `test_ingress_unknown_incident_records_nothing` | An unknown incident id leaves zero rows and answers with an ignore reason |
| 1.4 | integration | `test_ingress_repair_traffic_never_wakes` | No event reaches the fleet stream on either route for repair traffic |
| 2.1 | integration | `test_each_run_appends_its_own_result` | Three completed runs leave three rows, each with its own run id |
| 2.2 | integration | `test_early_run_is_retained_not_dropped` | A run arriving before the Pull Request is present after the linkage row lands |
| 2.3 | integration | `test_replayed_run_id_is_idempotent` | Re-delivering one run id leaves the row count unchanged |
| 2.4 | integration | `test_lint_run_does_not_displace_deploy_result` | A later lint failure leaves the deploy answer unchanged |
| 2.5 | unit | `test_stamp_path_has_no_production_caller` | The retired stamp symbol has zero non-test references |
| 3.1 | integration | `test_foreign_installation_records_nothing` | A foreign installation leaves zero rows and answers `UZ-REPAIR-014` |
| 3.2 | integration | `test_fork_head_is_refused` | A fork head leaves zero rows and answers `UZ-REPAIR-014` |
| 3.3 | unit | `test_provenance_refusal_returns_uz_repair_014` | The refusal carries the registered code, not a bare 403 |
| 3.4 | integration | `test_own_repair_pr_still_links` | The repairer's own Pull Request records exactly one row |
| 4.1 | integration | `test_mint_increments_spend_count` | One mint moves the gate row's count from zero to one |
| 4.2 | integration | `test_mint_past_ceiling_refuses` | The mint after the ceiling answers `UZ-REPAIR-013` and issues no token |
| 4.3 | integration | `test_concurrent_mints_respect_ceiling` | At ≥100 concurrent leases, tokens issued never exceed the ceiling |
| 4.4 | unit | `test_card_states_spend_ceiling` | The rendered card names the ceiling the approval funds |
| 5.1 | integration | `test_verifier_onboards_as_crew_member` | The bundle onboards into its own catalogue row |
| 5.2 | integration | `test_verifier_token_is_read_only` | The real mint returns a binding with no write permission |
| 5.3 | integration | `test_verifier_wakes_on_deploy_result_only` | A deploy result wakes it; a Pull Request opening does not |
| 6.1 | unit | `test_vercel_failure_normalises_to_incident` | A failed deploy payload yields the incident envelope the GitHub intake produces |
| 6.2 | unit | `test_vercel_success_is_ignored` | A successful deploy yields an ignore reason |
| 6.3 | integration | `test_vercel_unsigned_delivery_refused` | An unsigned delivery is refused before parsing |
| 7.1 | e2e | `test_live_repair_arc_opens_one_draft_pr` | Against a live disposable repository, the arc ends in exactly one draft Pull Request |
| 7.2 | e2e | `test_live_acceptance_cleans_up` | After the run, no repair branch, Pull Request, or row remains |
| regression | integration | `test_m157_002_write_gate_unchanged` | `UZ-REPAIR-010/011/012` behaviour is byte-identical to M157_002 |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Linkage is recorded on the ingress route (§1) | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_ingress_repair_pr_records_linkage` | `1 passed` | P0 | |
| R2 | Runs accumulate instead of overwriting (§2) | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_lint_run_does_not_displace_deploy_result` | `1 passed` | P0 | |
| R3 | Provenance refusal is typed (§3) | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_foreign_installation_records_nothing` | `1 passed` | P0 | |
| R4 | The ceiling holds under concurrency (§4) | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_concurrent_mints_respect_ceiling` | `1 passed` | P0 | |
| R5 | The verifier cannot write (§5) | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_verifier_token_is_read_only` | `1 passed` | P0 | |
| R6 | Vercel failures raise an incident (§6) | `make test-unit-agentsfleetd` | exit 0 | P1 | |
| R7 | The live arc opens exactly one draft Pull Request (§7) | `make test-acceptance-repair-live` | exit 0 | P1 | |
| R8 | M157_002's write gate is unchanged | `zig-out/bin/agentsfleetd-integration-tests --test-filter test_m157_002_write_gate_unchanged` | `1 passed` | P0 | |
| R9 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** N/A — no files deleted; the stamp path is removed from within `repair_pr_links.zig`.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `stampDeploy` | `grep -rn -w "stampDeploy" src/ \| grep -v _test` | 0 matches |
| `DEPLOY_STATUS_OK` | `grep -rn -w "DEPLOY_STATUS_OK" src/ \| grep -v _test` | 0 matches |
| `deploy_stamped_at` | `grep -rn -w "deploy_stamped_at" src/ schema/ \| grep -v 830_` | 0 matches |

## Out of Scope

- Reverting a failed repair. The design is forward-only; a bad fix is answered by another fix, and the revert path stays dead.
- A model-run verdict on whether the code is correct. §5 judges whether the incident cleared, from telemetry and run rows — not whether the diff is good.
- Repair across more than one repository per incident. One incident, one repository, one Pull Request stays the shape.
- Automatic merge of a verified repair. A human reviews the bytes; that boundary does not move.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator opens the incident, sees the repair Pull Request beside it, and sees that the deploy that followed it succeeded — without having opened GitHub.
2. **Preserved user behaviour** — Approving a write card still releases exactly one run. The repairer still opens one draft Pull Request. A human still merges. `UZ-REPAIR-010/011/012` keep their current meanings.
3. **Optimal-way check** — The direct route is a linkage row every installation gets and a result row per run. The gap to unconstrained-optimal is that "did it work" is inferred from a deploy conclusion rather than from the incident's own telemetry clearing; §5 closes that, and until it lands the inference is stated rather than hidden.
4. **Rebuild-vs-iterate** — Iterate. The shipped shape is right; what is missing is coverage on the second route and durability of the result. A rebuild would trade away the determinism M157_002 established.
5. **What we build** — A shared linkage arm both routes call, append-only run rows, a provenance check, a spend counter, a read-only verifier member, a Vercel intake, and one live acceptance run.
6. **What we do NOT build** — Revert. Multi-repository repair. Auto-merge. A model judgement on diff quality. Each is a different product decision, not a gap in this one.
7. **Fit with existing features** — Compounds with the approval gate and the fleet library. The one feature it must not destabilise is the write mint: `UZ-REPAIR-010/011` behaviour is regression-tested unchanged.
8. **Surface order** — Application Programming Interface (API) first; the operator surface reads rows the daemon writes. No command-line surface changes.
9. **Dashboard restraint** — The deploy-result column shows nothing until a run row exists. No "verified" badge before §5 lands and no confidence claim on top of a single deploy conclusion.
10. **Confused-user next step** — An empty deploy result names why: no run yet, no linkage row, or provenance refused. Each is a distinct ignore reason already carried on the response.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Two batches in one workstream. B1 (§1–§4) hardens what M157_002 shipped and its four slices are mutually independent, so they parallelise. B2 (§5–§7) extends the loop and reads B1's run rows, so it follows.
- **Alternatives considered:** Splitting B2 into its own workstream (M157_004). Rejected for now because the verifier's whole input is B1's run rows — specifying them apart invites the two to drift on the shape of the very table that joins them. If B2 grows past the verifier's first working shape, that split becomes correct and should be taken rather than stretching this spec.
- **Patch-vs-refactor verdict:** this is a **patch** for §1, §3 and §4, and a **refactor** for §2 — the mutable outcome pair is replaced rather than extended, because adding workflow identity to a single mutable row would keep last-write-wins and only make it harder to see.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: to be recorded as the work proceeds.
- **Metrics review** — to be recorded at CHORE(close): events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/write-integration-test`, `/review`, `kishore-babysit-prs` results to be recorded per `AGENTS.md` CHORE(close).
- **Deferrals** — the four items this workstream inherits were deferred from PR #591 with Indy's ack:
  > Indy (2026-08-10 11:12): "Yes deferrals scope it in M157_003 in the main work tree" / "defferals i have answered above in 2" — context: the four deferrals recorded in PR #591 Session notes (ingress-path linkage, workflow-identity stamps, Pull Request provenance, approval spend ceiling), plus the verifier member, Vercel intake, and live-repo acceptance run, all scoped here rather than shipped in M157_002.
