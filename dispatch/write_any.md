# write_any.md — cross-cutting authoring latent façade

This is the prose the AGENT reads before writing ANY source file (`*.zig` / `*.ts` / `*.tsx` / `*.js` / `*.jsx` / `*.py` / `*.rs` / `*.go` / `*.sh` / `*.sql`). These are the language-agnostic authoring invariants that apply identically to every surface — literal hygiene, length, observability, milestone-free naming, dead-code/legacy. They fire IN ADDITION to the language façade (`write_zig` / `write_ts_adhere_bun` / `write_sql`), not instead of it. It pairs with the deterministic façade `dispatch/write_any.sh`. Nine dissolving gate cards are merged verbatim below (headings demoted one level, each subsection tagged); the durable rule catalogue stays in `docs/greptile-learnings/RULES.md` (retained), which the Greptile Gate audits. Mechanical thresholds live once in the `.sh`; this file references rule codes, never restates the numbers.

**Signal legend** (printed by `write_any.sh`):

- 🟢 pass — deterministic check passed.
- 🔴 fail — deterministic check failed (or helper absent); STOP, fix, rerun.
- 🔵 DECIDE — judgment-only; no script can decide, the agent reads the section and makes the call (blocks the TURN, not the script).
- ⚪ delegated — the checker runs only in the product repo, not in dotfiles.

**Tag legend** — each section heading below carries one of:

- `> [DETERMINISTIC → <CODE>]` — a machine can pass/fail it; the `.sh` row for `<CODE>` (e.g. `FLL`, `UFS`, `LOG`, `MSID`, `ERR`) enforces it. `TODO-CHECK` marks a mechanizable rule with no helper wired yet (build-the-check). `NEW:<name>` marks a proposed-but-not-yet-existing code.
- `> [JUDGMENT → <CODE>]` — no script can decide; the agent decides at write time against the prose.
- `> [container]` — a non-enforcement wrapper heading (e.g. "Merged from dissolved gate cards"); its tagged subsections carry the real codes, and the coherence audit (§6.3) skips it.

The tag grammar and semantic-anchor model are defined by the tagged sections and
deterministic façade in this snapshot.

---

# Cross-cutting authoring discipline

## Scope

> [JUDGMENT → GRP]

Triggers on every `Edit`/`Write` to a source file in any language: `*.zig`, `*.ts`, `*.tsx`, `*.js`, `*.jsx`, `*.py`, `*.rs`, `*.go`, `*.sh`, `*.sql` (outside `vendor/`, `node_modules/`, `third_party/`, `.zig-cache/`, `dist/`, `build/`, `.next/`). Fires alongside — never instead of — the per-language façade. `.md` and everything under `docs/**` are exempt from the length sub-gate (see the card below).

**Per-file lens.** This is the EXECUTE/CONFORM early-warning: it dispatches over the source files actually touched or `--staged`, so a turn that stages no in-scope source short-circuits with nothing to check. The full-tree audits (`audit-ufs` etc.) still fire unconditionally via the product repo's conformance command — `make harness-verify` in `agentsfleet` — so the dispatch is the per-edit lens, not the codebase-wide backstop.

