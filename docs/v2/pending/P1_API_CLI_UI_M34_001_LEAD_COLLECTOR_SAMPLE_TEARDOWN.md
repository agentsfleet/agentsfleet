# M34_001: Lead-Collector Sample Teardown

**Prototype:** v2.0.0
**Milestone:** M34
**Workstream:** 001
**Date:** Apr 22, 2026
**Status:** PENDING
**Priority:** P1 — removes the legacy `lead-collector` sample referenced by `zombiectl install`, by five Zig test suites, by unit tests in `zombiectl/test/`, and by two UI test files. Blocking a clean v2.0-alpha story because the brainstormed v2 direction calls for exactly one flagship executable sample (`samples/homelab/`, owned by M33_001), not two.
**Batch:** B3 — post-alpha cleanup; depends on M33_001 (homelab sample authored) so migrated fixtures have a replacement name to target.
**Branch:** feat/m34-lead-collector-teardown (to be created when work starts)
**Depends on:** M33_001 (homelab sample authored — DONE for §1/§2/§5, parked). No dependency on M19_001 or nullclaw.

---

## Overview

**Goal (testable):** `rg -nw lead-collector` across `src/`, `zombiectl/`, `ui/`, and `samples/` returns zero load-bearing hits. `zombiectl install lead-collector` either exits with a "template not found" error or is replaced by a `homelab-zombie` installable template. `make test` + `make test-integration` + `bun test` across `ui/packages/` all stay green across the migration.

**Problem:** The brainstormed v2 direction (`docs/brainstormed/usezombie-v2-milestone-specs-prompt.md` §6 line 71) says "DELETE `samples/lead-collector/`". No v2/pending or v2/active spec currently owns that deletion. M32_001 removes only the external `docs/integrations/lead-collector.mdx` docs-repo page. M33_001 (homelab zombie) stopped citing `samples/lead-collector/` as a convention source but did not touch the directory itself (out of scope). The removal has real blast radius: the sample is baked into `zombiectl install` defaults, five Zig test-fixture suites, unit tests in `zombiectl/test/zombie.unit.test.js`, and two UI tests. Without a dedicated coordinated workstream, the deletion will either drift indefinitely or break CI when someone attempts it piecemeal.

**Solution summary:** Delete `samples/lead-collector/` and `zombiectl/templates/lead-collector/`. Remove `"lead-collector"` from `BUNDLED_TEMPLATES` in `zombiectl/src/commands/zombie.js`. Rename the `lead-collector` fixture strings in five Zig test files and `zombiectl/test/zombie.unit.test.js` to a neutral test name (recommend `test-zombie` — fixture names should not shadow real sample names, to avoid this same drift problem next time). Update the two UI test files similarly. Verify the full test suite (unit + integration + UI) green pre- and post-deletion; the diff should show zero behavioral drift, only name changes + file removals.

---

## Files Changed (blast radius)

All under `$REPO_ROOT/` (the `usezombie` checkout).

