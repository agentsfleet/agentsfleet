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

# M201_001: A billing charge still names its fleet after the fleet is purged

**Prototype:** v0.49.0
**Milestone:** M201
**Workstream:** 001
**Date:** Sep 19, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — an operator reconciling a bill cannot tell which fleet spent what once that fleet is deleted, and no other surface answers it.
**Categories:** API, SQL, UI
**Batch:** B1 — schema precedes the writers, the writers precede the readers; one stream, no parallel context.
**Branch:** `feat/m201-ledger-fleet-identity`
**Baseline revision:** e9bd5c2b2dfbb654647ea37b3720936d878a3ee3
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5, Sep 19, 2026)
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §1

---

## Overview

**Goal (testable):** A `billing.usage_ledger` row survives its fleet's purge carrying both `fleet_id` and `fleet_name`, so the charges table renders the fleet's callsign and chosen name instead of `DELETED AGENT`.

**Problem:** Delete a fleet and every charge it ever incurred starts rendering as `DELETED AGENT` in Settings → Billing. An operator reconciling last month's spend cannot tell which fleet the money went to. The charge row itself survives the purge — that is deliberate, so the wallet reconciles — but it keeps only `model` and `charge_type`, and the `fleet_id` that would name the fleet is nulled on the way out. There is no second surface that answers the question, so the spend is real and unattributable.

