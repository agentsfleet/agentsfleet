# AGENTS.md Invariance Suite

A two-layer ruleset that proves AGENTS.md still holds the line after edits:

1. **Deterministic layer** — `scripts/audit-agents-md.sh` (mechanical, fast, runs in `pre-commit`).
2. **Prompt-invariance layer** — this file. An LLM agent reads AGENTS.md and answers every question below. Every answer must be **YES**. A NO means the ruleset regressed.

You run this suite **before** any AGENTS.md change AND **after** the change. Both runs must produce the same all-YES result. A pass that flips to NO is the precise definition of a broken invariant.

---

## Step 1 — Run the deterministic script

```bash
bash scripts/audit-agents-md.sh
```

If exit ≠ 0, **STOP**. The script's `FAIL:` lines name the regression. Fix AGENTS.md first; do not proceed to Step 2 with a failing script.

---

## Step 2 — Prompt-invariance questionnaire

Hand AGENTS.md to the LLM and have it answer each question with **YES** / **NO** + the AGENTS.md line(s) that justify the answer. A `NO` (or "I can't find it") is a regression.

The questionnaire is organised by scenario. Each scenario corresponds to a moment in the lifecycle where determinism matters most — the points where a vague AGENTS.md would let an agent silently drift.

### Scenario 1 — A new spec is handed over (pre-CHORE(open))

| # | Question | Expected |
|---|---|---|
| 1.1 | Does AGENTS.md require the new spec to land in `docs/v{N}/pending/` with `Status: PENDING`? | YES |
| 1.2 | Is writing a `TODO.md` explicitly forbidden? | YES |
| 1.3 | Must `kishore-spec-new` be invoked rather than hand-rolling the spec file? | YES |
| 1.4 | Is the agent forbidden from writing code before CHORE(open) completes its 4 steps? | YES |
| 1.5 | When a spec contradicts a rule, must the spec be amended (not the rule weakened)? | YES |

### Scenario 2 — Brainstorming leads to a new spec

| # | Question | Expected |
|---|---|---|
| 2.1 | Is the Golden-path walk-through required before PLAN approval, with `[?]` blocking? | YES |
| 2.2 | Must surfaced assumptions be listed (`ASSUMPTIONS I'M MAKING…`) before non-trivial work? | YES |
| 2.3 | Must Discovery findings (e.g. legacy consults) be logged in the spec's Discovery section or filed as a new pending spec? | YES |
| 2.4 | Are external-credential needs enumerated in `M{N}_001` before any `M{N}_002+`? | YES |

### Scenario 3 — Executing a new spec; human spots and steers

| # | Question | Expected |
|---|---|---|
| 3.1 | When a mid-task conflict surfaces, must the agent STOP, name it, present tradeoff, and wait? | YES |
| 3.2 | Must the agent edit only files in the spec's approved scope (no opportunistic refactors)? | YES |
| 3.3 | If an NLR DECISION is required, does the agent have **no autonomous escape** — only the user can choose? | YES |
| 3.4 | Must Legacy-Design Consult **block on user reply** (not silently patch/keep)? | YES |
| 3.5 | If the user says "review this" / "look at this", does the agent treat it as investigation, not authorization? | YES |

### Scenario 4 — Executing on UI, Zig, TS/JS, shell, or CI files

| # | Question | Expected |
|---|---|---|
| 4.1 (UI) | For every `*.tsx`/`*.jsx` under `ui/packages/app/`, must raw HTML be substituted with a design-system primitive when one exists? | YES |
| 4.2 (Zig) | For every `*.zig` Edit/Write outside `vendor/`/`third_party/`/`.zig-cache/`, does ZIG GATE fire? | YES |
| 4.3 (Zig) | Must FILE SHAPE DECISION print before the first Write to a new `*.zig` under `src/` — and is that override **not** covered by auto-mode? | YES |
| 4.4 (Zig) | Does PUB GATE require external-consumer grep proof for every new `pub` symbol? | YES |
| 4.5 (Zig) | Is cross-compile to `x86_64-linux` AND `aarch64-linux` mandatory before commit? | YES |
| 4.6 (TS/JS) | Are milestone IDs (`M{N}_{NNN}`, `§X.Y`, `T7`, `dim 5.8.15`) banned in source files (incl. tests)? | YES |
| 4.7 (TS/JS) | Does RULE UFS require string literals used in ≥2 sites to become a named constant? | YES |
| 4.8 (Shell) | Are shell scripts subject to the File & Function Length Gate (≤350 / ≤50 / ≤70)? | YES |
| 4.9 (Shell) | Must `gitleaks` pass before any commit/push? | YES |
| 4.10 (CI) | Are CI/CD edits (`.github/workflows/**`, deploy configs) **forbidden without explicit user approval** even in auto mode? | YES |
| 4.11 (Bun) | For `*.ts`/`*.tsx`/`*.js`/`*.jsx` edits, does AGENTS.md require reading `docs/BUN_RULES.md` (TS FILE SHAPE DECISION at PLAN, const/import/Bun-primitive discipline)? | YES |

