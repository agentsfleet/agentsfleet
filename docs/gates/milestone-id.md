# 🚧 Milestone-ID Gate

**Family:** Source-rot prevention. **Source:** `AGENTS.md` (project-side guard). Related: **RULE TST-NAM** (test identifiers are milestone-free) in `docs/greptile-learnings/RULES.md`.

**Triggers — before saving any file matching:**

- `**/*.zig` · `**/*.sql` · `**/*.ts` · `**/*.tsx` · `**/*.js` · `**/*.jsx` · `**/*.py` · `**/*.rs` · `**/*.go` · `**/*.sh`
- Any config file (`*.toml`, `*.yaml`, `*.json`) outside `docs/`
- Test files (the `_test.` / `.test.` / `.spec.` naming doesn't exempt — tests are code).

**Exempt paths** (IDs allowed): `docs/`, `**/*.md` outside `node_modules/`/`vendor/`, `CLAUDE.md`/`AGENTS.md`/`AGENTS_INVARIANCE.md`.

**Override:** `MILESTONE ID ALLOWED per user override (reason: ...)` in the immediately-preceding comment line.

## Why

Milestone IDs (`M{N}_{NNN}`), section refs (`§X.Y`), and dimension tokens (`T7`, `dim 5.8.15`) belong in specs, PR descriptions, and scratchpads — never in source code, since the codebase outlives any individual milestone and these references rot.

## Pre-edit check

Grep about-to-save content for:

```
M[0-9]+_[0-9]+
§[0-9]+(\.[0-9]+)+
\bT[0-9]+\b
\bdim [0-9]+\.[0-9]+\b
```

If any match, **strip the reference before saving.** Rewrite to describe the code's purpose, not its spec lineage.

## Self-audit (end-of-turn)

Covered by the combined awk audit in AGENTS.md HARNESS VERIFY section.
