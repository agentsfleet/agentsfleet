<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M73_001: Retire `error-codes` mirror + audit allowlist exception

**Prototype:** v2.0.0
**Milestone:** M73
**Workstream:** 001
**Date:** May 17, 2026
**Status:** PENDING
**Priority:** P2 — quality / maintainability. Removes one friction point (allowlist exception) and one drift surface (hand-maintained per-language mirror). Not customer-facing.
**Categories:** CLI, INFRA
**Batch:** B1
**Branch:** feat/m73-error-code-mirror-retirement (to be created on CHORE(open))
**Depends on:** None. Sits alongside any in-flight TypeScript migration; does not block it.
**Provenance:** human-written from `HANDOFF_ERROR_CODE_MIRROR_RETIREMENT_SPEC.md` (Captain ask, May 17, 2026).

**Canonical architecture:** N/A — this is a build / hygiene spec; the canonical content is the Zig error registry at `src/errors/error_registry.zig` + `src/errors/error_entries.zig`.

---

## Implementing agent — read these first

1. `scripts/audit-error-codes.sh` (symlinked from `~/Projects/dotfiles/scripts/audit-error-codes.sh`) — walks `*.zig` + `zombiectl/src/` for raw `UZ-<CAT>-<NNN>` literals; fails commit on any hit outside the allowlist. Read both the regex (line ~15) and the file-walk logic.
2. `src/errors/error_registry.zig` + `src/errors/error_entries.zig` — the canonical source. Every UZ code lives here.
3. `zombiectl/src/constants/error-codes.js` — the file being retired. ~25 `export const ERR_*` declarations, each mapping a named symbol to its UZ string.
4. `zombiectl/src/lib/error-map-presets.js` — the **second** file with UZ literals (as map keys). The under-discussed second exception; this spec must address it too or the allowlist entry just relocates.
5. `zombiectl/src/commands/auth.js` — uses three ERR_* symbols in equality comparisons inside `zombiectl auth status` (D31 from M68). One of two confirmed consumers; grep for more before committing to a migration plan.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal discipline. **"Fix the code, not the harness"** is the load-bearing rule for this spec — the whole spec exists because the prior agent's patch-the-audit instinct was the wrong move.
- `docs/ZIG_RULES.md` — applies only if Path 1's codegen interface lands as `zig build emit-error-registry-json`.
- `docs/SCHEMA_CONVENTIONS.md`, `docs/REST_API_DESIGN_GUIDELINES.md` — N/A.

---

## Overview

**Goal (testable):** `scripts/audit-error-codes.sh` regex contains no path entry inside `zombiectl/src/`. `zombiectl` source tree contains no hand-maintained UZ-* string literals. Existing consumers of `ERR_*` symbols continue to compile and pass `bun test`.

**Problem:** The CLI mirror exists because JavaScript cannot import from `.zig`. The Zig server defines codes in `src/errors/error_registry.zig`; the CLI needs to recognize the same codes when they come back over HTTP; the only mechanism today is a hand-maintained parallel set of `export const ERR_*` declarations in `zombiectl/src/constants/error-codes.js`. This creates three problems:

1. **Audit allowlist exception.** The file is the one place in the CLI tree allowed to contain raw UZ-* literals. Every edit to `scripts/audit-error-codes.sh` (e.g. the recent TypeScript migration trying to widen `error-codes\.js$` to `error-codes\.(js|ts)$`) risks touching the entry. The exception is a forever-friction point.
2. **Drift risk.** New error codes in the Zig registry require a human to remember to update the CLI mirror. No compile-time link. Future services in other languages multiply the problem.
3. **`error-map-presets.js` carries the same smell.** Even if `error-codes.js` is deleted, the preset map contains UZ-* strings as keys. That file would need an allowlist entry — the exception is moved, not eliminated.

**Solution summary:** Pick one of three structural paths (codegen / fold-into-presets / no-mirror), surface trade-offs to Captain at plan-eng-review, then execute the picked path. All three paths share the same acceptance criteria: zero raw UZ-* literals in hand-maintained `zombiectl/src/` files, allowlist entry retired (or relocated only to a generated artifact whose path is excluded from the audit walk).

