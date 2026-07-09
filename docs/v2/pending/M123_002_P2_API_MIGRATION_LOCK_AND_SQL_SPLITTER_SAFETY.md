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

# M123_002: Serialize migration bookkeeping tables under the lock, split SQL correctly, clear stale failure rows

**Prototype:** v2.0.0
**Milestone:** M123
**Workstream:** 002
**Date:** Jul 09, 2026
**Status:** PENDING
**Priority:** P2 — three migration-path correctness gaps; the worst is a fresh-DB simultaneous-first-deploy race that self-heals on restart, the others latent (no live trigger) but silent when they fire.
**Categories:** API
**Batch:** B1 — runs alone; touches only the migration runner, the SQL splitter, and one reachable integration-test file.
**Branch:** {added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** M122_005 (test-root reachability) — the corpus guard this spec hardens in `cmd/common.zig` executes only once M122_005 wires that file into a test root; landing this ahead of it would leave the hardened guard green-by-omission, the exact pathology M122_005 closes.
**Provenance:** agent-generated — the Jul 09, 2026 `m122-gap-audit-security` workflow audited three migration-path areas a Jul 02, 2026 coverage critic flagged and never reached. Each finding survived an adversarial refutation pass; the Data Definition Language (DDL) race finding is 3/3 uphold with a corrected severity (P1→P2), the splitter and stale-row findings 1/1 uphold at their original P2/P3.
**Canonical architecture:** `docs/architecture/data_flow.md` — the serve-boot migration chain (`enforceServeMigrationSafety` → `inspectMigrationState` → `runMigrations`); this spec changes ordering and correctness inside that chain, not its shape.

---

## Overview

**Goal (testable):** the schema-migration advisory lock is held before any bookkeeping-table DDL runs, so two daemons booting against a fresh database cannot race `CREATE ... IF NOT EXISTS`; the SQL splitter parses tagged dollar-quotes and block comments correctly and loudly rejects structurally-unterminated input instead of silently truncating a migration; and a `schema_migration_failures` row whose version is already applied no longer blocks serve boot.

**Problem:** three correctness gaps in the migration path, each real but bounded. (1) `runMigrations` creates its bookkeeping tables (`ensureSchemaMigrationsTable` / `ensureSchemaMigrationFailuresTable`) BEFORE it takes `migration_lock.acquire`, and Postgres `CREATE ... IF NOT EXISTS` is not race-safe. On a fresh empty database, two replicas booting simultaneously with `MIGRATE_ON_START=1` can both reach the DDL before either holds the lock; one backend raises a duplicate-catalog error (sqlstate 23505 / 42P07), that replica's serve boot exits, and it self-heals on restart once the winner has created the tables. No data loss, no persistent outage — a once-ever fresh-DB race. The serve-boot policy probe (`decideServeMigrationPolicy` gating on `lock_available`) uses `pg_try_advisory_xact_lock`, which auto-releases at statement end, so it is a point-in-time check that does not hold anything across `runMigrations` — the reorder is what actually closes the window. (2) `sql_splitter.zig` recognises only bare `$$` dollar-quotes and treats block comments as unsupported; a future migration using the standard `AS $body$ ... ; ... $body$` plpgsql idiom would split on the first `;` inside the body and apply a truncated statement, while the only corpus guard asserts merely `count != 0` and so passes in CI. Latent, not live: every current migration uses bare `$$`. (3) `clearMigrationFailure` runs after `COMMIT`, outside the transaction, and swallows its error; `inspectMigrationState` flags `has_failed_migrations` from ANY failures row without correlating against `schema_migrations`, so a transient post-commit DELETE failure on an already-applied migration can permanently block serve boot with `MigrationFailed` until an operator runs `agentsfleetd migrate` by hand.

**Solution summary:** move `migration_lock.acquire` above the `ensure*` DDL in `runMigrations` (the lock needs only a session and a constant key, no table) so the advisory lock covers all bookkeeping DDL; teach the splitter to track tagged `$tag$` dollar-quotes and skip `/* ... */` block comments, add a non-allocating `validate` pass that returns a named error on an unterminated dollar-quote / block comment / string, call it before applying each migration, and harden the corpus guard from `count != 0` to a correct-split assertion; and make `hasFailedMigrationRecords` correlate against `schema_migrations` so a failure row whose version is already applied is treated as resolved rather than fatal.

## PR Intent & comprehension handshake

- **PR title (eventual):** Serialize migration bookkeeping DDL under the lock, split SQL correctly, stop stale failure rows wedging boot
- **Intent (one sentence):** the migration runner is safe under a simultaneous first deploy, cannot silently truncate a dollar-quoted migration, and does not brick serve boot on a stale failure row.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/db/pool_migrations.zig` — `runMigrations` (the ensure*→acquire order §1 reverses), `inspectMigrationState` + `hasFailedMigrationRecords` (the correlation §3 adds), `applySqlStatements` (the §2 validate call site). Mirror the existing `catch |err| logPgErrorContext` shape for any new error path.
2. `src/agentsfleetd/db/pool_migration_lock.zig` — `acquire` needs only a `*Conn` and the constant `AdvisoryLockKey`, no table; its header documents why the lock exists to serialize concurrent migrators — the DDL that builds the serialization bookkeeping must run inside it.
3. `src/agentsfleetd/db/sql_splitter.zig` — the `next()` state machine (`in_single_quote` / `in_dollar_quote`, the bare-`$$` branch, `skipWhitespaceAndComments`) §2 extends, plus its module header's "does NOT handle" caveats to delete.
4. `src/agentsfleetd/fleet/schema_migration_test.zig` — the reachable, DB-gated integration file (`openConnOrSkip`, `scalarI64` drain-safe probes) §1/§3 add their regression tests to; it already notes the migration-corpus guards live in `cmd/common.zig`.
5. `docs/greptile-learnings/RULES.md` — RULE RSP (reject unsupported patterns at parse time, not match time — the §2 loud-reject principle) and RULE MIG (migration bookkeeping assertions track the array, not a literal).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/db/pool_migrations.zig` | EDIT | §1 reorder `migration_lock.acquire` above the `ensure*` DDL; §2 call the splitter `validate` before applying each migration; §3 correlate `hasFailedMigrationRecords` against `schema_migrations` |
| `src/agentsfleetd/db/sql_splitter.zig` | EDIT | §2 tagged `$tag$` dollar-quote + `/* */` block-comment support, a non-allocating `validate`, header caveats removed, new unit tests |
| `src/agentsfleetd/fleet/schema_migration_test.zig` | EDIT | §1 lock-before-DDL regression; §3 stale-vs-genuine failure-row correlation regression (both reachable today) |
| `src/agentsfleetd/cmd/common.zig` | EDIT | §2 harden the `every migration SQL is parseable` corpus guard from `count != 0` to a correct-split assertion — runs only after M122_005 wires this file in |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **RSP** (the splitter rejects unterminated/unsupported input at parse time with a named error, never silently falls through to a wrong split), **ECL** (the new `validate` error is distinct from `error.PG` transport errors so callers do not conflate a malformed migration with a DB failure), **UFS** (the dollar-tag delimiters, block-comment markers, and the new error name live as named constants), **TST-NAM** (new test identifiers carry no milestone/section IDs), **NLR/NDC** (touch-it-fix-it on `sql_splitter.zig`'s header and `pool_migrations.zig`; no dead assertion residue in the hardened guard), **OBS** (the loud-reject and lock-ordering branches emit scoped `db_migrate` log lines).
- **`dispatch/write_zig.md`** — all four files are `*.zig`: pg-drain (`PgQuery` auto-drain on the new correlated query), tagged-union/named-error result shape for `validate`, `errdefer`/ownership review, File & Function Length near the `pool_migrations.zig` cap, cross-compile both linux targets.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — three Zig source files + one test file edited | cross-compile `x86_64-linux` + `aarch64-linux`; `make check-pg-drain` after the `hasFailedMigrationRecords` query change |
| PUB / Struct-Shape | yes — `sql_splitter.zig` gains a `pub fn validate` | shape verdict recorded at EXECUTE: `SqlStatementSplitter` stays a conventional multi-method struct (splitter + count + validate); no file-as-struct reshape (operations-over-value) |
| File & Function Length (≤350/≤50/≤70) | yes — `pool_migrations.zig` is at 340 lines; `sql_splitter.zig` at 229 grows with `$tag$`/block-comment/validate logic + tests | the §1 reorder is net-zero (moves three statements); if `sql_splitter.zig` nears the cap, extract its inline tests to a sibling `sql_splitter_test.zig` force-imported from `src/agentsfleetd/tests.zig`; keep `pool_migrations.zig` under 350 by tightening adjacent comments |
| UFS (repeated/semantic literals) | yes | dollar-tag scan bounds, `/*`/`*/` markers, the `validate` error name, and the correlated-failures SQL as named constants |
| UI Substitution / DESIGN TOKEN | no | no UI surface |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING yes; others no | the reorder and loud-reject branches log via the existing `.db_migrate` scope; no new error-registry code, no allocator lifecycle change, no schema/embed edit |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/db/pool_migration_lock.zig` — `probeAvailable`'s `pg_try_advisory_xact_lock` and `acquire`'s bounded session-lock retry are the exact primitives §1 reorders around; the module header already states the lock's serialization purpose. Divergence: none — §1 only moves the existing `acquire` call earlier.
- **Reference:** the splitter's existing `in_single_quote` escape-handling and `skipWhitespaceAndComments` — §2 mirrors that state-machine style for the `$tag$` and `/* */` cases rather than inventing a parser. Divergence: block comments are skipped like `--` line comments; tagged dollar-quotes capture the tag and match it verbatim on close.

## Sections (implementation slices)

### §1 — Advisory lock covers all migration bookkeeping DDL

`runMigrations` runs `ensureSchemaMigrationsTable` / `ensureSchemaMigrationFailuresTable` (each a `CREATE SCHEMA/TABLE IF NOT EXISTS`) before `migration_lock.acquire`, so the DDL that builds the serialization bookkeeping runs outside the lock meant to serialize it. **Implementation default:** move `migration_lock.acquire` (and its `defer migration_lock.release`) above the two `ensure*` calls, because `acquire` needs only a session and the constant key — no table — so it can be taken first, and the lock then covers the reap, the ensure DDL, and every apply. The loser of a genuinely-simultaneous boot now bounded-retries (~30s) and either succeeds as a no-op or exits with `MigrationLockUnavailable`, instead of crashing on a duplicate-catalog error.

- **Dimension 1.1** — with the advisory lock held by a separate session, `runMigrations` returns `error.MigrationLockUnavailable` and creates no `audit` bookkeeping (proving `acquire` precedes the DDL) → Test `test_lock_held_blocks_before_ddl`
- **Dimension 1.2** — a normal `runMigrations` on a clean database applies every migration and a second call is a no-op (regression: reorder does not change the happy path) → Test `test_run_migrations_idempotent_happy_path`

### §2 — SQL splitter: tagged dollar-quotes, block comments, loud on the unparseable

The splitter matches only bare `$$` and treats `/* */` as unsupported, so a tagged `$body$ ... ; ... $body$` function body splits on an internal `;`; the corpus guard only checks `count != 0` and cannot see the truncation. **Implementation default:** support the two documented gaps AND add a loud backstop — (a) capture the tag on an opening `$tag$` and match it verbatim on close so internal `;`/`$$` are inert; (b) skip `/* ... */` block comments like `--` line comments; (c) add a non-allocating `pub fn validate` that scans and returns a named error (distinct from `error.PG`) on an unterminated dollar-quote, block comment, or string; (d) call `validate` in `applySqlStatements` before splitting so a genuinely-unparseable migration fails loudly at apply time, never truncates. Silent truncation of a migration is the state this section makes unrepresentable. Delete the "does NOT handle" caveats from the module header.

- **Dimension 2.1** — `CREATE FUNCTION f() ... AS $body$ BEGIN ...; ...; END $body$ LANGUAGE plpgsql;` splits as exactly one statement → Test `test_splitter_tagged_dollar_quote_single_statement`
- **Dimension 2.2** — a `/* ; $$ $body$ */` block comment is skipped entirely, contributing no boundary and opening no quote state → Test `test_splitter_block_comment_skipped`
- **Dimension 2.3** — an unterminated `$tag$`, unterminated `/*`, or unterminated `'` string makes `validate` return its named error instead of the splitter returning a truncated tail → Test `test_splitter_validate_rejects_unterminated`
- **Dimension 2.4** — every canonical migration passes `validate` and splits to its expected boundary count (corpus guard hardened from `count != 0`; lives in `cmd/common.zig`, so it executes only once M122_005 wires that file into a test root) → Test `test_every_migration_splits_correctly`

### §3 — A stale failure row for an applied migration no longer blocks boot

`clearMigrationFailure` runs after `COMMIT`, outside the transaction, and swallows its error; `inspectMigrationState` treats any `schema_migration_failures` row as fatal, so a transient post-commit DELETE failure on an already-applied migration wedges serve boot with `MigrationFailed` until a manual `agentsfleetd migrate`. **Implementation default:** correlate in `hasFailedMigrationRecords` — a failure row counts only when its version is absent from `schema_migrations` (a genuinely failed, not-yet-applied migration). A failure row whose version is applied is treated as resolved, so the swallowed DELETE is no longer load-bearing for boot correctness. This is the durable fix over re-arming the post-commit DELETE.

- **Dimension 3.1** — a `schema_migration_failures` row whose version is present in `schema_migrations` → `inspectMigrationState` reports `has_failed_migrations = false` (before the fix: true) → Test `test_applied_version_failure_row_is_resolved`
- **Dimension 3.2** — a failure row whose version is NOT applied still reports `has_failed_migrations = true` (regression: a genuine failure still blocks) → Test `test_unapplied_version_failure_row_still_blocks`

## Interfaces

```
sql_splitter.SqlStatementSplitter
  pub fn next(self: *Self) ?[]const u8   // unchanged signature; now tracks
                                          // $tag$ dollar-quotes + skips /* */
  pub fn count(sql: []const u8) u32       // unchanged
  pub fn validate(sql: []const u8) SplitError!void   // NEW — returns a named
                                          // error on unterminated quote/comment/string

pool_migrations.runMigrations(pool, migrations) !void
  // unchanged signature; acquire now precedes ensure* DDL; applySqlStatements
  // calls validate before splitting.

pool_migrations.inspectMigrationState(pool, migrations) !MigrationState
  // unchanged signature; MigrationState.has_failed_migrations now derives from
  // failure rows whose version is NOT in schema_migrations.
```

No HTTP route, Command-Line Interface (CLI) surface, or on-disk schema path changes. `schema/embed.zig` and the migration SQL files are read-only inputs.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Simultaneous first deploy, lock held | two replicas boot on a fresh DB, one takes the lock first | the loser bounded-retries then returns `MigrationLockUnavailable`; no bookkeeping DDL runs unserialized, no duplicate-catalog crash (Dimension 1.1) |
| Tagged-dollar-quote migration | future plpgsql `$body$ ... ; ... $body$` function | splitter keeps the body intact; one statement applied, not a truncated fragment (Dimension 2.1) |
| Block comment with `;`/`$$` | migration carries an explanatory `/* ... */` | comment skipped; no false boundary, no false quote state (Dimension 2.2) |
| Structurally-unparseable migration | unterminated `$tag$` / `/*` / `'` | `validate` returns its named error before apply; the migration fails loudly, never truncates (Dimension 2.3) |
| Stale failure row for an applied migration | swallowed post-commit DELETE after a re-applied migration | correlation treats it resolved; serve boot proceeds (Dimension 3.1) |
| Genuine unapplied-migration failure | a migration actually failed and was not applied | failure row still fatal; boot blocks (Dimension 3.2 regression) |

## Invariants

1. No migration bookkeeping DDL runs outside the advisory lock — enforced by the reorder plus Dimension 1.1's lock-held test asserting no `audit` schema is created when the lock is unavailable.
2. A migration is either split correctly or rejected by `validate`; it is never silently truncated — enforced by `validate` (a compiler-checked `SplitError` return) called on every migration in `applySqlStatements`, plus Dimensions 2.1–2.4.
3. `MigrationState.has_failed_migrations` is true only for a failure row whose version is not applied — enforced by the correlated query in `hasFailedMigrationRecords` and Dimensions 3.1/3.2.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | migration-path correctness fixes only; the reorder and loud-reject branches log via the existing `.db_migrate` scope, no new or renamed event | migration version, error name | no secret material in migration logs | the new tests assert lock/exit behaviour, split correctness, and `has_failed_migrations`, not events |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_lock_held_blocks_before_ddl` | a separate session holds the advisory lock → `runMigrations` returns `MigrationLockUnavailable`; no `audit` bookkeeping was created on the blocked attempt |
| 1.2 | integration (regression) | `test_run_migrations_idempotent_happy_path` | clean DB → all migrations applied; second call applies zero, no error |
| 2.1 | unit | `test_splitter_tagged_dollar_quote_single_statement` | a `$body$`-wrapped plpgsql function with internal `;` → exactly one statement returned |
| 2.2 | unit | `test_splitter_block_comment_skipped` | input with a `/* ; $$ */` comment → boundary count unchanged, no open quote state |
| 2.3 | unit (negative) | `test_splitter_validate_rejects_unterminated` | unterminated `$tag$` / `/*` / `'` → `validate` returns its named error; no truncated tail |
| 2.4 | unit (corpus) | `test_every_migration_splits_correctly` | every `canonicalMigrations()` entry passes `validate` and yields its expected statement count (runs after M122_005) |
| 3.1 | integration (negative) | `test_applied_version_failure_row_is_resolved` | failure row for an applied version → `has_failed_migrations = false` |
| 3.2 | integration (regression) | `test_unapplied_version_failure_row_still_blocks` | failure row for an unapplied version → `has_failed_migrations = true` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Lock precedes DDL; splitter parses/rejects correctly (§1/§2) | `make test` | exit 0 incl. the splitter unit tests | P0 | |
| R2 | Lock-held and stale-row regressions pass (§1/§3) | `make test-integration` | exit 0 incl. the new migration tests | P0 | |
| R3 | Splitter header no longer disclaims tagged dollar-quotes/block comments (§2) | `grep -n "not supported\|does NOT handle" src/agentsfleetd/db/sql_splitter.zig` | no output | P1 | |
| R4 | Corpus guard no longer passes on `count != 0` alone (§2) | `grep -n "stmt_count == 0" src/agentsfleetd/cmd/common.zig` | no output | P1 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | pg-drain intact (query changed) | `make check-pg-drain` | exit 0 | P0 | |
| S5 | No leaks (Zig migration path touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted. §2 removes the "does NOT handle" header caveats and the `count != 0` corpus assertion in place; no symbol is renamed or orphaned.

## Out of Scope

- The `decideServeMigrationPolicy` check-then-act probe (`lock_available` via `pg_try_advisory_xact_lock`) — it is a point-in-time read that auto-releases and never claims to serialize across `runMigrations`; the §1 reorder closes the DDL window without touching it, and hardening the probe into a held lock is a separate design change.
- Folding the post-commit `clearMigrationFailure` DELETE into the apply transaction — the §3 correlation makes the swallowed DELETE non-load-bearing; the transactional variant is a redundant belt, deferred.
- Retroactively re-running or re-grading migrations already applied under the old ordering — the reorder is forward-only; existing databases already have their bookkeeping tables.
- Wiring `cmd/common.zig` into a test root — owned by M122_005 (the declared dependency); this spec only edits the guard body that file already contains.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator rolls out two `agentsfleetd` replicas simultaneously against a brand-new database and both converge (one migrates, the other waits and no-ops) instead of one crash-looping on a duplicate-catalog error; and a re-applied previously-failed migration lets serve boot come up on its own rather than wedging until someone runs `agentsfleetd migrate` by hand.
2. **Preserved user behaviour** — single-replica boot, `MIGRATE_ON_START` gating, the bounded lock retry, and every happy-path migration apply keep their exact semantics; only the failure and race branches change.
3. **Optimal-way check** — the reorder is the most direct fix (moving an existing call earlier costs no new query); the splitter change extends the existing state machine rather than swapping in a parser; the correlation reuses the two tables already queried. No larger design change is warranted.
4. **Rebuild-vs-iterate** — iterate: three contained edits on the existing migration runner and splitter, each mirroring a sibling pattern already in the file. Nothing trades determinism away.
5. **What we build** — one statement reorder, tagged-dollar-quote + block-comment handling with a `validate` backstop, and a correlated failure-row check.
6. **What we do NOT build** — a re-architected advisory-lock probe, a transactional failure-clear, or a general SQL parser (see Out of Scope).
7. **Fit with existing features** — compounds the serve-boot migration safety chain in `data_flow.md`; must not destabilize `enforceServeMigrationSafety`'s post-run re-inspection, which reads the same `has_failed_migrations` this spec redefines.
8. **Surface order** — N/A — no user surface. Internal migration-runner hardening; operators observe only boot behaviour and `db_migrate` logs, which change on failure branches only.
9. **Dashboard restraint** — N/A — no user surface. No UI or control is added.
10. **Confused-user next step** — N/A — no user surface. The operator-facing recovery is the existing `db_migrate` scoped logs: the loud-reject path names the unparseable migration version, and `MigrationLockUnavailable` names lock contention.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections, one per finding — a reorder, a splitter correctness fix, and a failure-correlation fix — each independently testable and DONE-markable, sharing a workstream because they are the same class of defect (a migration-path correctness gap) at adjacent priorities, not because they share code.
- **Alternatives considered:** (a) for §2, making the splitter LOUDLY REJECT all tagged dollar-quotes/block comments rather than supporting them — rejected: the plpgsql `$body$` idiom is standard and will land eventually, so supporting it is the durable fix; the `validate` backstop still gives the loud-reject guarantee for the genuinely unparseable. (b) for §3, re-arming the post-commit DELETE inside the transaction — rejected as the primary fix: correlation makes a stale row harmless regardless of the DELETE's fate, which the transactional variant alone does not (a later transient failure could still leave a row).
- **Patch-vs-refactor verdict:** this is a **patch** across the existing migration runner and splitter; the only new surface (`validate`) is additive and small, hardening the current shape rather than restructuring it.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: empty at creation.
- **Metrics review** — empty at creation.
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** — empty at creation.