### Scenario 5 — Picking up a handover

| # | Question | Expected |
|---|---|---|
| 5.1 | Is the agent required to verify CWD is inside the active worktree before resuming (`pwd` + `git worktree list`)? | YES |
| 5.2 | Must the agent re-read RULES.md when sub-task shape changes (new layer/language/resume after break)? | YES |
| 5.3 | If the spec is in `active/`, is CHORE(close) the mandatory next action after any COMMIT? | YES |
| 5.4 | If unexpected changes appear in files the agent is editing, must the agent stop and ask (not overwrite as stale)? | YES |

### Scenario 6 — Verification lifecycle (HARNESS VERIFY → VERIFY)

| # | Question | Expected |
|---|---|---|
| 6.1 | Does HARNESS VERIFY enumerate every gate as a row in its verdict block? | YES |
| 6.2 | Does any "fail" / remaining violation in HARNESS VERIFY return the lifecycle to EXECUTE (no advance)? | YES |
| 6.3 | Is `/write-unit-test` the FIRST verify action, with skipping = CHORE(close) violation? | YES |
| 6.4 | Are `make lint` + `make test` always required (tier 1)? | YES |
| 6.5 | Is `make test-integration` required when diff touches HTTP/schema/DB/Redis or `_integration_test.zig`? | YES |
| 6.6 | Is at least one tier-3 `make test-integration` from clean state required per branch before ship-ready? | YES |
| 6.7 | Are package-scoped runners (`bun run test`, `vitest <file>`, `zig build test` w/o integration) explicitly **not** verification? | YES |
| 6.8 | Must memleak evidence (last 3 lines verbatim) appear in PR Session Notes when touching `src/http/**` / `src/cmd/serve.zig` / allocator wiring? | YES |

### Scenario 7 — Reviewing / `/review-pr` of a specification

| # | Question | Expected |
|---|---|---|
| 7.1 | Is the skill chain order `/write-unit-test` → `/review` → `/review-pr` → `kishore-babysit-prs` preserved? | YES |
| 7.2 | Is `/review` required **before** CHORE(close) commits? | YES |
| 7.3 | Is `/review-pr` required **after** CHORE(close) + `gh pr create`? | YES |
| 7.4 | Does `kishore-babysit-prs` run after every push and stop only on two consecutive empty polls? | YES |
| 7.5 | Is using `gh pr checks --watch` for greptile explicitly disallowed? | YES |
| 7.6 | If an MCP-backed skill is unavailable, must PR Session Notes record the skip + a "rerun before merge" note? | YES |
| 7.7 | Is merging/closing/ready-from-draft of another user's PR forbidden without explicit approval? | YES |

### Scenario 8 — Conducting `/write-unit-test` while a human steers

| # | Question | Expected |
|---|---|---|
| 8.1 | Does every spec Dimension require a corresponding test case (no test → not implemented)? | YES |
| 8.2 | Does every Error Table row require a negative test? | YES |
| 8.3 | Is "DONE" defined as **called in production + tested** (grep entry-point for the symbol)? | YES |
| 8.4 | Does the Changelog Claim Challenge ask "Would this be true if the test file vanished?" before any `<Update>` block? | YES |

### Scenario 9 — Hot-fix / emergency change (added)

| # | Question | Expected |
|---|---|---|
| 9.1 | Are destructive git ops (`reset --hard`, `clean -fd`, `branch -D`, etc.) forbidden without explicit approval? | YES |
| 9.2 | Is force-push to default branch a **no-override** ban? | YES |
| 9.3 | Is reverting changes the agent did not create disallowed without approval? | YES |

### Scenario 10 — Touching dotfiles or docs-repo (added)

| # | Question | Expected |
|---|---|---|
| 10.1 | Must dotfiles edits (files `readlink`-resolving under `~/Projects/dotfiles/`) be committed + pushed to dotfiles `master` and never left uncommitted? | YES |
| 10.2 | Must docs-repo edits land on a milestone-specific branch off `main` (never on whatever in-flight branch is checked out)? | YES |

### Scenario 11 — Schema / migration work (added)

| # | Question | Expected |
|---|---|---|
| 11.1 | Pre-v2.0.0, is the table-removal flow rm-file + rm-embed + rm-migration-array (no `ALTER TABLE`/`DROP TABLE`/`SELECT 1;` markers)? | YES |
| 11.2 | Are static strings in SQL schema (`DEFAULT 'value'` / `CHECK (col IN ('a','b'))`) a **no-override** ban? | YES |

