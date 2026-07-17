# Oracle rules invariance suite

A three-part proof that generated `AGENTS.md` still holds the line after edits:

1. **Deterministic layer** — `audits/agents-md.sh` (mechanical, fast, runs in `pre-commit`).
2. **Prompt-invariance layer** — this file. A Large Language Model (LLM) agent reads `AGENTS.md` and answers every question below. Every answer must be **YES**. A NO means the ruleset regressed.
3. **Evidence layer** — `orly verify --all --write-evidence` records the source commit, registry digest, profile checks, and prompt result.

Run this suite before and after an operating-model change. Both runs must produce the same all-YES result. A pass that flips to NO is a broken invariant.

---

## Step 1 — Run the deterministic script

```bash
bash audits/agents-md.sh
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
| 4.1a (UI) | For every `*.tsx`/`*.jsx` under `ui/packages/{app,website}/`, does DESIGN TOKEN GATE block arbitrary `*-[...]` Tailwind classes (`text-[Npx]`, `leading-[...]`, `tracking-[...]`, `max-w-[Npx|Nch]`, `text-[clamp(...)]`, raw palette colours) when an equivalent token utility exists in `ui/packages/design-system/src/theme.css`? | YES |
| 4.1b (UI) | Is the DESIGN TOKEN GATE override `// DESIGN TOKEN: SKIPPED per user override (reason: ...)` user-only — i.e. auto-mode does NOT cover it, and reasons must cite a concrete external constraint (not "looks the same" / "shorter to write")? | YES |
| 4.1c (UI) | Does the project-side `audits/design-tokens.sh` audit run as part of `make lint` (`_website_lint` + `_app_lint`) and block on any arbitrary that has a token equivalent? (Default scope: full ui/packages working tree via `git ls-files` after M70 retired `--diff`.) | YES |
| 4.2 (Zig) | For every `*.zig` Edit/Write outside `vendor/`/`third_party/`/`.zig-cache/`, does ZIG GATE fire? | YES |
| 4.3 (Zig) | Must FILE SHAPE DECISION print before the first Write to a new `*.zig` under `src/` — and is that override **not** covered by auto-mode? | YES |
| 4.4 (Zig) | Does PUB GATE delegate mechanical consumer-grep to `zlint`'s `unused-decls: error` rule (run by `make lint`), leaving the gate body to enforce shape verdict + no-inheritance + per-edit proof? | YES |
| 4.4a (Zig) | Does AGENTS.md prohibit inheriting a sibling's `pub fn` decision — i.e. does every new pub surface require its own shape verdict, and does cloning a sibling's "Public for the integration test in …" comment NOT discharge the gate? | YES |
| 4.4b (Zig) | Does AGENTS.md require a per-edit proof-line (full block OR one-line `PUB GATE: skipped — <reason>`) before every `*.zig` Edit/Write, so silent gate-clean edits are still violations (the proof of consideration is part of the discipline)? | YES |
| 4.4c (Zig) | Does the PUB GATE trigger set explicitly include "any new `^pub` line in new-bytes" (the pre-edit grep at body §1), so the threshold list (first pub type / first method on pub-free-fn-dominant / last pub free fn removed) is a floor, not a ceiling? | YES |
| 4.5 (Zig) | Is cross-compile to `x86_64-linux` AND `aarch64-linux` mandatory before commit? | YES |
| 4.6 (TS/JS) | Are milestone IDs (`M{N}_{NNN}`, `§X.Y`, `T7`, `dim 5.8.15`) banned in source files (incl. tests)? | YES |
| 4.7 (TS/JS) | Does RULE UFS require string literals used in ≥2 sites to become a named constant? | YES |
| 4.7a (TS/JS/Zig) | Does RULE UFS extend to numeric literals carrying semantic meaning (conversion factors, thresholds, sub-cent rates) — required to become a named constant even at the first use site, with a pin-test carve-out? | YES |
| 4.7b (cross-runtime) | Must cross-runtime constants share an identical SCREAMING_SNAKE name across Zig + TS + JS (e.g. `NANOS_PER_USD` everywhere, never `NANOS_PER_DOLLAR` in one and `NANOS_PER_USD` in another)? | YES |
| 4.7c (test fixtures) | Are test fixtures, mock returns, and assertion arguments **not** RULE UFS exceptions — i.e. wire-format strings and semantic numerics in test code must use the named constant just like production code does? | YES |
| 4.8 (Shell) | Are shell scripts subject to the File & Function Length Gate (≤350 / ≤50 / ≤70)? | YES |
| 4.9 (Shell) | Must `gitleaks` pass before any commit/push? | YES |
| 4.10 (CI) | Are CI/CD edits (`.github/workflows/**`, deploy configs) **forbidden without explicit user approval** even in auto mode? | YES |
| 4.11 (Bun) | For `*.ts`/`*.tsx`/`*.js`/`*.jsx` edits, does AGENTS.md route to `dispatch/write_ts_adhere_bun.md` (TS FILE SHAPE DECISION at PLAN, const/import/Bun-primitive discipline)? | YES |
| 4.12 (Logging) | For every Edit/Write that adds/removes/changes a log emit (Zig `std.log.*`/`std.debug.print`/`obs.scoped`, TS/JS `console.*`/custom logger, shell `echo` to `&2`), does LOGGING GATE require reading `docs/LOGGING_STANDARD.md` and printing the per-edit gate block citing §3 (wire format) / §4 (severity) / §5 (error codes) / §6 (PII) / §10A (tightenings)? | YES |
| 4.13 (Logging) | Is `std.debug.print` in non-test source AND `console.log` in `agentsfleet/src/**` an automatic blocking violation, with no "temporary" carve-out? | YES |
| 4.14 (Logging) | Must a new `error_code=UZ-XXX-NNN` reference land in the same commit as its registry entry in `src/errors/error_registry.zig` (used-but-undeclared = blocking)? | YES |
| 4.15 (Lifecycle) | For every Edit/Write that adds/reshapes a lifecycle method in `*.zig` (`pub fn init|deinit|close|release|destroy|shutdown|dispose|free`) or an `errdefer`/`defer` adjacent to allocation, does LIFECYCLE GATE require reading `docs/LIFECYCLE_PATTERNS.md` and printing the per-edit gate block? | YES |
| 4.16 (Lifecycle) | Is `defer X.free(Y)` + `errdefer X.free(Y)` on the same allocation in the same scope a blocking violation? | YES |
| 4.17 (Lifecycle) | Must the LAST `errdefer` in init lexically precede the LAST allocation it protects (no batched-at-bottom errdefer)? | YES |
| 4.18 (Spec template) | For every Edit/Write to a spec under `docs/v*/{pending,active,done}/` or to `docs/TEMPLATE.md`, does SPEC TEMPLATE GATE forbid time/effort estimates, complexity ratings, percentage-complete fields, owners, and dates (per `TEMPLATE.md` "Prohibited" section)? | YES |
| 4.19 (Spec template) | Does `spec-template.sh` run as part of `make lint` and block on prohibited-section regex matches? | YES |
| 4.20 (Doc read) | For every Edit/Write whose file pattern matches a row in the EXECUTE doc-reads table, does DOC READ GATE require a `📖 DOC READ: <path>` proof-line — either citing §N applied OR the cited-skip variant — before the edit? | YES |
| 4.21 (Doc read) | Is the `📖 DOC READ:` proof-line required **per-edit** (not once per session, not once per file across multiple edits)? | YES |
| 4.22 (Doc read) | Are auto-mode and "I read this earlier in the session" both invalid grounds to skip the proof-line? | YES |
| 4.23 (Documentation) | Before published Markdown JSX (MDX), reusable snippets, customer readme, or public OpenAPI prose, must the agent read `dispatch/write_documentation.md` and `docs/DOCUMENTATION_RULES.md` before any narrower guide? | YES |
| 4.24 (Documentation) | Are page, snippet, generated API, and changelog scopes distinct, with repository-owned pre-commit checks enforcing the mechanical rules? | YES |
| 4.25 (Rust) | Do `*.rs` edits route to `dispatch/write_rust.md`, with ownership, error variants, feature combinations, and deterministic contention tests preserved? | YES |
| 4.26 (Python) | Do `*.py` edits route to `dispatch/write_python.md`, with parse-boundary validation and context-managed resources? | YES |
| 4.27 (Shell) | Do `*.sh` edits route to `dispatch/write_shell.md`, with quoted expansions, array arguments, cleanup, and compatibility requirements? | YES |
| 4.28 (Markdown JSX) | Do `*.mdx` edits route to `dispatch/write_mdx.md`, with structure, link, accessibility, and Mintlify-isolation requirements? | YES |

