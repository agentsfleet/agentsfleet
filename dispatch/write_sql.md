# write_sql.md — SQL / schema latent façade

This is the prose the AGENT reads before writing any `schema/*.sql` file. It pairs with the deterministic façade `dispatch/write_sql.sh` — the machine half that runs the mechanically-checkable subset and emits one verdict block. **Read `docs/SCHEMA_CONVENTIONS.md` first** — it is the source-of-truth for table/column naming, type choices, and schema-qualification (the analogue of `docs/REST_API_DESIGN_GUIDELINES.md` for `write_http`). The dissolving **Schema Table Removal Guard** card is merged verbatim below, while the durable SQL rules (`NSQ`, `STS`, `SGR`, `ITF`) stay in `docs/greptile-learnings/RULES.md` (retained) — referenced here, enforced by the GREPTILE GATE. Mechanical thresholds live once in the `.sh`; this file references rule codes, never restates the numbers.

**Signal legend** (printed by `write_sql.sh`):

- 🟢 pass — deterministic check passed.
- 🔴 fail — deterministic check failed (or helper absent); STOP, fix, rerun.
- 🔵 DECIDE — judgment-only; no script can decide, the agent reads the section and makes the call (blocks the TURN, not the script).
- ⚪ delegated — the checker runs only in the product repo, not in dotfiles.

**Tag legend** — each section heading below carries one of:

- `> [DETERMINISTIC → <CODE>]` — a machine can pass/fail it; the `.sh` row for `<CODE>` (e.g. `FLL`) enforces it. `TODO-CHECK` marks a mechanizable rule with no helper wired yet (build-the-check). `NEW:<name>` marks a proposed-but-not-yet-existing code.
- `> [JUDGMENT → <CODE>]` — no script can decide; the agent decides at write time against the prose.
- `> [container]` — a non-enforcement wrapper heading (e.g. "Merged from dissolved gate cards"); its tagged subsections carry the real codes, and the coherence audit (§6.3) skips it.

The tag grammar and semantic-anchor model are defined by the tagged sections and
deterministic façade in this snapshot.

---

# SQL & schema authoring discipline

## Scope

> [JUDGMENT → SCH]

Triggers on every `Edit`/`Write` to `schema/*.sql`, `schema/embed.zig` (`@embedFile` constants), and the canonical migration array in `src/cmd/common.zig`. **Before authoring DDL, read `docs/SCHEMA_CONVENTIONS.md`** (naming, types, schema-qualification). SQL embedded in handlers (`*.zig`) and integration-test fixtures is also governed by the companion rules below — those surfaces additionally fire `write_zig.md` and the GREPTILE GATE.

## Merged from dissolved gate cards

> [container]

The **Schema Table Removal Guard** card dissolves into this façade; its prose is preserved verbatim below (title demoted to a tagged `###`, its subsections demoted to `####` and tagged `SCH`).

### Schema Table Removal Guard (SCHEMA GUARD)

> [JUDGMENT → SCH]

**Family:** Schema discipline. **Source:** `AGENTS.md` (project-side guard). Related: **RULE STS** (no static strings in SQL schema), **RULE NSQ** (named constants, schema-qualified SQL) — both in `docs/greptile-learnings/RULES.md`.

**Triggers** — before any of these, run `cat VERSION` and print the guard output:

- Creating, editing, or deleting any file under `schema/*.sql`.
- Editing `schema/embed.zig` (any `@embedFile` constant).
- Editing the canonical migration array in `src/cmd/common.zig`.
- Writing `DROP TABLE`, `ALTER TABLE`, or `SELECT 1;` into any SQL file.
- Accepting a spec dimension prescribing a "DROP migration", "ALTER migration", or "version marker".

**Override:** `SCHEMA GUARD: SKIPPED per user override (reason: ...)`. **User-invokable only.** Spec violates the guard → amend the spec first.

#### Pre-v2.0.0 (teardown-rebuild era)

> [JUDGMENT → SCH]

To remove a table:

1. `rm schema/NNN_foo.sql`
2. Remove `@embedFile` from `schema/embed.zig`
3. Remove the entry from the migration array in `src/cmd/common.zig` and update length + index-based tests.

**Forbidden:** `ALTER TABLE`, `DROP TABLE`, `SELECT 1;` markers, comment-only files, "keep file for slot numbering". Slot gaps are fine — the DB is wiped on rebuild.

#### v2.0.0+

> [JUDGMENT → SCH]

Proper `ALTER`/`DROP` migrations in new numbered files. No teardown.

#### Required output

> [JUDGMENT → SCH]

```
SCHEMA GUARD: VERSION=<v> (<2.0.0 ? teardown : alter)
  rm:schema/<file>.sql
  rm-embed:<const>
  rm-migration:v<N>
```

For pre-v2.0.0 path, all three rm- lines appear. For v2.0.0+, replace with `migration:schema/<NNN>_<change>.sql`.

## Companion SQL rules (retained in `docs/greptile-learnings/RULES.md`)

> [container]

These rules are **not** dissolved — they remain in `docs/greptile-learnings/RULES.md`. STS/NSQ/SGR are enforced by the GREPTILE GATE until a leaf check is wired (`TODO-CHECK`); ITF additionally surfaces as a `🔵 DECIDE` judgment row in `write_sql.sh`. Summarised here so the SQL author sees the full surface in one place; RULES.md stays canonical (read it for the full text + `Ref:` provenance).

### RULE STS — No static strings in SQL schema

> [DETERMINISTIC → TODO-CHECK]

Never `DEFAULT` or `CHECK (… IN (…))` with hardcoded strings in schema — SQL cannot reference Zig/JS constants, so schema strings drift from code. Enforce value constraints via application constants. (RULES.md `RULE STS`.)

### RULE NSQ — Named constants, schema-qualified SQL

> [DETERMINISTIC → TODO-CHECK]

No magic numbers; all SQL in handlers is schema-qualified (`core.table`, not `table`) — unqualified names fail when `search_path` differs across environments. (RULES.md `RULE NSQ`.)

### RULE SGR — Migrations include GRANT statements

> [DETERMINISTIC → TODO-CHECK]

Every `CREATE TABLE` migration ends with `GRANT`s for every role that queries the table (`api_runtime` / `worker_runtime`), mirroring the table's callers. PostgreSQL denies by default — a missing grant fails only at first runtime use. (RULES.md `RULE SGR`.)

### RULE ITF — Integration tests use the real schema

> [JUDGMENT → ITF]

An integration test touching a production table seeds rows through a shared `src/db/test_fixtures_<scope>.zig` module against the real schema — never a session-local `CREATE TEMP TABLE` mocking the production shape (the mock drifts and hides schema changes). Fixtures use semantic scope names, never milestone-numbered ones. (RULES.md `RULE ITF`.)