| File | Action | Why |
|------|--------|-----|
| `samples/lead-collector/SKILL.md` | DELETE | Sample dir per v2 direction. |
| `samples/lead-collector/TRIGGER.md` | DELETE | Sample dir per v2 direction. |
| `zombiectl/templates/lead-collector/SKILL.md` | DELETE | Installable template for the deleted sample. |
| `zombiectl/templates/lead-collector/TRIGGER.md` | DELETE | Installable template for the deleted sample. |
| `zombiectl/src/commands/zombie.js` | EDIT | Remove `"lead-collector"` from `BUNDLED_TEMPLATES`; keep `"slack-bug-fixer"`. |
| `zombiectl/test/zombie.unit.test.js` | EDIT | Migrate test assertions: use `"slack-bug-fixer"` (already in `BUNDLED_TEMPLATES`) OR a purpose-built fixture template. The tests verify install behavior, not the specific template content. |
| `src/zombie/yaml_frontmatter.zig` | EDIT | Rename fixture string `"lead-collector"` → `"test-zombie"` (2 occurrences at lines 228, 234). |
| `src/zombie/config_markdown_test.zig` | EDIT | Rename fixture string (3 occurrences at lines 12, 17, 42). |
| `src/zombie/config_parser_test.zig` | EDIT | Rename fixture string (5 occurrences at lines 11, 18, 45, 52). Also update the chain-source fixture `"lead-collector"` → `"test-zombie"`. |
| `src/zombie/event_loop_integration_test.zig` | EDIT | Rename fixture string (9 occurrences including 7 `base.seedZombie` calls). |
| `src/zombie/event_loop_obs_integration_test.zig` | EDIT | Rename fixture string (6 occurrences including 3 `base.seedZombie` calls). |
| `ui/packages/app/tests/app-primitives.test.ts` | EDIT | Rename `zombie_name: "lead-collector"` → `zombie_name: "test-zombie"` (2 occurrences at lines 108, 120). |
| `ui/packages/website/src/components/domain/tokenize-bash.test.ts` | EDIT | Update bash-command example strings (lines 10, 66). The test checks the bash tokenizer, not the specific zombie name; a neutral example is equivalent. |

**Note on the test fixture rename:** the new fixture string `"test-zombie"` (or any other non-sample name) must not collide with a real installable template or a real sample directory. This is precisely the drift vector that made M34 necessary.

**Zero deletions outside the list above.** No schema files, no migrations, no handler code.

---

## Applicable Rules

- **RULE FLL** — files touched stay reviewable; line counts should not grow.
- **RULE ORP** — post-teardown, `rg -nw lead-collector -g '!docs/v*/done/' -g '!docs/brainstormed/' -g '!docs/v2/active/P1_SKILL_M33_001*' -g '!docs/v2/pending/P1_DOCS_API_CLI_M32_001*' -g '!docs/v2/pending/P1_API_CLI_UI_M34_001*' -g '!docs/nostromo/' -g '!docs/changelog*'` returns 0 matches. Historical specs (v1/done, v2/done) and this spec itself are exempt; so is M33's Discovery #8 and M32's still-pending reference.
- **RULE TST-NAM** — no milestone IDs in test filenames or `test "…"` names.
- **Zig drain rule** — if any edited `*.zig` file touches `conn.query`, verify `.drain()` is present. This spec does not introduce new queries; sanity check at VERIFY.

---

## Sections (implementation slices)

### §1 — Rename Zig test fixtures

**Status:** PENDING

Migrate every `"lead-collector"` fixture string in the five Zig test files to `"test-zombie"`. The rename is purely cosmetic — the tests assert behavior of config parsing, the event loop, and observability emission; the specific zombie name is never under test. Verify `zig build test` green pre-rename and post-rename.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `src/zombie/yaml_frontmatter.zig` | `sed -i 's/"lead-collector"/"test-zombie"/g'` then `zig build test` | test passes with new fixture name | unit |
| 1.2 | PENDING | `src/zombie/config_markdown_test.zig` | same | test passes | unit |
| 1.3 | PENDING | `src/zombie/config_parser_test.zig` | same (incl. chain-source fixture) | test passes | unit |
| 1.4 | PENDING | `src/zombie/event_loop_integration_test.zig` | same (incl. `seedZombie` calls) | test passes | integration |
| 1.5 | PENDING | `src/zombie/event_loop_obs_integration_test.zig` | same | test passes | integration |

### §2 — Rename JS/TS fixtures

**Status:** PENDING

Migrate the unit test in `zombiectl/test/zombie.unit.test.js`. Two approaches — pick whichever keeps the test's intent intact:

(a) Replace `"lead-collector"` with `"slack-bug-fixer"` (already in `BUNDLED_TEMPLATES`). The tests verify install behavior; they work with any bundled template. This is the simplest approach.
(b) Create a purpose-built `test-template` entry under `zombiectl/templates/test-template/` used only by the test suite. More ceremony; avoids coupling the test to a user-facing template name.