---

## Files Changed (blast radius)

> Exact file list depends on path chosen. Below names every file in scope.

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/constants/error-codes.js` | DELETE (Path 1, 3) / REWRITE (Path 2) | The mirror itself. |
| `zombiectl/src/lib/error-map-presets.js` | EDIT or SPLIT | Carries UZ literals as map keys. Path 1 splits into generated keys + hand-maintained semantics; Path 2 absorbs `error-codes.js`; Path 3 deletes outright. |
| `zombiectl/src/commands/auth.js` | EDIT | Three `code === ERR_*` comparisons. Migrate per chosen path (re-import from new path / compare against tags / use generated symbols). |
| `zombiectl/scripts/generate-error-codes.mjs` | CREATE (Path 1) | The codegen script. |
| `build.zig` | EDIT (Path 1, only if Q1 resolves to "Zig emits JSON sidecar") | Adds `zig build emit-error-registry-json` step. |
| `zombiectl/package.json` | EDIT (Path 1) | `prebuild` / `postinstall` script wiring. |
| `zombiectl/.gitignore` | EDIT (Path 1) | Ignore `src/constants/error-codes.generated.ts`. |
| `scripts/audit-error-codes.sh` (in dotfiles) | EDIT | Remove `^zombiectl/src/constants/error-codes\.js$` from allowlist. Path 2 replaces with the consolidated file path. |
| Other consumers (grep first) | EDIT | Any file importing `ERR_*` from `constants/error-codes.js` follows the import-path or symbol change. |

---

## Sections (implementation slices)

### §1 — Pick the path

Captain decides at plan-eng-review. The three paths and their trade-offs are documented in *Open Questions* (verbatim from the HANDOFF) — present them; do not silently resolve. Implementation does not start until §1 closes.

### §2 — Execute the chosen path

Three independent implementation slices, one per path. Only one runs.

**Path 1 — Build-time codegen.** Adds `zombiectl/scripts/generate-error-codes.mjs` reading the Zig registry (via direct parse or a JSON sidecar emitted by `zig build emit-error-registry-json` — see Q1). Emits `zombiectl/src/constants/error-codes.generated.ts` with the same shape as today's hand-maintained file. Wires as `prebuild`/`postinstall`. Adds the emitted file to `.gitignore`. Drops the `error-codes\.js$` allowlist entry; narrows audit walk to `git ls-files`-tracked paths or rewrites it to ignore `*.generated.ts`. For `error-map-presets.js`: split into a generated keys file (no UZ literals in source) + a hand-maintained semantics file keyed by the generated symbols.

**Path 2 — Fold into a single semantic-rich mirror.** Make `error-map-presets.js` (or its TS replacement) the single canonical CLI-side mirror. Re-export the named symbols (`ERR_UNAUTHORIZED`, etc.) so existing consumers keep working with a one-character rename of the import path. Replace the `error-codes\.js$` allowlist entry with the consolidated file path. Doesn't eliminate drift; relocates the exception to the file that legitimately needs the literals.

**Path 3 — No CLI-side mirror at all.** The `runCommand` wrapper parses inbound `err.code` into a generic `{ tag, category }`; consumers compare against `tag === "UNAUTHORIZED"` instead of `code === ERR_UNAUTHORIZED`. File ceases to exist; allowlist entry retired. Loses compile-time signal when the server adds new codes.

### §3 — Migration of existing consumers

Single grep-driven pass: `grep -rln 'from.*constants/error-codes' zombiectl/src/` lists every consumer. Each one's rewrite is mechanical per the chosen path.

### §4 — Audit hardening

Beyond removing the allowlist entry: add a test (or extend the audit) that fails when a new UZ-* literal lands in a non-allowlisted file. The test exists today in spirit (the audit fires); make it explicit and CI-gated so the regression cannot land silently.

---

## Interfaces

No HTTP, OpenAPI, or wire-protocol surface added. Internal interfaces touched depend on chosen path:

- **Path 1.** `zig build emit-error-registry-json` (new build target, only if Q1 resolves yes) producing a JSON file at a deterministic path. Schema: `{ codes: [{ name, code, category, description }] }`. Generated TS file shape is identical to today's hand-maintained `.js` shape so consumers do not change.
- **Path 2.** Imports from `zombiectl/src/lib/error-map-presets.js` (or its TS successor) expose `ERR_*` named exports identical to today's `constants/error-codes.js`.
- **Path 3.** `runCommand` return type adds `tag: string` and `category: string` derived from `UZ-<CAT>-<NNN>` parsing. Consumers no longer import `ERR_*`.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Codegen artifact accidentally committed (Path 1) | Author forgets `.gitignore` is recent and `git add .` picks up the generated file. | CI gate: `git ls-files | grep 'error-codes\.generated\.ts$'` must return empty. Pre-commit hook surfaces the violation. |
| Stale generated file on cold clone (Path 1) | `tsc --noEmit` runs before `prebuild`; the generated file does not exist. | Wire codegen as `postinstall`, not just `prebuild`. Document in `zombiectl/README.md`. |
| Tag-string typo (Path 3) | Stringly-typed comparisons (`tag === "UNAUTHORZIED"`) bypass type-check. | Add a const-tag enum: `export const TAG_UNAUTHORIZED = "UNAUTHORIZED"`. Still hand-maintained but only contains stringly tags, not UZ literals — moves the problem one layer but keeps the audit clean. |
| Drift between Zig registry and CLI mirror (Paths 2, 3) | Human edits `error_registry.zig` but forgets the CLI side. | Path 1 eliminates structurally; Paths 2 and 3 require a checklist or a CI test that imports both surfaces and asserts every Zig code has a CLI counterpart. |
| Audit script edit in dotfiles repo gets out-of-sync | Audit lives in `~/Projects/dotfiles/scripts/`, this spec edits it; the dotfiles commit might land before the usezombie code change. | Land both in lockstep: pre-merge gate validates `zombiectl/` has no UZ literals in non-allowlisted files. Order the dotfiles change AFTER the usezombie change to avoid a window where the audit fires on legitimate code. |

---

## Invariants

1. **Zero hand-maintained UZ-* literals in `zombiectl/src/`.** Enforced by `scripts/audit-error-codes.sh` after the spec lands (the audit walks tracked files; no hand-maintained file contains literals).
2. **Audit allowlist contains no path inside `zombiectl/src/`.** Enforced by grep on the audit script's regex as part of CI.
3. **Every server-defined UZ code that round-trips through the CLI is observable.** Enforced by an integration test (regardless of path) that simulates the server returning each code and asserts the CLI consumer recognizes it.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_audit_allowlist_clean` | `grep -E '\^zombiectl/src/' scripts/audit-error-codes.sh` returns zero matches. |
| `test_no_uz_literals_in_zombiectl` | `git ls-files zombiectl/src/ | xargs grep -l 'UZ-[A-Z]\+-[0-9]\+'` empty (excluding the generated artifact for Path 1). |
| `test_existing_consumers_compile` | `bun run lint` and `bun test` pass after migration. |
| `test_auth_status_known_codes` | `zombiectl auth status` recognises `UZ-AUTH-001`, `UZ-AUTH-002`, and `TOKEN_EXPIRED` and routes each to the correct user-facing message (regression on D31 from M68). |
| `test_codegen_round_trip` (Path 1 only) | The generated file's contents are byte-identical to a sample golden when run against a frozen registry snapshot. |
| `test_audit_blocks_new_violation` | Synthetic test that adds a UZ literal to a non-allowlisted file and asserts the audit fails. |

