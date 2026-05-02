# 🚧 File & Function Length Gate

**Family:** Universal length discipline. **Source:** `docs/greptile-learnings/RULES.md` RULE FLL (File & Function Length).

**Caps:** file ≤ 350 lines · function ≤ 50 lines · method ≤ 70 lines.

**Triggers** — every Write/Edit that net-adds lines to a source file:
`.zig`, `.js`, `.ts`, `.tsx`, `.jsx`, `.py`, `.rs`, `.go`, `.sh`, `.sql`, `.yaml`/`.toml` (when carrying code). If the file extension is ambiguous, the gate FIRES by default — opt-out requires the user override below.

**Exempt:** `vendor/`, `node_modules/`, `third_party/` (upstream); `.md` files; published API artefacts under `public/` (loose ≤ 400-line advisory on path YAMLs); per-repo extensions in `docs/greptile-learnings/RULES.md`.

**Override:** `LENGTH GATE: SKIPPED per user override (reason: ...)` immediately preceding the edit.

## Pre-edit check (mandatory)

1. `wc -l <file>` — current count (0 for new files).
2. Net delta: `+added - removed`.
3. Projected: `current + delta`.
4. If projected > 350, **STOP**. Split first: extract a cohesive block to a sibling file using the repo's `<module>_<concern>.<ext>` convention (`zombie_list.js` beside `zombie.js`). Then apply the original edit.
5. Function sub-gate: project post-edit line count for any touched function. If > 50 (function) or > 70 (method), split into named helpers **before** writing.

## Splitting conventions

- Files named after the concern extracted (`zombie_list.js` not `zombie2.js`).
- Helper function names describe the step (`normalizeCursor()` not `helperA()`).

## Required output

Print only when projected ≥ 300 lines OR touched function within 10 of cap:

```
LENGTH GATE: <file> N+Δ=<N+Δ> (cap 350, headroom <H>) | fn:<name> <F> lines (cap 50/70) | proceed|split.
```

## Self-audit (end-of-turn)

```bash
git diff --name-only origin/main \
  | grep -v -E '\.md$|^vendor/|_test\.|\.test\.|\.spec\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' \
  | awk '$1 > 350 { print "❌ " $2 ": " $1 " lines (limit 350)" }'
```

Non-empty output = hard fail.