### Scenario 5 — Picking up a handover

| # | Question | Expected |
|---|---|---|
| 5.1 | Is the agent required to verify CWD is inside the active worktree before resuming (`pwd` + `git worktree list`)? | YES |
| 5.2 | Must the agent re-read RULES.md when sub-task shape changes (new layer/language/resume after break)? | YES |
| 5.3 | If the spec is in `active/`, is CHORE(close) the mandatory next action after any COMMIT? | YES |
| 5.4 | If unexpected changes appear in files the agent is editing, must the agent stop and ask (not overwrite as stale)? | YES |

### Scenario 6 — Conformance and verification lifecycle

| # | Question | Expected |
|---|---|---|
| 6.1 | Does CONFORM enumerate every gate as a row in its verdict block and invoke the active profile's `conform` commands? | YES |
| 6.2 | Does any remaining violation in CONFORM return the lifecycle to EXECUTE without advancing? | YES |
| 6.3 | Is `/write-unit-test` the FIRST verify action, with skipping = CHORE(close) violation? | YES |
| 6.4 | Are `make lint` + `make test` always required (tier 1)? | YES |
| 6.5 | Is `make test-integration` required when diff touches HTTP/schema/DB/Redis or `_integration_test.zig`? | YES |
| 6.6 | Is at least one tier-3 `make test-integration` from clean state required per branch before ship-ready? | YES |
| 6.7 | Are package-scoped runners (`bun run test`, `vitest <file>`, `zig build test` w/o integration) explicitly **not** verification? | YES |
| 6.8 | Must memleak evidence (last 3 lines verbatim) appear in PR Session Notes when touching `src/http/**` / `src/cmd/serve.zig` / allocator wiring? | YES |
| 6.9 | Does CHORE(open) record a `Test Baseline:` line (unit + integration counts from `make _lint_zig_test_depth`) in the spec header? | YES |
| 6.10 | Does VERIFY end with a Test Delta row (growth vs the CHORE(open) baseline) plus a lacking-areas verdict, with zero/negative unit delta on a code-adding diff requiring justification or a return to EXECUTE? | YES |