---

## Acceptance Criteria

- [ ] `scripts/audit-error-codes.sh` regex has no path entry inside `zombiectl/src/`. Verify: `grep -nE 'zombiectl/src/' scripts/audit-error-codes.sh` empty.
- [ ] No hand-maintained UZ literal in `zombiectl/src/`. Verify: `git ls-files zombiectl/src/ | xargs grep -l 'UZ-[A-Z]\+-[0-9]\+' | grep -v 'generated'` empty.
- [ ] `bun test` green in `zombiectl/`.
- [ ] `bun run lint` green in `zombiectl/`.
- [ ] (Path 1) `zombiectl/src/constants/error-codes.generated.ts` is in `.gitignore` and `git ls-files` does not list it.
- [ ] `zombiectl auth status` golden-path passes against the integration fixture.
- [ ] A regression test exists that fails the build if a new UZ-* literal lands in `zombiectl/src/` outside the allowlist.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: allowlist clean
grep -nE 'zombiectl/src/' scripts/audit-error-codes.sh && echo FAIL || echo PASS

# E2: no UZ literals in hand-maintained CLI source
git ls-files zombiectl/src/ | xargs grep -l 'UZ-[A-Z]\+-[0-9]\+' 2>/dev/null | grep -v 'generated' && echo FAIL || echo PASS

