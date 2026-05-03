# 🚧 Schema Table Removal Guard

**Family:** Schema discipline. **Source:** `AGENTS.md` (project-side guard). Related: **RULE STS** (no static strings in SQL schema), **RULE NSQ** (named constants, schema-qualified SQL) — both in `docs/greptile-learnings/RULES.md`.

**Triggers** — before any of these, run `cat VERSION` and print the guard output:

- Creating, editing, or deleting any file under `schema/*.sql`.
- Editing `schema/embed.zig` (any `@embedFile` constant).
- Editing the canonical migration array in `src/cmd/common.zig`.
- Writing `DROP TABLE`, `ALTER TABLE`, or `SELECT 1;` into any SQL file.
- Accepting a spec dimension prescribing a "DROP migration", "ALTER migration", or "version marker".

**Override:** `SCHEMA GUARD: SKIPPED per user override (reason: ...)`. **User-invokable only.** Spec violates the guard → amend the spec first.

## Pre-v2.0.0 (teardown-rebuild era)

To remove a table:

1. `rm schema/NNN_foo.sql`
2. Remove `@embedFile` from `schema/embed.zig`
3. Remove the entry from the migration array in `src/cmd/common.zig` and update length + index-based tests.

**Forbidden:** `ALTER TABLE`, `DROP TABLE`, `SELECT 1;` markers, comment-only files, "keep file for slot numbering". Slot gaps are fine — the DB is wiped on rebuild.

## v2.0.0+

Proper `ALTER`/`DROP` migrations in new numbered files. No teardown.

## Required output

```
SCHEMA GUARD: VERSION=<v> (<2.0.0 ? teardown : alter)
  rm:schema/<file>.sql
  rm-embed:<const>
  rm-migration:v<N>
```

For pre-v2.0.0 path, all three rm- lines appear. For v2.0.0+, replace with `migration:schema/<NNN>_<change>.sql`.