### Scenario 12 — Auto-mode boundary (added)

| # | Question | Expected |
|---|---|---|
| 12.1 | Does auto-mode autonomy require BOTH auto-mode-active AND (active spec OR forward-looking start instruction) before commit/push/PR proceed without re-asking? | YES |
| 12.2 | Do action-triggered guards still block under auto mode (autonomy bypasses none)? | YES |

### Scenario 13 — Ruleset changes (Invariance Suite meta-gate)

| # | Question | Expected |
|---|---|---|
| 13.1 | When the agent itself edits `AGENTS.md`, `AGENTS_INVARIANCE.md`, or any `docs/gates/*.md` in this session, does the Invariance Suite Gate fire and require running the questionnaire before declaring done? | YES |
| 13.2 | Is the agent forbidden from self-overriding the Invariance Suite Gate? (Only the user may bypass at push time via `SKIP_INVARIANCE_PUSH=1`.) | YES |
| 13.3 | Does the sign-off line format `<short-sha>  <UTC-timestamp>  PASS` tie to the post-commit HEAD SHA? | YES |
| 13.4 | Does the pre-push hook block when sign-off SHA ≠ HEAD, result ≠ `PASS`, or mtime > 24 h? | YES |

### Scenario 14 — Communication discipline

| # | Question | Expected |
|---|---|---|
| 14.1 | Does AGENTS.md require expanding non-obvious acronyms / project codenames / vendor names on first mention in chat replies, PR descriptions, commit messages, AND inline code comments? | YES |
| 14.2 | Is there an explicit skip-list of undergrad-CS staples (API, URL, HTTP, JSON, SQL, DNS) that need NO expansion? | YES |
| 14.3 | Does the tone rule permit dry humour and swear words, while requiring that technical clarity is never traded for it? | YES |
| 14.4 | Does the verification done-message use ✅ / 🔴 / ⚠️ glyphs and the explicit format defined in `docs/gates/verification.md`? | YES |
| 14.5 | Does AGENTS.md identify "the Captain" as Kishore (the human), so addresses like "Aye Aye Captain" resolve unambiguously? | YES |

### Scenario 15 — Architecture-edit ordering

| # | Question | Expected |
|---|---|---|
| 15.1 | Must an architecture decision land its `docs/architecture/` edit either (a) in an immediate doc-only commit OR (b) in the same commit as the implementation, with (c) follow-up AFTER code explicitly forbidden? | YES |
| 15.2 | Must every M-spec branch touching flow-defining code produce a non-empty `git diff origin/main..HEAD -- docs/architecture/`, OR have PR Session Notes document why nothing architectural changed? | YES |

### Scenario 16 — Credentials & vault

| # | Question | Expected |
|---|---|---|
| 16.1 | Are plaintext secrets in entity tables (`core.zombies`, `core.workspaces`, etc.) a **no-override** forbidden? | YES |
| 16.2 | Must credentials be stored as a vault `key_name` reference and resolved at runtime via `crypto_store.load()`? | YES |
| 16.3 | Are static strings in SQL schema (`DEFAULT 'value'`, `CHECK (col IN ('a','b'))`) a **no-override** forbidden — enforced via app-code named constants instead? | YES |
| 16.4 | Must the agent NEVER print/log/paste a credential value, and always use `op read 'op://...'` at runtime when verification steps reference credentials? | YES |

### Scenario 17 — DB discipline (Zig)

| # | Question | Expected |
|---|---|---|
| 17.1 | Does `conn.query()` require `.drain()` in the same function before `deinit()`, with `make check-pg-drain` verifying? | YES |
| 17.2 | Is `conn.exec()` the prescribed alternative when no rows are needed? | YES |

### Scenario 18 — Commit/push hygiene & worktree isolation

| # | Question | Expected |
|---|---|---|
| 18.1 | Must `gitleaks` pass before any `git commit` / `git push`? | YES |
| 18.2 | Is the rule "one worktree per active stream — no edits outside, no reads from siblings, merge only after VERIFY" preserved? | YES |
| 18.3 | Are cross-worktree edits explicitly forbidden without explicit user approval? | YES |

### Scenario 19 — HARNESS VERIFY combined audit

| # | Question | Expected |
|---|---|---|
| 19.1 | Does HARNESS VERIFY include a combined awk pass over `git diff -U0 HEAD` that emits `MS-ID:`, `PUB:`, and `UI:` hits — replacing four separate self-audits? | YES |
| 19.2 | Is non-empty awk output a violation that MUST be addressed before HARNESS VERIFY passes? | YES |