**Solution summary:** Two additive changes to `billing.usage_ledger`, and nothing else. First, drop the `fleet_id` foreign key so the purge's `ON DELETE SET NULL` stops firing and the identifier survives the fleet — the web app's existing callsign derivation then works on a historical row exactly as it does on a live one. Second, add `fleet_name TEXT`, captured at charge time from `core.fleets.name`, so the operator-chosen name survives too and the row reads `AGENT NOVA · deploy-bot`. The purge itself is untouched: memory, approval gates, integration grants, sessions and the fleet row are destroyed exactly as they are today, and complete wipeout remains the product's default.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(billing): a charge names its fleet after the fleet is gone
- **Intent (one sentence):** An operator reading their bill can attribute every charge to the fleet that incurred it, including fleets they have since deleted.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `schema/720_usage_ledger_indexes.sql` — documents every index by the query it serves, and names "Reader 2 — the fleet SET NULL" as half the reason `idx_usage_ledger_fleet_id_workspace_id_last_charged_at` leads with `fleet_id`. §1 removes that reader; the comment is the thing that has to change with it.
2. `rustd/crates/afd_fleet_lifecycle/src/purge.rs` — the purge's module header states what survives and why. `billing.usage_ledger` is already called out as surviving deliberately. This spec does not change the purge; read it to confirm that.
3. `docs/SCHEMA_CONVENTIONS.md` — the forward-migration rule this repository runs under: the datastore is live from 0.30.0, so a change is a new 9xx slot applied forward, never an edit to a shipped file.
4. `rustd/crates/afd_billing/src/tenant/mod.rs` — the charge row struct and its `try_get` decode, the pattern `fleet_name` mirrors on the wire.
5. `ui/packages/app/lib/fleets/agent-label.ts` — the one place a fleet's display label is composed, and where `DELETED_AGENT_LABEL` is returned today.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/915_usage_ledger_retains_fleet_identity.sql` | CREATE | The forward slot: drops the `fleet_id` foreign key, adds `fleet_name TEXT`. |
| `schema/720_usage_ledger_indexes.sql` | EDIT | Comment only. The fleet index loses Reader 2 and keeps Reader 1; the rationale has to say so or it documents a constraint that no longer exists. |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | Registers slot 915 in the forward array. |
| `rustd/crates/afd_db/tests/migrations.rs` | EDIT | Pins the two claims slot 915 makes about itself: that `720`'s rationale stops citing the reader it lost, and that the constraint is dropped by catalogue lookup rather than by a guessed name. |
| `rustd/crates/afd_billing/src/sql.rs` | EDIT | `INSERT_USAGE_LEDGER` captures `fleet_name` by subselect from the fleet `$4` already names — no new bind, so no caller can supply it. Carries its own statement-shape test. |
| `rustd/crates/afd_fleet/src/lease/sql/report.rs` | EDIT | The report path's ledger insert gains `fleet_name` by scalar subselect, not a join — `probe` takes `FOR UPDATE OF l, a` and a lock-taking statement in the money path is the wrong thing to perturb. |
| `rustd/crates/afd_fleet/src/lease/sql/renew.rs` | EDIT | The renewal path's ledger insert, same change — and the accumulate clause that must not list the column. |
| `rustd/crates/afd_fleet/src/lease/sql/mod.rs` | EDIT | Holds the invariant both charging statements share: each captures the name, and neither accumulate path re-stamps it. Here because the rule is about the pair, and because `report.rs` sits within fifty lines of the length cap. |
| `rustd/crates/afd_billing/src/tenant_sql.rs` | EDIT | Both charge-list statements select the new column. |
| `rustd/crates/afd_billing/src/tenant/mod.rs` | EDIT | Charge row struct gains `fleet_name: Option<String>` and its decode. |
| `ui/packages/app/lib/api/tenant_billing.ts` | EDIT | The charge type gains the nullable field. |
| `ui/packages/app/lib/fleets/agent-label.ts` | EDIT | `agentDisplayName` takes the stored name and uses it instead of `DELETED_AGENT_LABEL` when the identifier is gone. |
| `ui/packages/app/components/domain/AgentLabel.tsx` | EDIT | Optional `fleetName` prop, forwarded to the composer. Absent prop preserves today's rendering for the approvals and events callers. |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` | EDIT | The only caller that passes the new prop. |
| `rustd/crates/afd_billing/src/tenant/mod.rs` tests | EDIT | Decode coverage for a null and a populated `fleet_name`. |
| `ui/packages/app/tests/billing-charge-cell.test.tsx` | EDIT | The label's three states: live, purged-with-name, purged-without-name. |
| `ui/packages/app/tests/identity-and-controls.test.tsx` | EDIT | Regression: the approvals and events callers still render the callsign with no prop passed. |
| `rustd/crates/afd_fleet_lifecycle/tests/` | EDIT | Integration proof that a purge leaves the ledger row addressable and destroys everything else. |
| `docs/AUTH.md` | EDIT | The `AUTH_SESSION_CODE_PEPPER` row barred disk outright while local development requires the value in a file for `docker-compose` to boot. Records the single permitted path, its mode, and why a real file inside a checkout is a defect. Folded in at the owner's direction — see Discovery. |
| `docker-compose.yml` | EDIT | The daemon's `env_file` path becomes `${AGENTSFLEETD_ENV_FILE:-…}`, so an operator may point at the machine-level source directly instead of the per-worktree symlink. Default preserved, so an unset variable behaves as before. Folded in at the owner's direction — see Discovery. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (the new column's absent value is a named constant on both sides, never a bare literal), **NDC** (no dead code: `DELETED_AGENT_LABEL` keeps exactly one reader after §3 or it goes), **NLR** (touch-it-fix-it: the `720` rationale comment is stale the moment §1 lands and is fixed in the same commit), **ORP** (orphan sweep at CHORE(close)).
- **`docs/SCHEMA_CONVENTIONS.md`** — forward-only 9xx slot; no edit to a shipped migration; RULE STS has no bearing here because no vocabulary column is added.
- **`docs/RUST_ERROR_STANDARD.md`** — the decode path adds a fallible read; it uses the crate's existing `ErrorKind` and `error_shell!`-generated `Error`, never a hand-written error type.
- **`dispatch/write_rust.md`** — fires on every `*.rs` edit in this diff.
- **`dispatch/write_sql.md`** — fires on `schema/*.sql`; carries the Schema Table Removal Guard, which this diff triggers by dropping a constraint.
- **`dispatch/write_ts_adhere_bun.md`** — fires on the `*.ts` / `*.tsx` edits.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — the diff drops a constraint and edits the migration array | The drop is declared here, in this table and in §1, before the edit. New forward slot, shipped files untouched apart from the `720` comment. |
| UI GATE | yes — three `*.tsx` / `*.ts` files under `ui/packages/app` | No raw HTML introduced; the label stays inside the existing `EYEBROW_CLASS` span and the design-system primitives already in use. |
| DESIGN TOKEN GATE | yes — same files | No new utility classes. The separator between callsign and name reuses an existing token; no `*-[...]` arbitrary value. |
| UFS GATE | yes — a new absent-value sentinel crosses Rust and TypeScript | Named constants both sides; no bare `null` comparison scattered through the render path. |
| LENGTH GATE (≤350 file / ≤50 function) | yes | `tenant/mod.rs` and `720_usage_ledger_indexes.sql` are the two nearest a cap; measure before editing and split the comment block rather than the code if `720` crosses. |
| MILESTONE-ID GATE | yes | Every new test name and migration comment carries `M201`. |
| LOGGING GATE | no | No new log line. The insert already reports its own failure through the billing error path. |
| ZIG GATE / PUB GATE | no | No `*.zig` in the diff. |
| ERROR REGISTRY | no | No new error code; the decode failure reuses the existing unreadable-row path. |

## Prior-Art / Reference Implementations

- **Reference:** `billing.usage_ledger.token_count_cached_input` (`schema/710`) — the precedent for this exact move, and its comment makes the argument already: "Carried for auditability, not a query reader — the one column here that earns its place that way." `fleet_name` is the second column with that job, added for the same reason.
- **Reference:** `core.fleet_approval_gates.resolved_by_name` (`schema/838`) — a name snapshotted beside an identifier so a historical row stays readable after the referent moves. Mirror its nullability and its write-time capture; diverge only in that this one is captured on insert rather than on resolution.
- **Reference:** `ui/packages/app/lib/fleets/agent-label.ts` — the composer is already the single place a label is built, so §3 adds an argument rather than a second spelling. No new module.

## Sections (implementation slices)

### §1 — The ledger keeps the identifier

The purge nulls `fleet_id` because a foreign key says it must. Dropping the constraint keeps the UUID, which is all the web app's callsign derivation needs — `AGENT NOVA` becomes derivable on a purged row without any new data at all. This slice alone removes `DELETED AGENT` from every row charged after it lands. **Implementation default:** `DROP CONSTRAINT` and keep the column typed `UUID` rather than converting to `TEXT`, because every reader already binds it as a UUID and the budget drain's index depends on the type.

- **Dimension 1.1** — Slot 915 drops the `fleet_id` foreign key and the column survives a fleet purge with its value intact → Test `test_m201_ledger_retains_fleet_id_across_purge`
- **Dimension 1.2** — `idx_usage_ledger_fleet_id_workspace_id_last_charged_at` still serves the budget drain after the constraint is gone, with no plan regression → Test `test_m201_budget_drain_plan_unchanged`
- **Dimension 1.3** — DONE — The `720` rationale names one reader, not two, and no longer cites a referential action that does not exist → Test `test_m201_index_comment_names_surviving_reader`
- **Dimension 1.4** — The purge destroys memory, approval gates, integration grants and sessions exactly as before → Test `test_m201_purge_destroys_no_less_than_before`

### §2 — The ledger captures the name

The identifier gives a callsign; it does not give the name the operator typed. `fleet_name` is captured at charge time from `core.fleets.name`, which makes it a snapshot rather than a lookup — a fleet renamed after a charge leaves the old charge reading the old name, and that is correct for a ledger. **Implementation default:** capture on insert via the join already available in each statement's driving CTE, rather than backfilling at read time, because a read-time join cannot resolve a row whose fleet is gone — which is the entire problem.

- **Dimension 2.1** — Slot 915 adds `fleet_name TEXT`, nullable, with no default → Test `test_m201_ledger_carries_fleet_name_column`
- **Dimension 2.2** — All three insert sites write the name at charge time from the fleet row → Test `test_m201_all_insert_sites_capture_fleet_name`
- **Dimension 2.3** — DONE — The `ON CONFLICT` accumulate path leaves an already-written `fleet_name` intact rather than overwriting it on every renewal → Test `test_m201_accumulate_preserves_captured_name`
- **Dimension 2.4** — A fleet renamed after a charge does not retroactively change that charge's stored name → Test `test_m201_rename_does_not_rewrite_history`

### §3 — The surfaces read it

The charge list carries the field; the label composer prefers the stored name when the callsign cannot be derived. Every other `AgentLabel` caller is untouched and keeps rendering callsigns. **Implementation default:** an optional prop with today's behaviour as the default, rather than a required one, because the approvals and events tables have no name to pass and changing them is not this spec's job.

- **Dimension 3.1** — Both charge-list statements select `fleet_name` and the row struct decodes it → Test `test_m201_charge_row_decodes_fleet_name`
- **Dimension 3.2** — The billing table renders callsign and name together for a purged fleet that has both → Test `test_m201_billing_renders_callsign_and_name`
- **Dimension 3.3** — A pre-migration row with neither identifier nor name still renders the deleted label rather than an empty cell → Test `test_m201_legacy_row_renders_deleted_label`
- **Dimension 3.4** — Approvals and events tables render unchanged with no prop passed → Test `test_m201_other_label_callers_unchanged`

## Interfaces

```
GET /v1/tenants/me/billing/charges

Response item gains one nullable field; every existing field is unchanged.

{
  "id":            "0199...",
  "tenant_id":     "0199...",
  "workspace_id":  "0199...",
  "fleet_id":      "0199...",   // NOW: survives the fleet's purge. Was null after delete.
  "fleet_name":    "deploy-bot", // NEW: nullable. Null for rows charged before slot 915,
                                 // and for a charge whose fleet row was unreadable.
  "event_id":      "...",
  "charge_type":   "receive",
  "model":         "claude-opus-5",
  ...
}

Internal signature change, TypeScript:

  agentDisplayName(fleetId: string | null, fleetName?: string | null): string

  fleetId non-null              → `AGENT ${callsign}`           (unchanged)
  fleetId null, fleetName set   → the stored name
  fleetId null, fleetName null  → DELETED_AGENT_LABEL           (unchanged)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Pre-migration purged row | The fleet was deleted before slot 915 landed; both identifier and name are already gone and nothing can recover them | Renders `DELETED AGENT`, exactly as today. The migration does not pretend to backfill history. Stated in the changelog so an operator is not left wondering why old rows differ. |
| Fleet row unreadable at charge time | The driving CTE finds no fleet row for the lease | `fleet_name` is written null; `fleet_id` is still written; the label falls back to the callsign. A charge is never dropped for want of a name. |
| Name written, identifier absent | A future path writes the name without the identifier | Label renders the stored name. No crash, no empty cell. |
| Renewal overwrites the captured name | The `ON CONFLICT DO UPDATE` accumulate path re-binds `fleet_name` on every renewal | The update clause does not list `fleet_name`, so the first capture stands. Asserted by Dimension 2.3, because the opposite is the natural thing to write. |
| Orphan identifier written | Nothing enforces referential integrity once the constraint is dropped | Accepted, bounded: `fleet_id` is server-derived from the lease and never client-supplied. A bogus value cannot be introduced from outside the daemon. Recorded as an Invariant, not a guard. |
| Budget drain plan regresses | The index was justified partly by a reader this spec removes, and an over-eager cleanup drops it | The index stays. Dimension 1.2 asserts the plan, not the index definition. |

## Invariants

1. **The purge destroys no less than it does today** — enforced by an integration test that counts memory, approval-gate, grant and session rows after a purge and requires zero, not by reading the diff.
2. **`fleet_name` is server-captured** — enforced by construction: every write site sources it from `core.fleets.name` inside the same statement, and no request field binds to it.
3. **`fleet_id` on this table is server-derived** — enforced upstream by the lease, which is where the identifier comes from; dropping the constraint does not open a client-supplied path because none exists.
4. **The captured name is a snapshot, not a mirror** — enforced by the `ON CONFLICT` clause omitting `fleet_name`, asserted by Dimension 2.4.
5. **Slot 915 is forward-only** — enforced by `afd_db`'s migration array deriving its slot from the filename at constant evaluation; a shipped file edited in place would not change the applied set.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | This spec adds a column and a label fallback; it adds, renames and removes no analytics event, no funnel timer and no operator metric. The billing surface's existing telemetry is untouched. | not applicable | The new field carries an operator-chosen fleet name, which is already returned by `/fleets` to the same authenticated reader; no new class of data reaches the client. | not applicable |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_m201_ledger_retains_fleet_id_across_purge` | Charge a fleet, kill it, purge it → the ledger row's `fleet_id` equals the purged fleet's identifier, not null. |
| 1.2 | integration | `test_m201_budget_drain_plan_unchanged` | `EXPLAIN` the budget drain after slot 915 → plan uses `idx_usage_ledger_fleet_id_workspace_id_last_charged_at`, zero sequential scans on `usage_ledger`. |
| 1.3 | unit | `test_m201_index_comment_names_surviving_reader` | Grep `schema/720` → zero occurrences of "SET NULL" in the fleet index's rationale block. |
| 1.4 | integration | `test_m201_purge_destroys_no_less_than_before` | Fleet with memory, two gates, one grant and a session, purged → all four counts zero; ledger count unchanged. |
| 2.1 | unit | `test_m201_ledger_carries_fleet_name_column` | Applied schema → `billing.usage_ledger.fleet_name` exists, is `text`, is nullable, has no default. |
| 2.2 | integration | `test_m201_all_insert_sites_capture_fleet_name` | Drive a receive charge, a report charge and a renewal against a fleet named `deploy-bot` → all three rows carry `deploy-bot`. |
| 2.3 | integration | `test_m201_accumulate_preserves_captured_name` | Charge, rename the fleet, renew the same `event_id` → the row still reads the original name. |
| 2.4 | integration | `test_m201_rename_does_not_rewrite_history` | Charge under name A, rename to B, read the charge list → the historical row reads A, a fresh charge reads B. |
| 3.1 | unit | `test_m201_charge_row_decodes_fleet_name` | Row with `fleet_name` null and row with it set → decodes to `None` and `Some("deploy-bot")`; neither errors. |
| 3.2 | e2e | `test_m201_billing_renders_callsign_and_name` | Settings → Billing with a purged fleet's charge → the cell shows the callsign and the stored name; `DELETED AGENT` appears zero times. |
| 3.3 | unit | `test_m201_legacy_row_renders_deleted_label` | `agentDisplayName(null, null)` and `agentDisplayName(null, undefined)` → `DELETED AGENT` both times, never an empty string. |
| 3.4 | unit | `test_m201_other_label_callers_unchanged` | Approvals and events tables rendered with no `fleetName` prop → identical output to the pre-change snapshot. |
| 3.3 | unit | `test_m201_display_name_rejects_blank_name` | `agentDisplayName(null, "")` and `agentDisplayName(null, "   ")` → `DELETED AGENT`, not a blank cell. |
| 2.2 | integration | `test_m201_charge_survives_unreadable_fleet_row` | Charge driven where the fleet row cannot be read → the ledger row is written, `fleet_name` null, no error raised to the caller. |
| 2.1 | unit | `test_m201_migration_slot_registered` | `afd_db`'s migration array → contains `915_usage_ledger_retains_fleet_identity.sql` exactly once, after 914. |
| 2.1 | integration | `test_m201_upgrade_from_populated_ledger` | Apply slot 915 to a database holding pre-existing charge rows → every existing row keeps its values, `fleet_name` reads null, no row is lost. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A purged fleet's charge carries its identifier (§1) | `cargo test -p afd_fleet_lifecycle test_m201_ledger_retains_fleet_id_across_purge -- --ignored` | exit 0 | P0 | |
| R2 | The purge still destroys memory, gates, grants and sessions (§1) | `cargo test -p afd_fleet_lifecycle test_m201_purge_destroys_no_less_than_before -- --ignored` | exit 0 | P0 | |
| R3 | All three insert sites capture the name (§2) | `cargo test -p afd_billing -p afd_fleet test_m201 -- --ignored` | exit 0 | P0 | |
| R4 | The billing cell no longer reads DELETED AGENT for a named purged fleet (§3) | `cd ui/packages/app && bun run test billing-charge-cell` | exit 0, `DELETED AGENT` asserted absent | P0 | |
| R5 | The budget drain's plan did not regress (§1) | `cargo test -p afd_billing test_m201_budget_drain_plan_unchanged -- --ignored` | exit 0 | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3a | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Integration green (live Postgres + Dragonfly) | `make test-integration-rustd` | exit 0 | P0 | |
| S3c | Version synced | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may also be **MOVED** — see below.

**A P0 whose SCOPE moves is not a P0 shipped red.** Met and unmet are not the only two states a criterion has, and a gate that pretends otherwise forces an agent to invent a third. One did, twice in a day, before this clause existed.

A deferral and a transfer are different claims. A **deferral** leaves work unowned inside a closed spec, which is what the P0 gate exists to prevent — the P1 quote is as far as that goes. A **transfer** moves the criterion whole: its Dimensions, its verification and its rubric row land in a named successor spec that carries them as its own P0. Nothing is less owned afterwards; it is owned somewhere else.

Mark such a row `MOVED to M{N}_{NNN} R{n}` and it is not ❌, on three conditions, all of which must hold:

1. The successor spec **exists** and carries the criterion as a rubric row of its own. A successor that does not carry the row is a deferral wearing a new word, and fails the gate as before.
2. Both specs record the mapping — the closing spec names where each Dimension went, the successor names what it inherited. One-sided assertion is not a transfer.
3. Discovery carries the **owner's verbatim quote** authorising it, in the deferral format. An agent-authored transfer is agent-authored scope reduction.

A MOVED row is never rendered ✅. The criterion has not been met; it has changed owner, and the rubric says which.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `DELETED_AGENT_LABEL` (must keep exactly one reader, not zero — RULE NDC) | `grep -rn -w "DELETED_AGENT_LABEL" ui/packages/app --include="*.ts" --include="*.tsx" \| grep -v node_modules` | ≥1 match in `agent-label.ts`, ≥1 in its test; zero elsewhere |
| `usage_ledger_fleet_id_fkey` | `grep -rn -w "usage_ledger_fleet_id_fkey" schema/ rustd/` | 1 match, in slot 915 only |

## Out of Scope

- **The Command-Line Interface's delete confirmation.** `agentsfleet delete` destroys a fleet's memory printing six words and asking nothing, while the web app requires a dialog that names the memory loss. That asymmetry is real and was found while authoring this spec. It is not fixed here — it was never requested, and bundling it would put user-facing copy into a schema change. Follow-up work.
- **Retiring the `AGENT` display vocabulary.** `AgentLabel`, `agent-label.ts`, `agentDisplayName`, the `AGENT` prefix and the `data-agent-name` attribute are stale legacy-brand spellings; this repository's entities are `fleet`. Renaming them touches three tables' render paths and an identity system with a "never reorder these buckets" constraint. Flagged, not folded — see Discovery.
- **Backfilling already-purged rows.** Their identifiers are gone from the database; nothing can recover them. Pre-migration rows keep reading `DELETED AGENT` and the changelog says so.
- **Any change to what the purge destroys.** Complete wipeout stays the default. Approval gates, memory, grants and sessions are destroyed exactly as today.
- **A fleet tombstone table, and audit rows surviving deletion.** Considered and rejected during authoring — it adds a second lifetime to maintain for a question this column answers.

## Product Clarity (authoring record)

1. **Successful user moment** — An operator opens Settings → Billing at month end, sees `AGENT NOVA · deploy-bot — $412.80` on a fleet they deleted three weeks ago, and closes the tab without filing a support ticket.
2. **Preserved user behaviour** — Deleting a fleet still destroys its memory, approvals, grants and sessions in one irreversible step after a kill. Live fleets still render as `AGENT {callsign}` everywhere they do today. The approvals and events tables render identically. Breaking any of these is a redesign, not this spec.
3. **Optimal-way check** — Direct. The gap to the unconstrained-optimal shape is that the ledger now holds a name that duplicates `core.fleets.name` for live fleets, which is denormalisation. Acceptable because a ledger is the one place a snapshot beats a join: the referent is expected to disappear, and `token_count_cached_input` already set this precedent on this table for the same reason.
4. **Rebuild-vs-iterate** — Iterate. The larger shape — a fleet tombstone table with audit rows re-pointed at it — was drafted and rejected: it introduces a second retention lifetime, a redaction path, and a policy question about what erasure means, to answer a question one nullable column answers. Determinism is unaffected either way.
5. **What we build** — One forward migration, three insert-site edits, one selected field, one optional prop, one label fallback.
6. **What we do NOT build** — No tombstone table. No redaction tier. No soft-delete window. No change to the purge. No CLI copy change. No brand rename. Each seeds an Out of Scope row above.
7. **Fit with existing features** — Compounds with the charge list and the callsign identity system, both of which keep working unchanged for live fleets. The one thing it must not destabilise is the budget drain: it runs on every event receive and every renewal, reads this table through the index this spec's constraint drop re-justifies, and a plan regression there is a money-path regression. Dimension 1.2 exists for that alone.
8. **Surface order** — Both, because the surfaces differ in kind rather than in order: the schema and capture are backend-only, and the sole reader is the web app's billing table. The CLI has no charge-list surface to update.
9. **Dashboard restraint** — Nothing new is shown until it is real. A purged fleet with no captured name shows the existing deleted label rather than an empty cell or a placeholder, and no filter, sort or grouping control is added on the new field — there is no evidence yet that anyone wants to slice a bill by fleet name.
10. **Confused-user next step** — An operator who sees `DELETED AGENT` on an old row reads the changelog entry naming the cutover: rows charged before this landed cannot be attributed, and rows after it can. Self-serve, no ticket.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Three Sections in strict dependency order — the column must exist before anything writes it, and something must write it before a surface reads it. Splitting further would produce Sections that cannot be verified independently; merging them would put a schema change and a render change in one commit.
- **Alternatives considered:** (a) **Identifier only** — drop the foreign key, add no column. Cheapest, and it restores the callsign, but it never yields the name the operator chose, so a bill reads `AGENT NOVA` where the operator thinks in `deploy-bot`. (b) **Name only** — add `fleet_name`, keep the foreign key. Yields the name but leaves live and historical rows spelled differently, since live rows render callsigns. (c) **The tombstone tier** — a `core.fleet_tombstones` table with approvals and grants re-pointed at it on purge. Rejected: it answers a broader question nobody asked, and it commits the product to a retention policy that should be decided deliberately rather than as a side effect of a billing fix.
- **Patch-vs-refactor verdict:** this is a **patch** because the problem is one missing fact on one row, not a wrong structure. The refactor alternative (c) is a real design worth its own spec if the question ever becomes "what survives deletion, as policy" — it is named in Out of Scope so it cannot be mistaken for something this spec silently absorbed.

## Discovery (consult log)

- **Consults** — Architecture consult against `docs/architecture/memory.md` §1–§2 (Sep 19, 2026): memory is `fleet_id`-keyed with no workspace key, and cascade-erased with the fleet by design. Confirmed this spec must not decouple memory from fleet lifetime; the doc's shape stands and nothing here changes it. Source read of `schema/720_usage_ledger_indexes.sql` established that `idx_usage_ledger_fleet_id_workspace_id_last_charged_at` serves two readers, of which the fleet `SET NULL` is one — the comment states Reader 1 "is indifferent to the order of the two equality columns", which is why dropping the constraint leaves the index correct with a stale rationale rather than a wrong definition. Source read of `rustd/crates/afd_fleet_lifecycle/src/sql/purge.rs` and `purge.rs` confirmed `billing.usage_ledger` already survives the purge deliberately, so this spec extends a documented decision rather than reversing one.
- **Legacy-brand flag (RULE NLG-adjacent, `AGENTS.orly.md` §Owner & Style)** — `AgentLabel`, `lib/fleets/agent-label.ts`, `agentDisplayName`, the `AGENT` display prefix and the `data-agent-name` attribute are stale legacy-brand spellings in a repository whose entities are `fleet`. Surfaced to Indy at authoring; not folded, because the rename crosses three render paths and an identity module carrying a "never reorder them" versioning constraint. Recorded in Out of Scope so it is neither lost nor silently absorbed.
- **Scope folded in by the owner (Sep 19, 2026)** — `docs/AUTH.md` joined this workstream's Files Changed on the owner's instruction:

  > Indy (2026-09-19): "AUTH.md must in this PR" — context: the `AUTH_SESSION_CODE_PEPPER` row listed `disk` as forbidden while local development requires the value in a file for `docker-compose`'s `env_file` to boot. Found while diagnosing why `.env.agentsfleetd.local` was not symlinked into new worktrees.

  Two findings from that diagnosis are recorded here because they have no other home. **One:** the file was a real file rather than a symlink in the base checkout, at mode 0644 — a catastrophic-if-disclosed value world-readable on the machine. Hardened to 0600, and the AUTH.md note now names that shape as a defect. **Two:** `provision-env-1password` (dotfiles) writes only `ui.env.local` and `runner.env.local`, so the `agentsfleetd.env.local` source the hook links was never created; every knob in `preflight/knobs.rs` was audited and all are live, while `AUDIT_LOG_PEPPER` — retired in M196_001 and carried by the local file alone — was stripped with the owner's approval. The provisioner change itself lands in the dotfiles repository, not here.

- **Metrics review** — No analytics or funnel playbook update required: this spec adds, renames and removes no product or operator event. The one new wire field is an operator-chosen fleet name already returned by `/fleets` to the same authenticated reader.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test` at each Section and again at the boundary, `/review` before DOCUMENT, `orly-babysit-prs` after every push.
- **Deferrals** — none at authoring. Every item removed from scope sits in Out of Scope as work never started, not work abandoned midway.