### Scenario 7 — Review discipline before merge

| # | Question | Expected |
|---|---|---|
| 7.1 | Is the skill chain order `/write-unit-test` → `/review` → `kishore-babysit-prs` preserved? | YES |
| 7.2 | Is REVIEW an explicit stage after VERIFY and before DOCUMENT, with `/review` required there when a spec is active? | YES |
| 7.4 | Does `kishore-babysit-prs` run after every push and stop only on two consecutive empty polls? | YES |
| 7.5 | Is using `gh pr checks --watch` for greptile explicitly disallowed? | YES |
| 7.6 | If an MCP-backed skill is unavailable, must PR Session Notes record the skip + a "rerun before merge" note? | YES |
| 7.7 | Is merging/closing/ready-from-draft of another user's PR forbidden without explicit approval? | YES |
| 7.8 | Does AGENTS.md require an Indy-acked verbatim quote (in PR Session Notes or the spec's Discovery section, format `> Indy (YYYY-MM-DD HH:MM): "<quote>"`) for any claim that a spec Section/Dimension was "deferred to follow-up" — and does an agent-unilateral deferral count as incomplete scope (not deferral), blocking CHORE(close) until either the item lands or the quote is captured? | YES |
| 7.9 | Does AGENTS.md treat `HANDOFF.md` (or `HANDOFF_*.md` at any depth) as a faithful state report — i.e. must a pickup agent reading a HANDOFF that claims items were deferred without ack-quotes treat those items as in-scope and surface the contradiction to Kishore before continuing? | YES |
| 7.10 (Architecture) | Does the `name_architecture` dispatch façade (`dispatch/name_architecture.md`) explicitly name "chat brainstorming counts" — i.e. multi-turn chat designing a new pattern fires the dispatch even before code touches, and capturing the brainstorm as a "punchlist task to land after the code ships" is forbidden (a pickup agent reading such a task must rewrite it to same-commit or doc-only-now)? | YES |

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
| 10.3 | When a harness/gate/hook fires (`msid-ui.sh`, `lint-zig.py`, gitleaks, ZIG GATE, FLL GATE, pre-commit/pre-push, etc.), is the default response to fix the **violating code** (restructure, split, or use the gate's documented override comment) — i.e. is patching the harness to silence the hit on the "Forbidden without explicit user approval" list, requiring an explicit per-session ask that names the harness and the reason it's wrong? | YES |
| 10.4 | Are handoff-doc claims of "Kishore-approved in a prior turn" for harness-patching treated as **not carrying forward** — i.e. must they be re-confirmed live in the current session? | YES |

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
| 13.1 | When the agent edits `orly/**`, generated `AGENTS.md`, `audits/agents-md.md`, governance hooks, or any dispatch entry, does `edit_rules` require the audit and questionnaire before declaring done? | YES |
| 13.2 | Is the agent forbidden from self-overriding the Invariance Suite Gate? (Only the user may bypass at push time via `SKIP_INVARIANCE_PUSH=1`.) | YES |
| 13.3 | Does generated evidence bind the source commit, registry digest, profile results, and prompt-comprehension result? | YES |
| 13.4 | Does the pre-push hook run live prompt evaluation for semantic governance changes and regenerate evidence against the pushed commit? | YES |

### Scenario 14 — Communication discipline

| # | Question | Expected |
|---|---|---|
| 14.1 | Does AGENTS.md require expanding non-obvious acronyms / project codenames / vendor names on first mention in chat replies, PR descriptions, commit messages, AND inline code comments? | YES |
| 14.2 | Is there an explicit skip-list of undergrad-CS staples (API, URL, HTTP, JSON, SQL, DNS) that need NO expansion? | YES |
| 14.3 | Does AGENTS.md require a pre-send self-check scanning outgoing text for unexpanded acronyms (regex `\b[A-Z][A-Z0-9]{1,5}\b`), treating skip as on par with skipping a gate (`ACRONYM CHECK: SKIPPED per user override (reason: ...)`)? | YES |
| 14.4 | Does the tone rule permit dry humour and swear words, while requiring that technical clarity is never traded for it? | YES |
| 14.5 | Does the verification done-message use ✅ / 🔴 / ⚠️ glyphs and the explicit format defined in `dispatch/verify.md`? | YES |
| 14.6 | Does AGENTS.md identify the human as Kishore (casual handle Indy) and the agent as Oracle (casual handle Orly), with address tags 🤠 Indy / 🦉 Orly so addressing resolves unambiguously? | YES |

### Scenario 15 — Architecture-edit ordering

| # | Question | Expected |
|---|---|---|
| 15.1 | Must an architecture decision land its `docs/architecture/` edit either (a) in an immediate doc-only commit OR (b) in the same commit as the implementation, with (c) follow-up AFTER code explicitly forbidden? | YES |
| 15.2 | Must every M-spec branch touching flow-defining code produce a non-empty `git diff origin/main..HEAD -- docs/architecture/`, OR have PR Session Notes document why nothing architectural changed? | YES |

### Scenario 16 — Credentials & vault

| # | Question | Expected |
|---|---|---|
| 16.1 | Are plaintext secrets in entity tables (`core.agents`, `core.workspaces`, etc.) a **no-override** forbidden? | YES |
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
| 18.2 | Is the rule "one worktree per active stream — no edits outside, no reads from siblings, merge only after REVIEW and the final commit" preserved? | YES |
| 18.3 | Are cross-worktree edits explicitly forbidden without explicit user approval? | YES |

### Scenario 19 — CONFORM combined audit

| # | Question | Expected |
|---|---|---|
| 19.1 | Does CONFORM include a combined awk pass over `git diff -U0 HEAD` that emits `MS-ID:`, `PUB:`, and `UI:` hits? | YES |
| 19.2 | Is non-empty awk output a violation that must be addressed before CONFORM passes? | YES |

### Scenario 20 — Rule extension protocol

| # | Question | Expected |
|---|---|---|
| 20.1 | Does AGENTS.md document a "Rule extension protocol" requiring 4 same-diff steps when introducing a new rules file (`docs/<TOPIC>_RULES.md`) or dispatch entry (`dispatch/<entry>.md`)? | YES |
| 20.2 | Does the protocol require: (a) doc-reads table row, (b) audits/agents-md.md question, (c) `DOTFILES_RESIDENT` audit entry, (d) `make audit` passing before commit? | YES |
| 20.3 | Does the Invariance Suite Gate fire on any commit landing the protocol's edits, with generated evidence mandatory before push? | YES |

### Scenario 21 — Gate-flag triage discipline

| # | Question | Expected |
|---|---|---|
| 21.1 | Does AGENTS.md define a "Gate-flag triage" rule that splits a fired gate by kind — a mechanical/deterministic fix is auto-applied + informed, a judgment-level flag makes the default a Kishore ask — never silencing the gate or patching the harness in either case? | YES |
| 21.2 | Does the rule require the ask to include: (a) symbol/file/line flagged, (b) fix scope (files, lines, follow-on), (c) what we gain, (d) what happens if not fixed? | YES |
| 21.3 | Does the rule explicitly forbid the agent from unilaterally classifying a flag as a false-positive (declaring the gate wrong), as distinct from auto-fixing a mechanical violation? | YES |

### Scenario 22 — Pre-commit audit scope (M70)

| # | Question | Expected |
|---|---|---|
| 22.1 | When `make harness-verify` (the pre-commit ceremony) invokes `ufs.sh`, `design-tokens.sh`, `deinit-pairs.sh`, `error-codes.sh`, `logging.sh`, or `spec-template.sh`, do those scripts default to scanning the full working tree via `git ls-files` — so staged-but-not-yet-committed content is in scope? | YES |
| 22.2 | Is the `--diff` (BASE...HEAD) mode of `ufs.sh` and `design-tokens.sh` retired — explicitly rejected with exit 2 and a pointer to the gate body? | YES |
| 22.3 | Does `msid-ui.sh` (renamed from `combined.sh` after the PUB clause moved to zlint + agent chat-output discipline) remain the lone diff-shaped audit (still default `--staged`) — because its sub-checks (MS-ID / UI substitution) assert on *added* lines, not file state, and `git diff --cached` reads the index? | YES |
| 22.4 | Does every dispatch façade that absorbed a converted full-codebase leaf audit (`dispatch/write_any.md` ← logging/error-registry/UFS, `dispatch/write_ts_adhere_bun.md` ← design-token, `dispatch/write_zig.md` ← lifecycle/deinit, `dispatch/write_spec.md` ← spec-template) carry a "Scope (M70)" section documenting full-codebase semantics + the M68 `02c1f3cf` forcing function? | YES |

### Scenario 23 — Agent comprehension robustness

This scenario exists because the most likely failure of the operating model is
not a missing rule — it's the *agent misreading a rule that is present*.
AGENTS.md is ~28 KB of table-dense, exception-laden prose; the conditions
below are where an LLM reading it tends to drift, conflate, or confabulate.
The questions force *proof of reading* over *recall*.

| # | Question | Expected |
|---|---|---|
| 23.1 | When answering a rule-specific question, must the agent quote the dispatch **façade** (`dispatch/<entry>.md`), not the one-line dispatch-index summary — because the index is explicitly "a floor, not a ceiling" and paraphrasing the façade is a hallucination risk? | YES |
| 23.2 | When a recalled memory, `CLAUDE.md` snippet, or prior-session note conflicts with the current `AGENTS.md`/gate body, must the agent defer to the file-on-disk and surface the conflict (recall is stale-by-default)? | YES |
| 23.3 | Must override strings be reproduced **verbatim** (`<GATE>: SKIPPED per user override (reason: ...)`) and never paraphrased, since the harness matches the literal string? | YES |
| 23.4 | When two rules fire on the same edit (e.g. PUB + LIFECYCLE on `pub fn init`, or a spec that contradicts a rule), must the agent apply **both**/escalate rather than silently picking one? | YES |
| 23.5 | For an auto-mode / override question, must the agent trace the full conditional chain (auto-mode AND (active-spec OR start-instruction); action-triggered guards still block) rather than collapsing it to "auto mode = yes"? | YES |
| 23.6 | Is the negative-test harness (`evals/test-agents-md.sh`) required to pass — proving each deterministic check still *bites* — whenever `audits/agents-md.sh` itself changes? | YES |
| 23.7 | Does the live, cross-agent comprehension runner (`evals/llms/run.sh`, `make llmevals`) feed the frozen golden-set to every installed agent and grade each `VERDICT:` by exact match? | YES |
| 23.8 | When the live runner is unavailable or the golden-set changes, is `make llmevals CHECK=1` the minimum dry validation? | YES |
| 23.9 | Does pre-push cap token spend with `make llmevals SMOKE=1` while keeping the full golden-set available through an explicit `make llmevals` run? | YES |

### Scenario 24 — Memory routing (auto-memory retired)

The file-based auto-memory system is disabled (`autoMemoryEnabled: false`); durable
knowledge routes to dispatch / repo docs / HANDOFF instead of a per-session memory
store. These questions force the agent to *route a fact* rather than reach for a
memory file — the exact drift the `## Memory Discipline` section exists to prevent.

| # | Question | Expected |
|---|---|---|
| 24.1 | Is writing to `**/memory/*.md` or any `MEMORY.md` forbidden, because the harness neither records nor recalls them under `autoMemoryEnabled: false`? | YES |
| 24.2 | Does a rule that fires on a file-type / lifecycle trigger belong in `dispatch/<entry>.md` behind its gate, not in a memory note? | YES |
| 24.3 | Does in-flight state (branch / PR / next steps) belong in a `HANDOFF_*.md` + PR Session Notes + the spec, surfaced by the `pickup` / `handoff` skills? | YES |
| 24.4 | Is a durable architecture fact homed in the product repo's `docs/architecture/*.md`, not memory? | YES |
| 24.5 | If a fact has no firing gate and no doc home, is the correct move to add the rule (Rule extension protocol) or drop it — never to create a memory file? | YES |

### Scenario 25 — Allocator & concurrency discipline (ghostty-derived rules A1–A6 / C1–C5)

The M126 adversarial review mined ghostty's Allocator and concurrency practice into eleven
citable rules in `dispatch/write_zig.md`. These questions force *reading the façade rule
bodies* (per 23.1 — quote the façade, not the index), because their failure mode is an
editing agent that omits the discipline the way the liveness sweeper diverged from its
siblings.

| # | Question | Expected |
|---|---|---|
| 25.1 (A1) | Does `dispatch/write_zig.md` rule A1 require the backing allocator to be chosen once in `main` and threaded as a parameter, with global state never holding the allocator for a Zig path? | YES |
| 25.2 (A2) | Does rule A2 require an `errdefer` immediately after each fallible acquisition in a multi-step init (never batched at the bottom), with `errdefer comptime unreachable` after the last fallible op? | YES |
| 25.3 (A3) | Does rule A3 require leaf structures to take `alloc` per call and store nothing, reserving a stored `alloc` field for lifecycle roots only? | YES |
| 25.4 (A4) | Does rule A4 make an arena the ownership unit (`_arena` per config/request object, scratch arena per transient op, arena-in-message across a thread boundary) and forbid using an arena to mask a missing free in a reusable helper? | YES |
| 25.5 (A5) | Does rule A5 require ownership stated in the fixed phrases "caller must free" / "takes ownership" on every allocating public fn plus `self.* = undefined` poisoning in every deinit — with the phrase/poison checks blocking inside the discipline roster and advisory outside? | YES |
| 25.6 (A6) | Does rule A6 require every multi-step init in the roster to carry comptime-erased tripwire fail points and a loop-all-failpoints test under `std.testing.allocator` asserting both that the errdefer chain freed everything and that state rolled back? | YES |
| 25.7 (C1) | Does rule C1 require cross-thread channels to be Single-Producer Single-Consumer (SPSC), with the payload carrying its own allocator and the receiver freeing in a `defer` at the top of the handler? | YES |
| 25.8 (C2) | Does rule C2 define shutdown as stop-signal → join → deinit, explicitly forbidding freeing shared state on a drain timeout while a straggler thread can still touch it? | YES |
| 25.9 (C3) | Does rule C3 forbid a blocking push/socket-write while holding a lock the consumer needs (try-instant, else notify + unlock + block + relock), with lock state an explicit parameter? | YES |
| 25.10 (C4) | Does rule C4 require exactly one documented mutex per shared aggregate whose doc comment names precisely what it protects, with `lock(); defer unlock();` adjacent? | YES |
| 25.11 (C5) | Does rule C5 make thread-confined state the default, marked by a "only touched by thread X" comment and a `*Locked` suffix on lock-required entry points? | YES |
| 25.12 (GRD) | Does every A1–A6 / C1–C5 rule in `dispatch/write_zig.md` carry an exemplar cite (ghostty or in-repo) per RULE GRD, so each rule is traceable to the source it was mined from? | YES |

### Scenario 26 — Rules propagation

| # | Question | Expected |
|---|---|---|
| 26.1 | Is `orly/registry.json` the canonical profile and pack registry, with `orly/core/operating-model.md` as the global operating-model source? | YES |
| 26.2 | Do agent-home links point to `orly/generated/global/AGENTS.md`, making a new global render immediately visible to installed agents? | YES |
| 26.3 | Are project rules ordinary tracked snapshots with `.oracle/ruleset.lock`, rather than symbolic links into dotfiles? | YES |
| 26.4 | Does repository synchronization require a clean tree and `AGENTS.project.md`, and refuse sibling-worktree mutation? | YES |
| 26.5 | Does the `agentsfleet` profile map CONFORM to `make harness-verify` while VERIFY remains behavior proof? | YES |
| 26.6 | Does `README.md` document initialization, explicit synchronization, status, and doctor commands for new repositories? | YES |

## Comprehension layer

The deterministic audit proves the rules are *present*; it cannot prove an
agent *reading* them complies — the hallucination / won't-follow class. The
comprehension layer closes that gap:

- **Golden-set** — `evals/llms/fixtures.jsonl`: frozen
  question → expected `YES`/`NO` verdict + the justifying rule, each targeting
  a known drift mode (index-vs-body paraphrase, override-string drift,
  conditional collapse, co-firing rules, negation blindness, stale recall,
  investigate-vs-authorize, no-override bans). YES/NO is balanced so a
  constant-answer strategy fails the threshold.
- **Runner** — `evals/llms/run.sh` embeds AGENTS.md +
  the dispatch façades (`dispatch/*.md`) in every prompt (no
  tool use, no file-read variance), asks
  each installed agent, and grades the single `VERDICT:` line by exact match.
  Resumable — each agent's verdict is journalled, so a re-run after an
  interruption replays finished agents instead of re-spending tokens.
- **Cross-agent** — claude, codex, amp, opencode all run the same set;
  divergence between models flags an *ambiguous rule* (a doc bug) as much as a
  non-compliant model. Absent / credit-blocked agents are logged + excluded
  from the gate, never silently dropped.
- **Evidence** — the pre-push hook passes the live result to
  `orly verify --all --write-evidence`, which binds it to the current
  source commit and registry digest.

---

## Step 3 — Generate evidence

After all questions answer YES and the report below is complete, generate the
machine-readable evidence file:

```bash
orly verify --all --write-evidence --llm-result pass
```

The evidence is local and ignored by Git. The pre-push hook regenerates it
against the exact commit being pushed.

## Step 4 — Tabulated audit report

After all questions answer YES, the agent emits a final report in this exact shape:

```
AGENTS.md INVARIANCE REPORT — <commit sha> — <UTC timestamp>

| Layer            | Result               |
|------------------|----------------------|
| Script (audit)   | PASS / FAIL          |
| Prompt (Q&A)     | <N>/<total> YES      |
| Size             | <bytes> / <limit>    |

Dispatch entries:     <count> / 10       (list any missing)
Rules referenced:     <count> / 13       (list any missing)
Lifecycle stages:     <count> / 9        (CHORE(open), PLAN, EXECUTE, CONFORM, VERIFY, REVIEW, DOCUMENT, COMMIT, CHORE(close))
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
| 6  | Conformance and verification            | <N/M YES>       |
| 7  | Review discipline before merge          | <N/M YES>       |
| 8  | /write-unit-test with human steering    | <N/M YES>       |
| 9  | Hot-fix / emergency                     | <N/M YES>       |
| 10 | Dotfiles / docs-repo                    | <N/M YES>       |
| 11 | Schema / migration                      | <N/M YES>       |
| 12 | Auto-mode boundary                      | <N/M YES>       |
| 13 | Ruleset changes                         | <N/M YES>       |
| 14 | Communication discipline                | <N/M YES>       |
| 15 | Architecture-edit ordering              | <N/M YES>       |
| 16 | Credentials and vault                   | <N/M YES>       |
| 17 | Database discipline                     | <N/M YES>       |
| 18 | Commit and worktree isolation           | <N/M YES>       |
| 19 | CONFORM combined audit                  | <N/M YES>       |
| 20 | Rule extension protocol                 | <N/M YES>       |
| 21 | Gate-flag triage                        | <N/M YES>       |
| 22 | Pre-commit audit scope (M70)            | <N/M YES>       |
| 23 | Agent comprehension                     | <N/M YES>       |
| 24 | Memory routing                          | <N/M YES>       |
| 25 | Allocator and concurrency discipline    | <N/M YES>       |
| 26 | Rules propagation                       | <N/M YES>       |

OVERALL: PASS | FAIL — <reason if fail>
```

---

## Wiring it into `pre-commit`

The dotfiles `.githooks/pre-commit` runs `make audit` when generated rules,
canonical sources, profiles, dispatch pages, audits, or governance hooks are
staged. It is fast and deterministic.

The Step-2 questionnaire is read during the edit. A live smoke run is hooked
only for semantic governance changes because:

- It needs an LLM, which means latency, cost, and credentials in the hook environment.
- It only adds value when operating-model semantics change.
- Step 1 already catches the regressions a script can catch; Step 2 catches the regressions that need reading comprehension.

Required workflow when you edit Oracle rules:

1. Run Step 1 (`bash audits/agents-md.sh`).
2. Answer every question in this file against the generated `AGENTS.md` and dispatch pages.
3. Emit the report. All-YES permits commit; any NO returns to the edit.
4. Let pre-push run one fixture per installed agent and generate commit-bound evidence.

---

## Adding a new scenario or question

When the operating model grows (new gate, new rule, new lifecycle wrinkle):

1. Add to `audits/agents-md.sh` if the invariant is mechanically checkable.
2. Add to this file as a new question if the invariant needs reading comprehension.
3. Update `REQUIRED_DISPATCH` / `CONFORM_KEYS` / `FORBIDDEN_KEYS` arrays in `audits/data.sh` as appropriate.
4. Run both layers and check in the new baseline.

The cost of this suite is bounded by these arrays. If they grow without bound, the suite is leaking complexity — split AGENTS.md before adding more invariants.