Default to (a). Drop to (b) only if the tests assert specific content of the lead-collector template that slack-bug-fixer does not carry.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `zombiectl/test/zombie.unit.test.js` | rename fixture; `bun test` | test passes | unit |
| 2.2 | PENDING | `ui/packages/app/tests/app-primitives.test.ts` | rename `zombie_name: "lead-collector"` → `"test-zombie"`; `bun test` | test passes | unit |
| 2.3 | PENDING | `ui/packages/website/src/components/domain/tokenize-bash.test.ts` | update bash-command example string; `bun test` | test passes (tokenizer output unaffected by name change) | unit |

### §3 — Remove the sample + template directories + BUNDLED_TEMPLATES entry

**Status:** PENDING

Order is important: §1 and §2 must land first so no test references `lead-collector` anymore. Then delete the two directories and edit `BUNDLED_TEMPLATES`.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `samples/lead-collector/` | `git rm -r` | directory gone; `git status` shows 2 deletions | filesystem |
| 3.2 | PENDING | `zombiectl/templates/lead-collector/` | `git rm -r` | directory gone; `git status` shows 2 deletions | filesystem |
| 3.3 | PENDING | `zombiectl/src/commands/zombie.js` `BUNDLED_TEMPLATES` | remove `"lead-collector"` entry | list becomes `["slack-bug-fixer"]`; `zombiectl install lead-collector` returns a clear "template not found" error; `zombiectl install slack-bug-fixer` still works | manual + unit |

### §4 — Orphan sweep + CI green

**Status:** PENDING

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | whole repo minus historical/brainstormed/spec-prose | orphan grep (see Eval E5) | zero matches | grep |
| 4.2 | PENDING | Zig suite | `zig build test && make test-integration` | green | integration |
| 4.3 | PENDING | JS suites | `(cd zombiectl && bun test) && (cd ui && bun test)` | green | integration |

---

## Interfaces

**Status:** LOCKED — M34 removes a user-facing installable template (`zombiectl install lead-collector`) but does not introduce new interfaces.

### Removed interfaces

- **`zombiectl install lead-collector`** — removed. Post-teardown, the command returns a "template not found" error identifying the two remaining bundled templates (`slack-bug-fixer`) or directs the user to `zombiectl zombie install --from samples/homelab` once M19_001 ships.
- **`samples/lead-collector/`** — removed. No sample occupies this path; any downstream doc or tool that still references it is stale.

### Preserved interfaces

- `zombiectl install slack-bug-fixer` continues to work.
- `BUNDLED_TEMPLATES` stays a list of strings — shape unchanged, one entry fewer.
- Zig config parsers + event loop behavior unchanged; only test fixture names change.

---

## Failure Modes

| Failure | Trigger | System behavior | User observes |
|---------|---------|-----------------|---------------|
| Operator runs `zombiectl install lead-collector` post-teardown | pre-existing muscle memory or stale docs | CLI prints "template not found" and lists remaining templates | clear error; no crash |
| `make test` fails post-rename | missed occurrence in a test fixture | CI red | grep the failing assertion, update missed occurrence, re-run |
| UI test green locally but red in CI | `bun install` skipped a workspace | re-run with explicit workspace flag | follow `reference_bun_workspace_install` memory — root `bun install` hydrates both workspaces |
| Stale doc references `lead-collector` after merge | external docs repo not updated | no runtime failure, but operator guidance rots | M32_001 already removes `docs/integrations/lead-collector.mdx`; any remaining doc prose is cosmetic |

---

## Implementation Constraints (Enforceable)