### Scenario 20 — Rule extension protocol

| # | Question | Expected |
|---|---|---|
| 20.1 | Does AGENTS.md document a "Rule extension protocol" requiring 4 same-diff steps when introducing a new rules file (`docs/<TOPIC>_RULES.md`) or gate body (`docs/gates/<slug>.md`)? | YES |
| 20.2 | Does the protocol require: (a) doc-reads table row, (b) AGENTS_INVARIANCE.md question, (c) `DOTFILES_RESIDENT` audit entry, (d) `make audit` passing before commit? | YES |
| 20.3 | Does the Invariance Suite Gate fire on any commit landing the protocol's edits, with sign-off mandatory before push? | YES |

---

## Step 3 — Write the sign-off file

After all questions answer YES and the report below has been produced, write a sign-off line to `.agents-invariance-signoff` (gitignored). The pre-push hook reads this to allow the push.

```bash
printf '%s  %s  PASS\n' "$(git rev-parse --short HEAD)" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  > .agents-invariance-signoff
```

Sign-off format: `<short-sha>  <UTC-timestamp>  <PASS|FAIL>`. The hook accepts `PASS` only when the SHA matches HEAD and the file is < 24 h old.

## Step 4 — Tabulated audit report

After all questions answer YES, the agent emits a final report in this exact shape:

```
AGENTS.md INVARIANCE REPORT — <commit sha> — <UTC timestamp>

| Layer            | Result               |
|------------------|----------------------|
| Script (audit)   | PASS / FAIL          |
| Prompt (Q&A)     | <N>/<total> YES      |
| Size             | <bytes> / <limit>    |

Gates present:        <count> / 12       (list any missing)
Rules referenced:     <count> / 13       (list any missing)
Lifecycle stages:     <count> / 8        (CHORE(open), PLAN, EXECUTE, HARNESS VERIFY, VERIFY, DOCUMENT, COMMIT, CHORE(close))
Always-forbidden:     <count> / 6        (list any missing)
Skill chain order:    <ordered | broken>
Cross-refs:           <ok | broken: list>

Scenario verdicts:
| #  | Scenario                                | Result          |
|----|-----------------------------------------|-----------------|
| 1  | New spec handover                       | <N/M YES>       |
| 2  | Brainstorming → new spec                | <N/M YES>       |
| 3  | Executing, human steers                 | <N/M YES>       |
| 4  | Editing UI/Zig/TS/JS/shell/CI files     | <N/M YES>       |
| 5  | Handover pickup                         | <N/M YES>       |
| 6  | Verification lifecycle                  | <N/M YES>       |
| 7  | Review / /review-pr                     | <N/M YES>       |
| 8  | /write-unit-test with human steering    | <N/M YES>       |
| 9  | Hot-fix / emergency                     | <N/M YES>       |
| 10 | Dotfiles / docs-repo                    | <N/M YES>       |
| 11 | Schema / migration                      | <N/M YES>       |
| 12 | Auto-mode boundary                      | <N/M YES>       |

OVERALL: PASS | FAIL — <reason if fail>
```

---

## Wiring it into `pre-commit`

The dotfiles `.githooks/pre-commit` calls `scripts/audit-agents-md.sh` directly when AGENTS.md is in the staged set. This is fast, deterministic, and needs no LLM.

The Step-2 prompt-invariance run is **agent-invoked**, not hooked, because:

- It needs an LLM, which means latency, cost, and credentials in the hook environment.
- It only adds value when AGENTS.md itself changed (which is rare).
- Step 1 already catches the regressions a script can catch; Step 2 catches the regressions that need reading comprehension.

Recommended workflow when you edit AGENTS.md:

1. Run Step 1 (`bash scripts/audit-agents-md.sh`).
2. Open this file in a Claude Code / Oracle / Codex session and instruct: *"Read AGENTS.md and answer every question in AGENTS_INVARIANCE.md."*
3. The agent emits the Step-3 report. All-YES → commit. Any NO → fix AGENTS.md first.

If you want Step 2 enforced in `pre-push` rather than ad-hoc, see `.githooks/pre-push.example`.

---

## Adding a new scenario or question

When the operating model grows (new gate, new rule, new lifecycle wrinkle):

1. Add to `scripts/audit-agents-md.sh` if the invariant is mechanically checkable.
2. Add to this file as a new question if the invariant needs reading comprehension.
3. Update `REQUIRED_GATES` / `HARNESS_KEYS` / `FORBIDDEN_KEYS` arrays in the script as appropriate.
4. Run both layers and check in the new baseline.

The cost of this suite is bounded by these arrays. If they grow without bound, the suite is leaking complexity — split AGENTS.md before adding more invariants.