# E3: tests + lint
cd zombiectl && bun test && bun run lint

# E4: (Path 1) generated artifact not tracked
git ls-files zombiectl/src/constants/error-codes.generated.ts && echo FAIL || echo PASS
```

---

## Dead Code Sweep

Mandatory if Path 1 or Path 3 ships (both delete `zombiectl/src/constants/error-codes.js`):

| Deleted file/symbol | Grep | Expected |
|---------------------|------|----------|
| `constants/error-codes.js` | `grep -rn 'constants/error-codes' zombiectl/ ui/` | Zero matches (Paths 1, 3) or only `constants/error-codes.generated` matches (Path 1) |
| `ERR_*` symbols (if Path 3) | `grep -rn 'ERR_[A-Z_]\+' zombiectl/src/` | Zero matches |

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After plan-eng-review picks the path, before CHORE(close) | `/write-unit-test` | Confirms every test in §Test Specification has a runnable assertion; adds missing coverage. | Clean. |
| After tests pass, before CHORE(close) | `/review` | Adversarial pass against `docs/greptile-learnings/RULES.md` "Fix the code, not the harness" — does the chosen path actually fix the code, or did it work around the audit? | Findings dispositioned. |
| After `gh pr create` | `/review-pr` | Re-runs the audit on the immutable diff; catches any UZ literal that landed in a fix-up. | Comments addressed. |

---

## Discovery (consult log)

Empty at creation. Open questions in *Out of Scope / Open Questions* below need Captain decisions before EXECUTE.

---

## Open Questions (Captain decides at plan-eng-review)

- **Q1: Codegen interface (Path 1 only).** Does Zig already emit error registry as a JSON artifact, or do we add `zig build emit-error-registry-json`? Read `build.zig` before answering — if there is already an existing emit target close enough, reuse it.
- **Q2: Same retirement for the executor mirror?** `src/executor/client_errors.zig` has the same shape and the same allowlist entry. Is its retirement in-scope here or a follow-up spec? Recommendation: follow-up. Surface explicitly.
- **Q3: `error-map-presets.js` resolution.** If Path 1 wins, does the codegen also emit the map skeleton (no semantic content), or does the file stay hand-maintained with its own allowlist entry? Captain picks.
- **Q4: TypeScript migration timing.** Mid-TS migration is in flight. Does the emit ship as `.ts` (post-migration) or `.js` (independent)? Recommendation: `.ts`; flag the timing dependency.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Allowlist clean (E1) | see Eval | | |
| No UZ literals (E2) | see Eval | | |
| Tests + lint (E3) | `bun test && bun run lint` | | |
| (Path 1) artifact not tracked (E4) | see Eval | | |

---

## Out of Scope

- **Zig-side mirror retirement.** `src/executor/client_errors.zig` is a sibling problem; this spec scopes the CLI surface. See Q2.
- **AUTH.md updates.** Not applicable — this spec does not change auth surfaces.
- **Server-side error registry restructuring.** The canonical Zig registry stays as-is; only the CLI consumer changes.
- **Adding new error codes.** This spec moves existing codes through a different surface; adding codes is unrelated.
- **Multi-service / SDK extension.** Same pattern would help a future Python SDK, Go agent, etc. — out of scope; M73 ships CLI-only.