| Constraint | How to verify |
|-----------|---------------|
| Zero load-bearing references to `lead-collector` post-teardown | Eval E5 (narrowed grep) returns 0 |
| `zombiectl install slack-bug-fixer` still works | `zombiectl install slack-bug-fixer` in a tmpdir → exit 0 |
| Zig test suite green | `zig build test` exit 0 |
| Integration tests green | `make test-integration` exit 0 |
| JS/TS test suites green | `(cd zombiectl && bun test) && (cd ui && bun test)` exit 0 |
| No new mentions in `samples/`, `src/`, `zombiectl/`, `ui/`, tracked docs | Eval E5 |

---

## Invariants (Hard Guardrails)

| # | Invariant | Enforcement |
|---|-----------|-------------|
| 1 | `samples/` contains no `lead-collector/` directory | `test ! -d samples/lead-collector` |
| 2 | `zombiectl/templates/` contains no `lead-collector/` directory | `test ! -d zombiectl/templates/lead-collector` |
| 3 | `BUNDLED_TEMPLATES` in `zombiectl/src/commands/zombie.js` does not include `"lead-collector"` | `grep -c '"lead-collector"' zombiectl/src/commands/zombie.js` returns 0 |
| 4 | No Zig test file uses `"lead-collector"` as a fixture | `grep -rn '"lead-collector"' src/ --include='*.zig'` returns 0 |
| 5 | No JS/TS test file uses `"lead-collector"` as a fixture | `grep -rn '"lead-collector"' zombiectl/test/ ui/packages/ --include='*.js' --include='*.ts' --include='*.tsx'` returns 0 |

---

## Test Specification

### Unit / integration tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| `zig build test green` | 1.1–1.5 | whole zig tree | `zig build test` | exit 0 |
| `zombiectl unit test green` | 2.1 | `zombiectl/test/zombie.unit.test.js` | `bun test` | exit 0 |
| `ui app test green` | 2.2 | `ui/packages/app/tests/app-primitives.test.ts` | `bun test` | exit 0 |
| `ui website test green` | 2.3 | `ui/packages/website/src/components/domain/tokenize-bash.test.ts` | `bun test` | exit 0 |
| `integration tests green` | 4.2 | whole repo | `make test-integration` | exit 0 |

### Negative tests

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|----------------|
| `install lead-collector post-teardown` | 3.3 | `zombiectl install lead-collector` in a tmpdir | clear "template not found" error; non-zero exit |
| `resurrected sample directory` | 1 | add back `samples/lead-collector/` in a PR after teardown | CI grep gate (Invariant 1) fails |

### Regression tests

N/A — rename+delete workstream. The tests that previously depended on `"lead-collector"` are migrated in-place; their assertions still verify the same behavior with a new fixture name.

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|------|--------|--------|
| 1 | CHORE(open) — move this spec `pending/` → `active/`, create worktree `../usezombie-m34-lead-collector-teardown` on `feat/m34-lead-collector-teardown`. | `ls docs/v2/active/` shows this spec |
| 2 | Rename fixture strings in the five Zig test files (§1). Run `zig build test` after each file. | `zig build test` exit 0 after all five |
| 3 | Rename fixture in `zombiectl/test/zombie.unit.test.js` (§2.1). `bun test`. | exit 0 |
| 4 | Rename fixture in two UI test files (§2.2, §2.3). `bun test` in each workspace. | exit 0 |
| 5 | `git rm -r samples/lead-collector zombiectl/templates/lead-collector` (§3.1, §3.2). | `git status` shows 4 deletions |
| 6 | Edit `BUNDLED_TEMPLATES` in `zombiectl/src/commands/zombie.js` (§3.3). | `grep '"lead-collector"' zombiectl/src/commands/zombie.js` returns 0 |
| 7 | Full CI: `make test && make test-integration && (cd zombiectl && bun test) && (cd ui && bun test)`. | all green |
| 8 | Orphan sweep (Eval E5). | 0 load-bearing hits |
| 9 | CHORE(close), spec → `done/`, Ripley log, changelog `<Update>` block under `["Internal"]` tag (internal cleanup — short block). | spec in `done/`; changelog has new entry |

---

## Acceptance Criteria

