<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M229_001: Postgres rebuilds from empty into nine owned schemas with one naming rule set

**Prototype:** v2.0.0
**Milestone:** M229
**Workstream:** 001
**Date:** Oct 17, 2026
**Status:** PENDING
**Priority:** P1 — the rebuild is only cheap before the first production launch (M224_001) puts data in Postgres
**Categories:** API, DOCS, INFRA
**Batch:** B1 — single workstream; the schema, every reader and the docs land in one Pull Request (PR), as the owner chose
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none blocking. Ordering: lands before M224_001 puts production data in Postgres; the `fix/754-dev-acceptance-app` branch's slots 956/957 fold into the new slots if it has not merged first
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 17, 2026), from a table-by-table owner review against `main` at `a8646bb2a`
**Canonical architecture:** `docs/architecture/postgres_schemas.md` §Rules, §Layout and one section per schema

---

## Overview

**Goal (testable):** `test_migrate_from_empty_builds_nine_owned_schemas` — `agentsfleetd migrate` on an empty database creates exactly the tables in `docs/architecture/postgres_schemas.md`, no schema object carries a string literal, and the full integration lane passes against it.
**Problem:** 33 of 47 tables sit in `core`; the setup-chat tables span `core`, `fleet` and `billing`. Columns exist with no reader (`drafts.creation_digest`, `wallets.grant_source`), facts are stored twice or three times (`fleets.config_json`, `fleets.setup_version`), and an owner reading the schema cannot tell what a field means or who fills it.
**Solution summary:** delete all 97 `schema/*.sql` slots and author one slot per table across nine schemas (identity, authoring, fleet, runner, connector, model, billing, vault, memory), exactly as signed off in the canonical doc. Every Rust query, struct and test follows the new names. Two wire fields leave the public surface. The development and production databases are dropped and rebuilt from empty under an owner-recorded override. The naming rules land in the schema conventions at their source in the orly repository so the next table starts right.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(schema): rebuild Postgres into nine owned schemas
- **Intent (one sentence):** every table has one obvious home, every column has a reader and one meaning, and the conventions stop the drift recurring.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/postgres_schemas.md` — the signed-off layout, every table's columns, the nine rules and the counter names; the schema files are a transcription of it.
2. `docs/v2/done/M154_001_P1_API_DOCS_SQL_UI_SCHEMA_REBUILD_FOR_SCALE.md` — the previous from-empty rebuild: slot ordering by dependency layer, registry and test mechanics.
3. `.orly/dispatch/write_sql.md` — SCHEMA GUARD, RULE STS, NSQ, SGR and ITF; the guard fires on every slot edit here.
4. `rustd/crates/afd_db/src/migration.rs` — the explicit migration list and the `rust-migration-list` lint that compares it to tracked files.
5. `rustd/crates/afd_db/src/migrate/ledger.rs` — `audit.schema_migrations`; old version numbers collide with new slot numbers.

## Files Changed (blast radius)

Discovery grep (rerun at PLAN, same pattern in the Dead Code Sweep): `git grep -lE "(^|[^a-z_])(core|fleet|billing|vault|memory)\.[a-z_]+"` — 832 tracked files at `a8646bb2a`, of which 92 are schema slots and 142 are `docs/v2` specs.

| File | Action | Why |
|------|--------|-----|
| `schema/*.sql` (97 slots) | DELETE | Clean slate; slot history is not migrated |
| `schema/*.sql` (one per table, numbered by schema block) | CREATE | The signed-off design, dependency-ordered |
| `rustd/crates/afd_db/src/migration.rs` | EDIT | Registry equals the new slot set |
| `rustd/crates/afd_db/src/migrate/**` | EDIT | Refuse a ledger version the registry lacks |
| `rustd/crates/*/src/**/*.rs` with a schema-qualified table name | EDIT | Queries and row structs follow new names; dropped columns lose their writers |
| `rustd/crates/*/tests/**`, `rustd/crates/afd_db/src/test_util/**`, `rustd/crates/afd_bench/**` | EDIT | Fixtures and assertions use the new schema |
| `rustd/crates/afd_wire/src/authoring/**`, `rustd/crates/afd_api_wire/src/authoring/**` | EDIT | Drop `deletion_generation` and the message `operation_id` |
| `public/openapi.json` | EDIT | Regenerated for the two wire removals |
| `ui/packages/app/lib/api/draft-types.ts` | EDIT | Message schema without `operation_id` |
| `cli/src/**`, `scripts/**`, `playbooks/**`, `make/*.mk` | EDIT | Only where a discovery-grep hit names a renamed table |
| `docs/architecture/*.md` | EDIT | Old table names replaced; `postgres_schemas.md` §Settled outcomes |
| `docs/v2/pending/*.md`, `docs/v2/active/*.md` | EDIT | Live specs naming a renamed table; `done/` stays frozen |
| `.orly/docs/SCHEMA_CONVENTIONS.md` | EDIT | Only through `orly update` after the source change |
| `~/Projects/orly/docs/SCHEMA_CONVENTIONS.md` | EDIT | Cross-repository, own branch and PR (owner-approved) |
| `~/Projects/docs/**` | EDIT | Runner protocol and draft message pages, changelog `<Update>`; own branch |
| `HANDOFF_schema_clean_slate.md` | DELETE | Ephemeral sign-off log absorbed by this spec and the canonical doc |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — STS (no string literals in schema objects), NSQ (schema-qualified SQL through named constants), SGR (every slot grants), ITF (integration tests on the real schema), NDC (dropped columns lose every writer), NLR, NLG (no alias for an old table or column name), UFS, ORP (orphan sweep across Rust, TypeScript, docs), VLT (no credential material in rows), KYS (keyset indexes carry the tiebreak).
- `.orly/docs/SCHEMA_CONVENTIONS.md` — identity, timestamps and standard columns, as amended by §1.
- `docs/RUST_ERROR_STANDARD.md` — the migrator's new refusal is an `ErrorKind` arm, never a hand-written error type.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — every slot deletion, `migration.rs` edit; `VERSION` is 0.62.0 | Owner records `SCHEMA GUARD: SKIPPED per user override (reason: …)` before the first slot deletion; guard output lists every removed and created slot |
| File & Function Length (≤350/≤50/≤70) | yes | One table per slot keeps each under 100 lines; split any Rust file a rename pushes over the cap |
| UFS | yes | Status, type and scope values stay named constants bound as parameters |
| MILESTONE-ID | yes | No `M229` in source, test names or slot comments |
| GREPTILE | yes | End-of-turn read of the rules named above |
| LOGGING | yes, where a renamed query logs its table | Event names keep their meaning; only table text changes |

## Prior-Art / Reference Implementations

- **Reference:** `docs/v2/done/M154_001_P1_API_DOCS_SQL_UI_SCHEMA_REBUILD_FOR_SCALE.md` — same from-empty posture and dependency-layer ordering; this rebuild diverges by grouping slots per Postgres schema and by requiring an owner override, because `VERSION` is past 0.30.0.

## Sections (implementation slices)

### §1 — Conventions land at their source

The nine rules and the counter table from the canonical doc land in `~/Projects/orly/docs/SCHEMA_CONVENTIONS.md` on its own branch and PR, together with removing the stale Zig timestamp and `schema/embed.zig` lines. After it merges, `orly update --no-hooks` refreshes the generated copy here. **Implementation default:** the source edit precedes any slot, because the slots are reviewed against it.

- **Dimension 1.1** — generated conventions carry rules 1–9, the counter table, and no Zig or `embed.zig` text → Test `test_conventions_carry_naming_rules`
- **Dimension 1.2** — `orly doctor` reports no drift after the update → Test `test_orly_doctor_clean_after_update`

### §2 — Slots rebuilt from empty

All 97 slots go. New slots follow the canonical doc table by table, one per table, in dependency order: roles, identity, model, vault, connector, fleet, runner, authoring, billing, memory. A circular head pointer (`fleets.head_revision_id`, `drafts.head_candidate_id`, `library_entries.head_revision_id`) gets its foreign key as `DEFERRABLE INITIALLY DEFERRED`, declared in the slot of the second table. **Implementation default:** `fleet.revisions.source_candidate_id` carries no foreign key, because candidates are swept after 30 days.

- **Dimension 2.1** — migrate from empty yields exactly the documented tables and columns → Test `test_migrate_from_empty_builds_nine_owned_schemas`
- **Dimension 2.2** — no index predicate, trigger, CHECK or DEFAULT contains a string literal → Test `test_schema_objects_carry_no_string_literals`
- **Dimension 2.3** — mutable tables carry `updated_at`; immutable tables lack it, lack UPDATE grants and reject UPDATE → Test `test_immutable_tables_reject_update`
- **Dimension 2.4** — no table name repeats its schema; every foreign-key column ends `_id` → Test `test_table_and_key_names_follow_rules`
- **Dimension 2.5** — a ledger holding a version the registry lacks is refused before any slot runs → Test `test_migrate_refuses_unknown_ledger_version`

### §3 — Every reader and writer follows the new names

Every query, row struct and fixture moves to the new tables and columns. Each column in the canonical doc's §Settled list is resolved from source: a cited reader keeps it, otherwise it is dropped, and the outcome is written into that section.

- **Dimension 3.1** — the full integration lane passes on the rebuilt schema → Test `test_integration_lane_on_rebuilt_schema`
- **Dimension 3.2** — every §Settled column carries a recorded outcome with a citation → Test `test_settled_columns_have_outcomes`

### §4 — Behaviour that changes with the schema

- **Dimension 4.1** — closed drafts untouched for 30 days are swept with their messages and candidates; attempts and settlements remain → Test `test_closed_draft_sweep_keeps_money`
- **Dimension 4.2** — a retried request inside the receipt window returns the first result; after its resource is deleted it returns Gone; receipts past the window are swept → Test `test_replay_receipt_window`
- **Dimension 4.3** — deleting a Library card deletes its revisions while a fleet installed from it keeps its own setup → Test `test_library_delete_cascades_revisions`
- **Dimension 4.4** — `tenant provider set` writes through `model.tenant_models`; the selection holds only a pointer, NULL meaning platform default → Test `test_tenant_selection_points_at_model`
- **Dimension 4.5** — runner protocol bodies carry no `deletion_generation`; the draft message response carries no `operation_id` → Test `test_wire_drops_retired_fields`
- **Dimension 4.6** — a resolved approval gate or invite cannot be resolved again; the pending indexes read `resolved_at` → Test `test_resolution_happens_once`

### §5 — Databases rebuilt

Development first, then production, each dropped to empty (including `audit`) and migrated by the release command. Production is rebuilt only while M224_001 is unlaunched, with the owner present.

- **Dimension 5.1** — development and production report the new layout after rebuild → Test `test_deployed_databases_rebuilt` (manual)

### §6 — Docs

- **Dimension 6.1** — no doc outside `docs/v2/done/` names a removed table → Test `test_docs_name_no_removed_table`
- **Dimension 6.2** — public runner protocol and draft message pages and a changelog `<Update>` land on their own docs branch → Test `test_public_docs_branch_open` (manual)

## Interfaces

```
Runner protocol (POST /v1/runners/me/authoring-leases/{lease_id}/{activity|model-calls|renew|reports},
  lease payload): field `deletion_generation` removed.
Draft messages (GET/POST /v1/workspaces/{workspace_id}/drafts/{draft_id}/messages): field
  `operation_id` removed from each message; request bodies unchanged.
CLI: `agentsfleet tenant provider set` unchanged from outside; server writes model.tenant_models.
Tables and columns: docs/architecture/postgres_schemas.md is the interface; renames there are final.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stale ledger | Migrate runs against a database still holding old `audit.schema_migrations` rows | Migrator refuses before any slot with the unknown version named; nothing applied |
| Old name survives | A query still names a removed table or column | Integration lane fails on the undefined relation; discovery grep in the Dead Code Sweep reports it |
| Circular insert order | Head pointer written before its target in one transaction | Deferred foreign key checks at commit; an orphan pointer rolls the transaction back |
| Production holds data | Rebuild attempted after M224_001 launched | §5 stops; owner decides a forward migration instead |
| Late 754 merge | Slots 956/957 land on `main` after this branch opens | Rebase folds them into the new slots; `rust-migration-list` fails until folded |
| Concurrent resolution | Two people resolve one gate or invite | Guarded UPDATE on `resolved_at IS NULL`; second sees zero rows and a conflict |

## Invariants

1. No schema object carries a string literal — catalogue query in `test_schema_objects_carry_no_string_literals` fails the lane.
2. Immutable tables cannot be updated — no UPDATE grant plus an append-only trigger; `test_immutable_tables_reject_update`.
3. The migration list equals the tracked slots — `rust-migration-list` in `make lint-all`.
4. A ledger the registry does not recognise never migrates — refusal arm in `afd_db::migrate`; `test_migrate_refuses_unknown_ledger_version`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | not applicable | not applicable | not applicable | `test_integration_lane_on_rebuilt_schema` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_conventions_carry_naming_rules` | generated conventions file → contains the counter table heading; 0 matches for `embed.zig` |
| 1.2 | manual | `test_orly_doctor_clean_after_update` | `orly doctor` after update → no drift line; owner merges the orly PR |
| 2.1 | integration | `test_migrate_from_empty_builds_nine_owned_schemas` | empty database → table and column set equals the canonical doc, nine schemas plus `audit` |
| 2.2 | integration | `test_schema_objects_carry_no_string_literals` | catalogue scan of index predicates, triggers, CHECKs, DEFAULTs → 0 quoted literals |
| 2.3 | integration | `test_immutable_tables_reject_update` | UPDATE on each append-only table → privilege or trigger error; mutable tables all have `updated_at` |
| 2.4 | integration | `test_table_and_key_names_follow_rules` | catalogue → no table prefixed by its schema; foreign-key columns end `_id` |
| 2.5 | integration | `test_migrate_refuses_unknown_ledger_version` | ledger seeded with version 955 → refusal naming 955; 0 slots applied |
| 3.1 | integration | `test_integration_lane_on_rebuilt_schema` | `make test-integration-rustd` on the rebuilt schema → exit 0 |
| 3.2 | unit | `test_settled_columns_have_outcomes` | each §Settled bullet → carries kept/dropped plus a citation |
| 4.1 | integration | `test_closed_draft_sweep_keeps_money` | closed draft 31 days old → draft, messages, candidates gone; attempt and settlement remain |
| 4.2 | integration | `test_replay_receipt_window` | same key twice → one message; resource deleted → Gone; aged receipt → swept |
| 4.3 | integration | `test_library_delete_cascades_revisions` | card with two revisions, one installed → card and revisions gone; fleet revision intact |
| 4.4 | integration | `test_tenant_selection_points_at_model` | provider set → one `tenant_models` row, selection points at it; reset → NULL pointer row |
| 4.5 | unit | `test_wire_drops_retired_fields` | serialised lease payload and message → no `deletion_generation`, no `operation_id` |
| 4.6 | integration | `test_resolution_happens_once` | two concurrent resolutions → one succeeds, one conflict; second invite for same address while pending → refused |
| 5.1 | manual | `test_deployed_databases_rebuilt` | owner-present rebuild; `agentsfleetd doctor --schema-gate` output pasted in Session Notes |
| 6.1 | unit | `test_docs_name_no_removed_table` | discovery grep over docs outside `done/` → 0 hits |
| 6.2 | manual | `test_public_docs_branch_open` | docs branch URL with the two pages and the `<Update>` recorded in Session Notes |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Schema rebuilds from empty into the documented layout (§2) | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | No code or live doc names a pre-rebuild table (§3, §6) | `git grep -nE "(^\|[^a-z_])core\.[a-z_]+" -- rustd ui cli scripts docs/architecture docs/v2/pending docs/v2/active \| wc -l` | 0 | P0 | |
| R3 | Conventions carry the rules (§1) | `grep -c "Counter and pointer names" .orly/docs/SCHEMA_CONVENTIONS.md` | 1 | P1 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version in sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S7 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. A P1 ❌ requires an owner-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `schema/500_fleets.sql` and every other pre-rebuild slot | `git ls-files schema \| grep -cE '^schema/(5[0-9]{2}\|9[0-9]{2})_'` → 0 |
| `HANDOFF_schema_clean_slate.md` | `test ! -f HANDOFF_schema_clean_slate.md` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `deletion_generation` | `git grep -nw deletion_generation -- rustd ui cli public \| head` | 0 matches |
| `setup_version`, `config_json`, `setup_context` | `git grep -nwE "setup_version\|config_json\|setup_context" -- rustd \| head` | 0 matches |
| `creation_digest`, `grant_source`, `checkpoint_at` | `git grep -nwE "creation_digest\|grant_source\|checkpoint_at" -- rustd \| head` | 0 matches |
| `core.` table names | `git grep -nE "(^\|[^a-z_])core\.[a-z_]+" -- rustd \| head` | 0 matches |

## Out of Scope

- Narrowing `api_runtime` to per-schema runtime roles — its own milestone, as `300_vault_secrets.sql` and `700_tenant_wallet.sql` record.
- A Firecracker or SPACE sandbox tier — `runners.sandbox_config` is ready for its keys; the tier ships with its own spec.
- Renaming wire fields to the new column names (`fence`, `call_index`, `activity_sequence`) — owner chose column names only.
- Moving `authoring.attempts` beside `runner.leases` — owner kept it in `authoring`.

---

## Product Clarity (authoring record)

1. **Successful user moment** — the owner opens `docs/architecture/postgres_schemas.md`, picks any column, and can say what it holds, who fills it and why, without reading Rust.
2. **Preserved user behaviour** — every dashboard page, CLI command and runner keeps working; chat, save, publish, install, schedules, approvals and billing behave as before apart from §4.
3. **Optimal-way check** — a from-empty rebuild is the most direct route while no production data exists; forward migrations would keep 97 slots of history and every copy column.
4. **Rebuild-vs-iterate** — rebuild; the owner chose it, and iterating would mean dozens of `ALTER` slots that freeze the mistakes.
5. **What we build** — new slots, renamed queries, two wire removals, the conventions at source, the docs.
6. **What we do NOT build** — new features; per-schema roles; Firecracker; wire renames.
7. **Fit with existing features** — compounds with M224_001 launch readiness; must not destabilise the fleet lease path (`runner.fleet_slots`, fencing).
8. **Surface order** — API and runner first; the CLI is unchanged from outside.
9. **Dashboard restraint** — no new UI; the draft client stops parsing a field it never read.
10. **Confused-user next step** — `docs/architecture/postgres_schemas.md` for contributors; the changelog `<Update>` for runner operators.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, one PR, Sections in dependency order: conventions, slots, readers, behaviour, databases, docs.
- **Alternatives considered:** forward migrations per renamed table (rejected: freezes the copies and dead columns as history); one PR per schema (rejected by the owner: "do write it on big bang").
- **Patch-vs-refactor verdict:** this is a **refactor** because placement, naming and duplication are structural; a patch would rename without removing copies.

## Discovery (consult log)

- **Consults** — Owner decisions via AskUserQuestion, Oct 17, 2026, recorded verbatim as the chosen option: clean slate "Yes, whole repo"; layout "Fold library into authoring"; shared definitions table "Drop it"; invite email table "Drop it"; library delete "Cascade delete"; deadline rename "Rename"; conventions "Orly repo + orly update"; counter names "Apply everywhere"; runner knobs "Add Firecracker knobs now" then "Fold it in"; fleet "Keep required_tags"; model "Approve with fold"; invites "Add resolved_at"; each schema "Approve". Architecture: `ARCH: grounded in schema/600_runners.sql | proposal: nine owned schemas | status: silent | landing: a` — doc-only commit `a8646bb2a`.
- **Metrics review** — no analytics/funnel playbook update required: no event, funnel or signal changes.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test`, `/orly-write-integration-test`, `/review`, `orly-babysit-prs`.
- **Deferrals** — none.

## Review prompt (Codex — Aiwa CTO with Astra)

Paste as-is. The reviewer reads source on `main`; prose is a claim to check, never evidence.

```text
Review the milestone spec docs/v2/pending/M229_001_P1_API_DOCS_INFRA_POSTGRES_SCHEMA_CLEAN_SLATE.md
in ~/Projects/agentsfleet against its canonical design docs/architecture/postgres_schemas.md and
the source on main. Be adversarial: find what breaks, not what reads well.

Attack, with file:line evidence for every claim:
1. Ordering: can §2's dependency order (roles, identity, model, vault, connector, fleet, runner,
   authoring, billing, memory) create every foreign key, including the deferred head pointers?
2. Ledger collision: does a database holding old audit.schema_migrations rows get refused before
   any slot runs (rustd/crates/afd_db/src/migrate/)? Name the failure if it would silently skip.
3. Blast radius: does Files Changed cover every writer and reader the discovery grep finds,
   including fixtures, bench seeds, scripts and live specs? List any path it misses.
4. Behaviour changes in §4: is each one tested by a negative case, and is any user-visible change
   missing from Interfaces or the docs list in §6?
5. Wire removals: does dropping deletion_generation break a runner build still in the field, and
   does anything in ui/ or cli/ read the message operation_id?
6. Production safety: is §5 safe to run only while M224_001 is unlaunched, and what stops it after?
7. Template: every Dimension has a test with a concrete assertion; the rubric uses the exact
   commands in .orly/orly.json; no estimates, owners or dates of completion.

Output: a table of findings, most severe first — severity (P0/P1/P2), file:line, what fails,
the smallest fix. Then a verdict: READY, or BLOCKED with the P0s that block. No praise, no summary.
```