**"Green deferred" defers only the build gate.** When the user grants "free to break, green later" on a multi-slice cutover, the per-edit gates in this façade (UFS, LENGTH, and the language façade's PUB / LIFECYCLE / BUFFER) still run on every Write/Edit — with no compile until late convergence they are the *only* correctness signal, and a big-bang build will not retro-catch speculative `pub`s, re-spelled literals, or missing `errdefer`s (M80 B3 drifted exactly this way).

## Merged from dissolved gate cards

> [container]

Nine cross-cutting gate cards dissolve into this façade. Each is preserved verbatim below (title demoted to a tagged `###`, subsections demoted to `####` and tagged with the card's code — except the file-length card's fn/method sub-cap, which is a `TODO-CHECK` subsection per §13). Five carry deterministic codes: `FLL` (inline) and `UFS`/`LOG`/`MSID` run here via leaf helpers, while `ERR` is deterministic-but-**delegated** — its leaf needs the product-repo `error_registry.zig`, so `write_any.sh` emits `⚪ DELEGATED` and the product harness runs it. Four are judgment gates (`GRP`/`NLR`/`NLG`/`LDC`).

### File & Function Length Gate (LENGTH GATE)

> [DETERMINISTIC → FLL]


**Family:** Universal length discipline. **Source:** `docs/greptile-learnings/RULES.md` RULE FLL (File & Function Length).

**Caps:** file ≤ 350 lines · function ≤ 50 lines · method ≤ 70 lines.

**Triggers** — every Write/Edit that net-adds lines to a source file:
`.zig`, `.js`, `.ts`, `.tsx`, `.jsx`, `.py`, `.rs`, `.go`, `.sh`, `.sql`, `.yaml`/`.toml` (when carrying code). If the file extension is ambiguous, the gate FIRES by default — opt-out requires the user override below.

**Exempt:** `vendor/`, `node_modules/`, `third_party/` (upstream); `.md` files; **everything under `docs/**`** (documentation tree — any extension); published API artefacts under `public/` (loose ≤ 400-line advisory on path YAMLs); per-repo extensions in `docs/greptile-learnings/RULES.md`.

**Override:** `LENGTH GATE: SKIPPED per user override (reason: ...)` immediately preceding the edit.

#### Pre-edit check (mandatory)

> [DETERMINISTIC → FLL]

1. `wc -l <file>` — current count (0 for new files).
2. Net delta: `+added - removed`.
3. Projected: `current + delta`.
4. If projected > 350, **STOP**. Split first: extract a cohesive block to a sibling file using the repo's `<module>_<concern>.<ext>` convention (`agent_list.js` beside `agent.js`). Then apply the original edit.
5. Function sub-gate: project post-edit line count for any touched function. If > 50 (function) or > 70 (method), split into named helpers **before** writing.

#### Splitting conventions

> [DETERMINISTIC → FLL]

- Files named after the concern extracted (`agent_list.js` not `agent2.js`).
- Helper function names describe the step (`normalizeCursor()` not `helperA()`).

#### Required output

> [DETERMINISTIC → FLL]

Print only when projected ≥ 300 lines OR touched function within 10 of cap:

```
LENGTH GATE: <file> N+Δ=<N+Δ> (cap 350, headroom <H>) | fn:<name> <F> lines (cap 50/70) | proceed|split.
```

#### Self-audit (end-of-turn)

> [DETERMINISTIC → FLL]

```bash
git diff --name-only origin/main \
  | grep -v -E '\.md$|^docs/|^vendor/|_test\.|\.test\.|\.spec\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' \
  | awk '$1 > 350 { print "❌ " $2 ": " $1 " lines (limit 350)" }'
```

Non-empty output = hard fail.

#### fn/method sub-cap is a separate check

> [DETERMINISTIC → TODO-CHECK]

`dispatch_length_gate` enforces only the **file** cap (FLL). The **fn ≤ 50 / method ≤ 70** sub-cap is mechanizable but has no leaf wired — flagged TODO-CHECK and named explicitly (not folded into the 350 file cap), per the dispatch acceptance criteria (§13).

### Logging Gate (LOGGING GATE)

> [DETERMINISTIC → LOG]


**Family:** Observability discipline. **Source:** `docs/LOGGING_STANDARD.md`.

**Triggers** — every `Edit`/`Write` that adds, removes, or changes a log emit:

- `*.zig` outside `vendor/`/`third_party/`/`.zig-cache/`/`*_test.zig` — `std.log.*`, `std.debug.print`, raw stderr writes, calls into the `log` named module (source: `src/logging/mod.zig`).
- `*.ts`/`*.tsx`/`*.js`/`*.jsx` outside `vendor/`/`node_modules/`/`*.test.*`/`*.spec.*` — `console.*`, custom logger calls.
- `*.sh` outside generated dirs — `echo`/`printf` to `&2`.

**Override:** `LOGGING GATE: SKIPPED per user override (reason: ...)`. **User-invokable only.** Auto-mode does NOT cover this override.

#### What this gate covers

> [DETERMINISTIC → LOG]

`docs/LOGGING_STANDARD.md` codifies the wire format (logfmt), severity ladder, error-code embedding, scope discipline, and PII redaction. Drift is silent until incident response hits ungreppable logs at 3 AM.

#### Pre-edit check

> [DETERMINISTIC → LOG]

| Pattern | Rule |
|---|---|
| New `logging.scoped(.tag)` call | Scope is a Zig enum literal — adding a new tag is freeform. `event` must be snake_case `verb_noun`. |
| New `err`/`warn` log mapping to a domain failure | `error_code=UZ-XXX-NNN` field required. Registry entry must land in same commit. |
| Per-iteration / hot-loop log | Use `debug` (hidden by default), not `info`. |
| `info` level | Event must appear in the `info` allow-list (see `LOGGING_STANDARD.md` §10A.L4). Otherwise downgrade to `debug` or justify. |
| `console.log`/`std.debug.print` in non-test source | Forbidden. Convert to logger or delete before commit. |
| `std.log.scoped` outside `src/logging/` | Migration target. The `log` named module's `scoped` API is the only non-test entry point; flip the file's alias and migrate every call site in the same commit. |
| `msg=` field | ≤ 300 chars. Stack traces emit as separate `event=stack_trace` debug record. |
| Multi-line values | Newlines must be `\n` literal (two chars), not raw newline byte. |

Verify each rule applies or is N/A for this edit.

#### Required output (default — one line)

> [DETERMINISTIC → LOG]

```
LOGGING GATE: <file> | scope:<ok|new:.tag> event:<ok|new:.event> error_code:<ok|N/A> severity:<ok|escalation> redaction:<ok|N/A>
```

Comment-only edit:

```
LOGGING GATE: <file> | comment-only | N/A
```

Full multi-line block fires when a sub-rule reports a violation:

```
LOGGING GATE: <file>
  LOGGING_STANDARD.md sections consulted: §3 (wire format), §4 (severity), §5 (error codes), §6 (PII), §7 (zig binding) | §8 (TS binding), §10A (tightenings)
  Wire format: <logfmt ✓ | violation: <where>>
  Required keys: <ts_ms,level,scope,event present ✓ | violation: <missing>>
  Severity choice: <within rules ✓ | violation: <e.g. info on per-iteration path>>
  Error-code embedding: <UZ-XXX-NNN present and registered ✓ | orphan: <code> | missing on err/warn>
  PII discipline: <no secret materials ✓ | violation: <where>>
  Field caps: <≤15 fields, msg≤300 chars ✓ | violation: <count>>
  Newline encoding: <\n literal ✓ | raw newline at <line>>
  Audit script: <logging.sh on staged diff: 0 findings ✓ | N findings>
```

#### Scope (M70)

> [DETERMINISTIC → LOG]

`logging.sh` walks the **full `src/` + `agentsfleet/src/` working tree** via `git ls-files`. The index includes staged-but-not-yet-committed content, so a fix staged in pre-commit satisfies the check on the same hook run. `--staged` is preserved as an opt-in narrowing mode for iterative dev.

#### End-of-turn audit

> [DETERMINISTIC → LOG]

`audits/logging.sh` runs as part of `make lint`. Mechanical enforcement; reviewer responsibility for severity-level subjective calls and PII spot-checks (allow-list and msg-length are mechanical).

#### Family

> [DETERMINISTIC → LOG]

- `docs/LOGGING_STANDARD.md` — full standard, including §10A tightenings.
- Selected language façades carry their language-specific logging discipline;
  this gate supplies the shared checks.
- the Error Registry Gate section of this façade — pairs with this gate on `error_code=` audits.

### Milestone-ID Gate (MILESTONE ID)

> [DETERMINISTIC → MSID]


**Family:** Source-rot prevention. **Source:** `AGENTS.md` (project-side guard). Related: **RULE TST-NAM** (test identifiers are milestone-free) in `docs/greptile-learnings/RULES.md`.

**Triggers — before saving any file matching:**

- `**/*.zig` · `**/*.sql` · `**/*.ts` · `**/*.tsx` · `**/*.js` · `**/*.jsx` · `**/*.py` · `**/*.rs` · `**/*.go` · `**/*.sh`
- Any config file (`*.toml`, `*.yaml`, `*.json`) outside `docs/`
- Test files (the `_test.` / `.test.` / `.spec.` naming doesn't exempt — tests are code).

**Exempt paths** (IDs allowed): `docs/`, `**/*.md` outside `node_modules/`/`vendor/`, `CLAUDE.md`/`AGENTS.md`/`audits/agents-md.md`.

**Override:** `MILESTONE ID ALLOWED per user override (reason: ...)` in the immediately-preceding comment line.

#### Why

> [DETERMINISTIC → MSID]

Milestone IDs (`M{N}_{NNN}`), section refs (`§X.Y`), and dimension tokens (`T7`, `dim 5.8.15`) belong in specs, PR descriptions, and scratchpads — never in source code, since the codebase outlives any individual milestone and these references rot.

#### Pre-edit check

> [DETERMINISTIC → MSID]

Grep about-to-save content for:

```
M[0-9]+_[0-9]+
§[0-9]+(\.[0-9]+)+
\bT[0-9]+\b
\bdim [0-9]+\.[0-9]+\b
```

If any match, **strip the reference before saving.** Rewrite to describe the code's purpose, not its spec lineage.

#### Self-audit (end-of-turn)

> [DETERMINISTIC → MSID]

Covered by the combined awk audit in the AGENTS.md CONFORM section.

### Error Registry Gate (ERROR REGISTRY GATE)

> [DETERMINISTIC → ERR]


**Family:** Observability discipline. **Source:** `docs/LOGGING_STANDARD.md` §5; canonical registry in `src/errors/error_registry.zig` (per-project, not in dotfiles).

**Triggers** — every `Edit`/`Write` that:

- Adds or modifies `error_code=UZ-XXX-NNN` in any source file under `src/**` or `agentsfleet/**`.
- Edits `src/errors/error_registry.zig`.
- Touches HTTP error responses, executor frames, or CLI error surfaces.

**Override:** `ERROR REGISTRY GATE: SKIPPED per user override (reason: ...)`. **User-invokable only.** Auto-mode does NOT cover this override.

#### What this gate covers

> [DETERMINISTIC → ERR]

Every `err`/`warn` log mapping to a domain failure must carry an `error_code=UZ-XXX-NNN` registered in `src/errors/error_registry.zig`. Every CLI / HTTP / executor error surface must emit codes from the same registry. Drift produces ungreppable error chains across layers — operator can't trace `UZ-EXEC-012` from the CLI back to the executor frame back to the docs.

#### Pre-edit check

> [DETERMINISTIC → ERR]

| Pattern | Rule |
|---|---|
| New `error_code=UZ-XXX-NNN` reference | Registry entry exists in `src/errors/error_registry.zig` in the same commit. **Blocking.** |
| Adding a registry entry | Code follows `UZ-<CATEGORY>-<NNN>` pattern (categories: AUTH, EXEC, HTTP, REDIS, DB, CLI, EXTERNAL, INTERNAL). Hint and docs URL fields populated. |
| Removing a registry entry | All references removed in same commit (RULE ORP — orphan sweep). |
| Registry entry with no references in `src/**` or `agentsfleet/**` | Informational — declared-but-unreferenced (dead code). |
| Reference in source with no registry entry | Blocking — used-but-undeclared (orphan). |
| HTTP error response | Body includes `code: "UZ-XXX-NNN"`. |
| `agentsfleet` error output | Human format: `error UZ-XXX-NNN: <message> (<context>)`. JSON format: `{ "code": "UZ-XXX-NNN", "message": ..., ... }` per `LOGGING_STANDARD.md` §8. |

#### Required output (default — one line)

> [DETERMINISTIC → ERR]

```
ERROR REGISTRY GATE: <file> | new-codes:<list|none> orphans:<0|N> dead-codes:<0|N informational> hint:<populated|missing>
```

Comment-only edit:

```
ERROR REGISTRY GATE: <file> | comment-only | N/A
```

Full multi-line block fires on violation:

```
ERROR REGISTRY GATE: <file>
  Registry: src/errors/error_registry.zig
  New codes added (this diff): <UZ-XXX-NNN, ...>
  Code references added (this diff): <UZ-XXX-NNN at <file>:<line>, ...>
  Orphan codes (used but not declared): <list — BLOCKING>
  Dead codes (declared but unreferenced): <list — informational>
  Hint/docs populated: <ok ✓ | missing for <code>>
  Audit script: <error-codes.sh on staged diff: 0 findings ✓ | N findings>
```

#### Scope (M70)

> [DETERMINISTIC → ERR]

`error-codes.sh` walks the **full `src/` + `agentsfleet/` working tree** via `git ls-files`. The index includes staged-but-not-yet-committed content, so a fix staged in pre-commit satisfies the check on the same hook run. `--staged` is preserved as an opt-in narrowing mode for iterative dev.

#### End-of-turn audit

> [DETERMINISTIC → ERR]

`audits/error-codes.sh` runs as part of `make lint`. Greps `src/errors/error_registry.zig` for declared codes, then greps `src/**` and `agentsfleet/**` for any `UZ-[A-Z]+-[0-9]+` literal that isn't in the registry. Reports orphans (blocking) and dead codes (informational).

#### Family

> [DETERMINISTIC → ERR]

- `docs/LOGGING_STANDARD.md` §5 — error-code embedding rules in log records.
- the Logging Gate section of this façade — pairs with this gate on `error_code=` audits.
- `docs/REST_API_DESIGN_GUIDELINES.md` — HTTP error response shape includes `code`.
- `docs/greptile-learnings/RULES.md` RULE ORP — orphan sweep on rename/delete.

### UFS Gate (Unified Form for Symbols)

> [DETERMINISTIC → UFS]


**Family:** Constant discipline. **Source:** `AGENTS.md` conformance enforcement
+ the selected language façade's Uniform Free Strings (UFS) clauses.

**Triggers** — every `Edit`/`Write` to source files under `src/`, `ui/packages/*/src/`, `ui/packages/*/app/`, `ui/packages/*/lib/`, `ui/packages/*/components/`, `ui/packages/*/pages/`, `ui/packages/*/tests/`, `agentsfleet/src/`, `agentsfleet/test/` matching `*.zig`, `*.ts`, `*.tsx`, `*.js`, `*.jsx`. Excluded: `vendor/`, `third_party/`, `.zig-cache/`, `node_modules/`, `*.tsbuildinfo`.

**Override:** `UFS GATE: SKIPPED per user override (reason: ...)` immediately preceding the edit. **User-only**; auto-mode does not cover.

**Carve-out:** **Pin tests** keep their literals if and only if the literal IS the contract under test (`expect(formatDollars(4_710_000_000)).toBe("$4.71")`). Each pin must carry an inline `// pin test: literal is the contract` comment on or above the line.

#### What this gate enforces

> [DETERMINISTIC → UFS]

Three discipline points, all part of RULE UFS:

1. **Repeat literals → named constant.** Any string literal appearing ≥2 times in the same file (or in the same module/package — see scope ladder below) becomes a named const. Wire-format enum values (`"platform"`, `"self_managed"`, `"receive"`, `"stage"`, mode strings, posture labels, channel names, queue names, error code categories) are *always* named — even at first use, even in test fixtures.

2. **Semantic numeric literals → named constant.** Numeric literals carrying meaning beyond their digits become named consts even at first use. Includes: conversion factors (powers-of-ten ≥ 1e3 used as multipliers/divisors), thresholds, sub-cent rates, time-unit factors, byte-unit factors, retry caps, sentinel offsets. Bare `1_000_000_000` in `nanos / 1_000_000_000` is a smell — `nanos / NANOS_PER_USD` is the rule.

3. **Cross-runtime parity.** Any constant that exists in more than one runtime (Zig, TS, JS) **must share its identifier verbatim**, modulo case where a language idiom forces it (Zig snake_case → TS/JS SCREAMING_SNAKE is a single-runtime style choice; the *semantic* name is identical). When a literal in runtime A would benefit from a name and a sibling runtime already has that name, reuse it. When introducing a new constant, define it in every runtime that uses the same value, same commit. There is no per-constant carve-out: this applies to every cross-runtime constant, not a curated subset.

The gate exists because RULE UFS is otherwise pure self-policing. Without an audit script, an inline literal in a 800-LOC editing session reliably slips past attention.

#### Pre-edit checks

> [DETERMINISTIC → UFS]

Before saving an edit that introduces or relocates a literal:

1. **Same-file repeat?** Grep within the file for the literal value. If it appears ≥1 time already, define a const at the top of the file (or in the module's `constants.ts`/`constants.zig`) and replace all occurrences.

2. **Same-module repeat?** Grep within the module/package (`ui/packages/app/`, `agentsfleet/src/`, `src/`). If repeated, define in the module's canonical types/constants file (`lib/types.ts`, `src/state/<topic>.zig`, `agentsfleet/src/constants/<topic>.js`).

3. **Cross-runtime sibling?** When the literal is a wire-format string or unit-conversion numeric, grep all three runtimes for an existing matching name. If found, reuse the name. If not, define in every runtime that uses the same value, same commit.

4. **Carve-out check?** If the literal IS the contract a pin test verifies, write the `// pin test: literal is the contract` comment on or above the line. Otherwise, name it.

#### Required output (per Edit/Write — one line by default)

> [DETERMINISTIC → UFS]

```
UFS GATE: <file> | repeats:<count> | semantic-numerics:<count> | cross-runtime:<status> | pin-test-carve-outs:<count>
```

Multi-line block fires when repeats > 0 OR semantic-numerics > 0 OR cross-runtime status ≠ ok:

```
UFS GATE: <file>
  Repeat string literals named: <list of value → CONSTANT_NAME pairs>
  Repeat string literals KEPT (carve-out): <value> — <reason: pin test / single-use semantically irreducible / etc.>
  Semantic numerics named: <list of value → CONSTANT_NAME pairs>
  Semantic numerics KEPT: <value> — <reason>
  Cross-runtime parity: <status — e.g. "NANOS_PER_USD added to Zig+TS+JS this commit" or "n/a — runtime-internal constant">
```

#### Scope (M70)

> [DETERMINISTIC → UFS]

`ufs.sh` walks the **full working tree** via `git ls-files`. The index includes staged-but-not-yet-committed content, so a fix staged in pre-commit satisfies the check on the same hook run.

The previous `--diff` (`BASE...HEAD`) default was retired with M70. The forcing function was M68 commit `02c1f3cf`: pre-commit's `HEAD` is the prior commit, so `BASE...HEAD` was blind to nine cross-runtime mismatches the agent had staged but not yet committed. The full-codebase scope removes that blindspot.

`--all` is accepted as a back-compat alias for the default. `--diff` is rejected with exit 2 + a pointer to this section.

#### Self-audit (end-of-turn / CONFORM)

> [DETERMINISTIC → UFS]

`audits/ufs.sh` is invoked at CONFORM by the agent. Convention follows the sibling audit scripts (`logging.sh`, `error-codes.sh`, `deinit-pairs.sh`, `spec-template.sh`) — none of them wire into `make lint`; the agent runs each as part of the gate ceremony, with the result feeding the CONFORM table. The script is generic — no manifest of known literals — so it scales as the codebase grows:

```bash
bash audits/ufs.sh          # full-codebase scan (default and only mode)
bash audits/ufs.sh --all    # alias for default
```

The script emits a violations table. Empty table = pass. Each row is one of:

- `string-dup-file <file> "<literal>" <count>` — same string ≥2× in one file
- `string-dup-module <module> "<literal>" <count>` — same string ≥2× across a module/package
- `numeric-suspect <file>:<line> <literal>` — power-of-ten or known unit-factor numeric not bound to a const (excludes anything on a `// pin test: literal is the contract` line)
- `cross-runtime-orphan <const_name> <runtime>` — SCREAMING_SNAKE constant defined in one runtime, missing from a sibling runtime that the same diff touches

Violations resolve by either (1) extracting to a named const + replacing all sites, (2) adding the constant in the missing sibling runtime same-commit, or (3) adding the `// pin test: literal is the contract` comment.

#### CONFORM row

> [DETERMINISTIC → UFS]

```
| UFS GATE             | ✅ pass | 🟡 N violations addressed | 🔴 N unresolved |
```

🔴 unresolved → return to EXECUTE; the lifecycle does not advance.

#### Failure mode this gate exists to prevent

> [DETERMINISTIC → UFS]

Indy's M66_001 §3 tail surfaced the failure clearly: `RULE UFS` lived as a single bullet in the former Bun rules §2 with no audit, no CONFORM row, and no per-edit ceremony. Across an 800-LOC session the agent introduced ~20 inline literals (`"platform"`, `"self_managed"`, `"receive"`, `"stage"`, `mode: "byok"`, `1e9`, `1_000_000_000`, etc.) that the rule already covered but no mechanism caught. The rule is fine; the enforcement was missing. This gate adds the enforcement.

### Greptile Gate (GREPTILE GATE)

> [JUDGMENT → GRP]


**Family:** Universal coding-rules audit. **Source:** `docs/greptile-learnings/RULES.md`.

**Triggers:** fires twice per work unit — (1) per EXECUTE iteration when diff languages change (new layer/language enters the diff); (2) end-of-turn, before claiming complete.

**Override:** `GREPTILE GATE: SKIPPED per user override (reason: ...)`. Violations stay in the diff and the user's override gets recorded in the spec's Discovery section.

#### What this gate enforces

> [JUDGMENT → GRP]

`docs/greptile-learnings/RULES.md` is the universal coding-rules catalogue. The agent failure mode is grepping the spec verbatim instead of grepping the rules; this gate makes the rule audit a printable artefact.

#### Common rules referenced

> [JUDGMENT → GRP]

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

#### Pre-print check

> [JUDGMENT → GRP]

1. Identify diff languages (`zig|ts|tsx|sql|sh|py|go|rs`).
2. List rules whose `Applies to` overlaps. UFS applies to all source; STS to SQL; EMS to Zig handlers; etc.
3. **String-literals (UFS) audit:**

   ```bash
   git diff -U0 origin/main | grep -oE '"[^"]{4,}"' | sort -u
   ```

   For each unique literal, grep `src/ ui/ agentsfleet/` for an existing `const` / `pub const` / `export const` / `as const` / `Final[str]` / `readonly` declaration. Found → import. Novel + ≥2 sites → declare a const.

#### Required output (one row per applicable rule, suppress non-applicable)

> [JUDGMENT → GRP]

```
GREPTILE GATE: <iteration tag>  Diff langs: <…>
| Code     | Verdict                                |
| UFS      | clean | N violations: <list>           |
| STS      | clean | N violations                   |
| …        | …                                      |
String-literals audit: <N literals scanned, M violations>
```

#### Anti-rationalisation clause

> [JUDGMENT → GRP]

"It's just a label" / "I'll only use it once" are not exceptions to UFS. The clause applies to every rule with a behaviour modifier — when you find yourself reaching for a justification, the rule wins.

### RULE NLR — No Legacy Retained

> [JUDGMENT → NLR]


**Family:** Legacy-control. Sibling rules: **RULE NDC** (no dead code at write time), **RULE NLG** (no new legacy framing pre-v2.0.0), **Legacy-Design Consult Guard** (judgment calls on whole subsystems). **Source:** `docs/greptile-learnings/RULES.md` RULE NLR.

**Triggers:** any Edit/Write to a file containing pre-existing legacy framing or dead code.

**Override:** `RULE NLR: SKIPPED per user override (reason: ...)` immediately preceding the edit. **User-invokable only.** Concrete external-impact constraint OR explicit NLR DECISION resolution required. Generic "scope creep"/"too big" not valid.

#### What this gate enforces

> [JUDGMENT → NLR]

Any edit to a file with pre-existing legacy framing or dead code MUST remove it in the same diff. Patterns covered:

- `?*T = null` fields whose only caller always sets a non-null value (dead defense for a phantom caller).
- `legacy_*` symbol names, `V2`-suffixed twin types, `if (legacy_caller)` branches.
- `// legacy` / `// pre-M*` / `// bootstrap` comments.
- Runtime warns saying `legacy path` / `deprecated` / `*_bootstrap_*`.
- `pub` fns/fields with no in-tree consumer (verify with `grep -rn`).
- `defer if (x) ... else null` patterns that compensate for an `?T` that should have been `T`.
- Unused parameters, captures, unreachable branches.

The carve-out "pre-existing violations are not the agent's responsibility" does **not** apply when the agent is already touching the file. You see it, you own it.

The inverse discipline on the same touch: **performance opportunities are surfaced, never silently bundled.** A behavior-preserving refactor that quietly folds in optimizations (pooling a per-call construction, collapsing allocations on a hot path) destroys bisectability of any regression. Keep a perf-opportunities list as you edit and surface it at the next checkpoint for the user's fold-in-or-file decision — same posture as security follow-ups. Dead-code removal on touch stays in (that is this rule); perf rewrites stay out until decided.

#### Pre-edit check

> [JUDGMENT → NLR]

1. Before the first `Edit`/`Write` to a file, scan for the patterns above in the *whole file*, not just the lines you're touching.
2. List violations in the gate output before the edit.
3. Remove them in the same diff. Update every caller in the same commit.
4. If cleanup is judged infeasible (large net-line delta, cross-package cascade, meaningfully different design path emerges), print the **NLR DECISION** block below and **wait**. The agent has no autonomous escape; the user is the only authority.

#### Decision block

> [JUDGMENT → NLR]

```
NLR DECISION: <file>
  Cleanup-in-place cost: +<N> net lines, <M> files touched.
  Alternative approach: <one-line description> (avoids the dirty file).
  If alternative chosen, legacy that survives:
    - <symbol/pattern>: <file:line>
    - ...
  WAITING for user: clean / alternative.
```

#### Required output (when violations found before edit)

> [JUDGMENT → NLR]

```
NLR: <file> | violations: <list with file:line each> | action: clean-in-diff
```

#### Anti-evasion clause

> [JUDGMENT → NLR]

Three failure modes the agent MUST NOT use to skip cleanup:

1. **Route-around design.** Picking an architecture that avoids the dirty file specifically to dodge cleanup. If NLR avoidance was a motivation, surface it.
2. **Silent rejection.** Quietly choosing not to touch a file because cleanup looked expensive, without disclosing the decision.
3. **Shim-and-skip.** Introducing a wrapper/adapter to sidestep a dirty interface — the new code becomes its own legacy debt; the original rots in place.

If any of these patterns is in play, surface it via the NLR DECISION block. The user retains all discretion.

#### Family

> [JUDGMENT → NLR]

- **RULE NDC** — prevention at write time (don't author dead code).
- **RULE NLG** — ban new legacy framing pre-v2.0.0.
- **Legacy-Design Consult Guard** — harder judgment calls ("should this whole subsystem exist") that need user input.
- **RULE NLR** — mechanical cleanup on touch.

### RULE NLG — No new Legacy framing

> [JUDGMENT → NLG]


**Family:** Legacy-control. Sibling rules: **RULE NDC**, **RULE NLR**, **Legacy-Design Consult Guard**. **Source:** `docs/greptile-learnings/RULES.md` RULE NLG.

**Triggers:** introducing any new `legacy_*` name, `V2`-twin type, `if (legacy_caller)` branch, backward-compat shim, command-line alias for an old verb or flag, "rejecting legacy X" prose, or violation tracking-list while `cat VERSION` < `2.0.0`.

**Override:** `RULE NLG: SKIPPED per user override (reason: ...)` immediately preceding the edit. **User-invokable only.** Requires a concrete external consumer that can't migrate same-commit (vanishingly rare pre-v2.0.0).

#### What this gate enforces

> [JUDGMENT → NLG]

While `cat VERSION` < `2.0.0`, the project has no external consumers and no published API. Do not introduce *new* legacy concepts in any form:

- No `legacy_*` error variant names.
- No `if (legacy_caller)` branches.
- No `V2`-suffixed twin types.
- No backward-compat shims.
- No "rejecting legacy X" prose in specs/docs/commit messages.
- No old command verbs or flag aliases retained beside the new form unless Indy explicitly asks in the same session.

Edit interfaces in place; update every caller in the same commit. Name errors by *what is wrong*, not *when it was wrong* (e.g. `runtime_keys_outside_block`, not `legacy_top_level_runtime`).

#### Why

> [JUDGMENT → NLG]

Pre-alpha duplicates rot faster than documentation. Every `legacy_*` name introduces a phantom interface nobody owns; every future spec then has to reason about it. Schema Table Removal Guard already encodes this for SQL — RULE NLG generalises it to every interface (RPC, route, struct, error name, config key, spec prose).

#### Tracking-list ban

> [JUDGMENT → NLG]

Any constant, doc structure, or carve-out list whose purpose is to catalog "violations to be cleaned up later" is itself an NLG violation. Banned name patterns: `LEGACY_`, `PENDING_`, `_VIOLATIONS`, `_CARVE_OUTS` (when meant for deferred cleanup), `TO_FIX_`, `DEFERRED_`, or any equivalent.

Either fix every entry in the same diff that introduces or touches the list, or delete the list and let the next touch fix the underlying violations. The tracking list **legitimises deferral**; the rule exists to prevent that.

**Vendor-immortal carve-outs** — paths or names dictated by external commitments that genuinely cannot be renamed (e.g. OAuth callback URLs that Slack/GitHub register with us) — are a separate class. Name those explicitly with `VENDOR_` or `EXTERNAL_` prefix so the distinction from "deferred cleanup" is mechanical, and add a comment line stating the external commitment that pins the name.

#### Required output (when violation found)

> [JUDGMENT → NLG]

```
NLG: <file>:<line> | new legacy framing: <description> | action: rename | suggested name: <what-is-wrong-form>
```

#### Full text

> [JUDGMENT → NLG]

`docs/greptile-learnings/RULES.md` RULE NLG.

### Legacy-Design Consult Guard

> [JUDGMENT → LDC]


**Family:** Legacy-control. Sibling rules: **RULE NDC**, **RULE NLR**, **RULE NLG**. **Source:** `AGENTS.md` (project-side guard, not a greptile-derived rule).

**Triggers** — STOP and consult before any of these:

- Patching legacy to fit new architecture ("compensating code").
- Keeping legacy for "backward compat" pre-alpha when there are no external consumers.
- Defensive `orelse` / fail-open whose only reason is legacy nullability.
- Authoring tests that exercise the legacy path.
- Choosing patch-vs-remove silently — never your call.

**Override:** none — user decides A/B/C in the consult block.

#### Definition — "legacy design"

> [JUDGMENT → LDC]

Any code path, env-var, table, route, or API that the surrounding milestone work is deprecating, that predates the current architectural direction, or that exists solely as a smoke-test / bootstrap / pre-migration shim. Signals:

- Comments like `// legacy`, `// pre-M*`, `// bootstrap`, `// TODO remove`, `// temporary`.
- Runtime warn logs that announce themselves (`legacy path`, `deprecated`, `*_bootstrap_*`).
- Env vars / principals / roles / schema cols whose only live consumer is a fallback branch or pre-signup/dev-only path.

#### Required output

> [JUDGMENT → LDC]

```
LEGACY CONSULT: <desc> | found:<file:line> | (A) remove [blast:<files>] / (B) patch [risk] / (C) keep [why] | rec:<A|B|C> because <reason> | WAITING.
```

Block on the user's reply. If the user previously approved one *class* of legacy decisions this session, note that and proceed — but every *new* class of finding still triggers a consult.

#### Escape hatch

> [JUDGMENT → LDC]

Legacy findings unambiguously in-scope of the active spec's Dead Code Sweep or Out-of-Scope list skip the consult and follow the spec.

#### Discovery capture

> [JUDGMENT → LDC]

Every triggered consult is logged in the active spec's **Discovery** section, or filed as a new pending spec in `docs/v{N}/pending/` if pushed to follow-up.

#### Family

> [JUDGMENT → LDC]

NLR is the cleanup-on-touch arm; NLG bans new legacy framing pre-v2.0.0; this guard covers the harder judgment calls ("should this whole subsystem exist") that need the user's input.

## Comment Voice — New to the Codebase, Familiar with the Goal

> [JUDGMENT → NEW:CMT]

Indy's standing instruction (Jul 2026): write comments for a reader who is **new
to the codebase but familiar with the goal of the project**. That reader knows
WHAT the product does and why it exists; they don't know where things live, which
invariants bind, or why this file deviates from the obvious approach.

- **Explain the local WHY** — the constraint, invariant, or cross-module
  interplay the code alone can't show (*"drained before deinit because the pool
  recycles the conn"*, not *"call drain then deinit"*).
- **Name the non-obvious neighbours** — when correctness depends on a sibling
  file, registry, or cross-runtime pairing, say so by path/symbol. That map is
  exactly what a codebase-newcomer lacks.
- **Don't narrate the language or restate the line** (*"increment i"*), and
  **don't re-explain the product goal** the reader already has.
- **Review-context comments stay banned** — nothing about what changed, which
  spec asked for it (MILESTONE-ID gate), or why the diff is correct; that is
  PR/Session-Notes material, not code. Acronym expansion applies inside comments
  (`AGENTS.md` §Owner & Style).

## Mid-Task Repo-Wide Flips Need a Blast-Radius Grep

> [JUDGMENT → NEW:BLAST]

Before flipping or removing any identifier/value referenced beyond the file in hand — a skill ref, package name, template slug, config key — run `git grep -rn -w '<token>'` from the repo root with **no path filter**, even for a "one-line change". Word-boundary, never quoted-literal (YAML and multiline-string refs carry no quotes). Separate true targets from same-spelling-different-meaning hits (a skill ref vs the GitHub repo slug) and update every true hit in the same diff. Grepping only the "obvious" files is the trap: an installer rename can leave a simultaneously edited document advertising the old slug. A selected specification façade applies the same rule to teardown and rename specs.