- [ ] `samples/lead-collector/` does not exist on disk — verify: `test ! -d samples/lead-collector`
- [ ] `zombiectl/templates/lead-collector/` does not exist on disk — verify: `test ! -d zombiectl/templates/lead-collector`
- [ ] `BUNDLED_TEMPLATES` excludes `"lead-collector"` — verify: Invariant 3
- [ ] Zero load-bearing `"lead-collector"` references — verify: Eval E5
- [ ] All CI suites green — verify: Dims 4.2, 4.3
- [ ] `zombiectl install slack-bug-fixer` still produces a working zombie — verify: manual smoke
- [ ] `zombiectl install lead-collector` returns a clean "template not found" error — verify: negative test
- [ ] Changelog has a new `<Update>` block tagged `["Internal"]` describing the cleanup — verify: diff of `docs/changelog.mdx` in `/Users/kishore/Projects/docs/`

---

## Eval Commands

```bash
# E1: sample directory gone
test ! -d samples/lead-collector && echo "ok: samples/lead-collector removed" || echo "FAIL"

# E2: template directory gone
test ! -d zombiectl/templates/lead-collector && echo "ok: template removed" || echo "FAIL"

# E3: BUNDLED_TEMPLATES excludes lead-collector
grep -c '"lead-collector"' zombiectl/src/commands/zombie.js | grep -q '^0$' \
  && echo "ok: BUNDLED_TEMPLATES clean" || echo "FAIL"

# E4: no Zig / JS / TS fixture uses the old name
grep -rn '"lead-collector"' src/ zombiectl/ ui/ \
  --include='*.zig' --include='*.js' --include='*.ts' --include='*.tsx' \
  && echo "FAIL: fixture reference" || echo "ok: no fixture refs"

# E5: orphan sweep — everything outside historical specs + brainstormed + this spec
grep -rn -w 'lead-collector' . \
  | grep -v -E '(\.git/|v1/done/|v2/done/|docs/brainstormed/|docs/nostromo/|docs/changelog|M33_001_HOMELAB_ZOMBIE|M32_001_QUICKSTART|M34_001_LEAD_COLLECTOR)' \
  && echo "FAIL: orphan reference" || echo "ok: orphans clean"

# E6: slack-bug-fixer install still works (smoke)
TMPDIR=$(mktemp -d); cd $TMPDIR && zombiectl install slack-bug-fixer >/dev/null 2>&1 \
  && echo "ok: slack-bug-fixer installs" || echo "FAIL"

# E7: lead-collector install fails cleanly
zombiectl install lead-collector 2>&1 | grep -iE "not found|unknown" >/dev/null \
  && echo "ok: clean error for removed template" || echo "FAIL"

# E8: full test suites
zig build test 2>&1 | tail -3
make test-integration 2>&1 | tail -5
(cd zombiectl && bun test 2>&1 | tail -5)
(cd ui && bun test 2>&1 | tail -5)
```

---

## Dead Code Sweep

Post-teardown, these paths should not exist:

| Content | Verify |
|---------|--------|
| `samples/lead-collector/` | E1 |
| `zombiectl/templates/lead-collector/` | E2 |
| `lead-collector` fixture strings in Zig/JS/TS tests | E4 |
| `lead-collector` in `BUNDLED_TEMPLATES` | E3 |
| Any other source reference | E5 |

---

## Out of Scope

- Removing `slack-bug-fixer` template — still in use; not marked for deletion.
- Creating new sample directories beyond what M33_001 already landed (`samples/homelab/`). `homebox-audit`, `migration-zombie`, `side-project-resurrector` are M32_001's territory.
- Changing `zombiectl install` command shape (`--from <path>` is M19_001; `--template X` is a separate concern).
- External docs-repo updates (`/Users/kishore/Projects/docs/`) — M32_001 already removes `docs/integrations/lead-collector.mdx`; any other stale external prose is cosmetic and follows in that workstream.
- Renaming existing zombies in operator databases (no such users in v2-alpha).
