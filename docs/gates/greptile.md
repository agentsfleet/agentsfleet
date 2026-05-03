# 🚧 GREPTILE GATE

**Family:** Universal coding-rules audit. **Source:** `docs/greptile-learnings/RULES.md`.

**Triggers:** fires twice per work unit — (1) per EXECUTE iteration when diff languages change (new layer/language enters the diff); (2) end-of-turn, before claiming complete.

**Override:** `GREPTILE GATE: SKIPPED per user override (reason: ...)`. Violations stay in the diff and the user's override gets recorded in the spec's Discovery section.

## What this gate enforces

`docs/greptile-learnings/RULES.md` is the universal coding-rules catalogue. The agent failure mode is grepping the spec verbatim instead of grepping the rules; this gate makes the rule audit a printable artefact.

## Common rules referenced

| Code | Rule | Applies to |
|---|---|---|
| UFS | String literals are always constants | all source |
| STS | No static strings in SQL schema | SQL |
| EMS | Error messages follow a standard structure | Zig handlers |
| NSQ | Named constants, schema-qualified SQL | SQL |
| TGU | Tagged unions over optional-field structs | Zig |
| VLT | Secrets belong in vault, not in entity tables | SQL/Zig |
| CTM | Constant-time comparison for secrets | Zig |
| CTC | Constant-time compare must not short-circuit on length | Zig |
| FLL | File / function length caps | all source |
| ORP | Cross-layer orphan sweep on rename/delete/format | all |
| WAUTH | Workspace-IDOR safety | handlers |
| TST-NAM | Test identifiers are milestone-free | tests |
| PRI | No prompt injection from user input | LLM I/O |

Read RULES.md for any code not in this short-list.

## Pre-print check

1. Identify diff languages (`zig|ts|tsx|sql|sh|py|go|rs`).
2. List rules whose `Applies to` overlaps. UFS applies to all source; STS to SQL; EMS to Zig handlers; etc.
3. **String-literals (UFS) audit:**

   ```bash
   git diff -U0 origin/main | grep -oE '"[^"]{4,}"' | sort -u
   ```

   For each unique literal, grep `src/ ui/ zombiectl/` for an existing `const` / `pub const` / `export const` / `as const` / `Final[str]` / `readonly` declaration. Found → import. Novel + ≥2 sites → declare a const.

## Required output (one row per applicable rule, suppress non-applicable)

```
GREPTILE GATE: <iteration tag>  Diff langs: <…>
| Code     | Verdict                                |
| UFS      | clean | N violations: <list>           |
| STS      | clean | N violations                   |
| …        | …                                      |
String-literals audit: <N literals scanned, M violations>
```

## Anti-rationalisation clause

"It's just a label" / "I'll only use it once" are not exceptions to UFS. The clause applies to every rule with a behaviour modifier — when you find yourself reaching for a justification, the rule wins.
