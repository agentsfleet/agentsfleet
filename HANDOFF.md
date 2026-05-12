# Handoff — M65_002 commander refactor (session 5 → session 6)

**Date:** May 13, 2026 (session 5 close)
**Outgoing agent:** Session 5 — landed Step 7 (spec amend) + Step 7b
  (options-metavar E2E spec + EVENT_STATUS constants) + reviewed Captain's
  reversals on review #9 (SIGINT) and #6 (HTTP verbs — declined again per
  Captain). All session-5 commits are LOCAL ONLY — push happens once at
  the top of Step 8 per Captain's directive.
**Incoming agent:** Picks up at Step 7c (coverage gate ≥95% funcs) → Step 8
  (push + CHORE close + skill chain). Wait for "ship it" before opening
  the sibling docs PR.

---

## Where you are

- **Worktree:** `~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e`
- **Branch:** `chore/m65-002-spec-zombiectl-e2e-lifecycle` — **5 commits
  LOCAL ONLY** (push at Step 8): `99a7223a`, `aa22c6f4`, `a99ffb8f`,
  `e3c6b1a4`, `9d4cb80e`.
- **PR:** [#323](https://github.com/usezombie/usezombie/pull/323) — head
  `9ea6490f` on origin; the 5 local commits will push together at Step 8.
- **Docs sibling PR:** `chore/m65-002-zombiectl-cli-e2e-changelog` on
  `usezombie/docs` — branch exists, changelog block pushed earlier; no
  PR opened yet (waits for Captain's "ship it" at Step 8).

### Session 5 commits (newest first, ALL local-only)

| Commit | Step | What |
|---|---|---|
| `9d4cb80e` | 7b | `test(zombiectl): options-metavar E2E coverage + EVENT_STATUS constants` — new `test/acceptance/options-metavar.spec.js` (27 tests) pinning --help metavar / validator-reject / wire-request round-trip across 8 commands; new `src/constants/event-status.js` wired into `zombie_events.js` + `zombie_steer.js` (5 sites) — review #21 (gate_blocked / processed / agent_error). |
| `e3c6b1a4` | 6.1 (#9) | `feat(zombiectl): constantize SIGINT signal name (review #9 revisited)` — new `src/constants/signals.js`; core.js's two `process.on("SIGINT", …)` / `removeListener` sites now read `SIGINT` from the constant. Verified in both Node and Bun. |
| `a99ffb8f` | 7 | `docs(spec): M65_002 — Commander refactor evidence + close Discovery #10/#14/#15` — Verification Evidence amended with the Commander-refactor subsection; Discovery rows #10/#14/#15 marked resolved. |
| `aa22c6f4` | 4-handoff | session-4 HANDOFF (will be `git rm` at Step 8). |
| `99a7223a` | 6.1 (#1/#8/#10/#11/#12/#16) | session-4 constants tail (analytics-events, zombie-status, doctor-checks, auth-roles + URL constants in api-paths.js). |

### Current verification state

```
bun run lint        ✅ 0 warnings, 0 errors (131 files, 64 rules)
bun test            ✅ 625 pass / 2 skip / 0 fail / 800 expect() calls
bun run test        ✅ 574 pass / 0 fail (node --test + bun test combined)
make check-version  ✅ all versions match 0.34.0
LENGTH GATE         ✅ every touched file ≤ 350L (cli-tree.js at 345)
ERROR REGISTRY      ✅ no raw UZ-* literals outside the registry
gitleaks detect     ✅ no leaks found (pre-commit verified each commit)
HARNESS VERIFY      ✅ COMBINED audit 0 hits (RULE TST-NAM clean)
COVERAGE            ⚠ funcs 93.62% / lines 95.79% — Step 7c lifts to ≥95% funcs
```

---

## What landed in session 5

### Step 7 — spec amendment (commit `a99ffb8f`)

Appended a **"Commander refactor"** subsection to
`docs/v2/done/M65_002_P1_TESTING_ZOMBIECTL_E2E_LIFECYCLE.md` Verification
Evidence covering:

- Parser swap (parseFlags → commander 14 + validators.js)
- `ZombieHelp` subclass owning every level of help rendering
- Test-shim rationale in `test/helpers.js` (intentional adapters, NOT
  RULE NLR violations)
- `[--option <value>]` metavar convention adopted tree-wide
- Captain's 20-item review summary (8 landed / 12 declined with
  rationale)
- Barrel-collapse cleanup (`agent_external.js`/`tenant_provider.js`
  merged back)
- VERSION read from `package.json` at runtime

Discovery rows marked resolved with `✅ Resolved (Commander refactor)`:
**#10** (printHelp(jsonMode)), **#14** (validateRequiredId scatter),
**#15** (CLI constants centralisation).

### Step 7b — options-metavar E2E spec + EVENT_STATUS (commit `9d4cb80e`)

New `zombiectl/test/acceptance/options-metavar.spec.js` (290L, 27 tests)
pins three contracts for every option that takes a value across the
8 minimum-touchpoint commands (list / logs / events / install / login /
billing show / agent add / tenant provider add):

1. **`--help` body advertises the option with `<metavar>`** — e.g.
   `--limit <n>`, `--cursor <token>`, `--workspace <id>`. 8 tests.
2. **Validators reject invalid input with a clear stderr stem** —
   e.g. `--limit 0` → `"must be ≥ 1"`. 11 tests. ⚠ **CLI emits exit
   1, not 2, for commander.invalidArgument** today; the spec asserts
   non-zero + stem and notes that mapping to POSIX 2 is a separate
   cli.js hygiene PR (extend `COMMANDER_USAGE_CODES`).
3. **Valid values round-trip end-to-end to the wire request** — in-spec
   capturing HTTP stub records every `(method, url, body)`. 8 tests
   pinning `--limit`/`--cursor` in GET queries, `--name`/`--zombie`
   in agent-keys POST body, `--credential_ref`/`--model` in provider
   PUT body, `--from` in install error path, `--timeout-sec` in
   login poll loop wall-clock exit.

Also new: **`src/constants/event-status.js`** — Captain caught the
remaining wire-status literals (`"processed"`, `"agent_error"`,
`"gate_blocked"`) in `zombie_events.js` (renderStatus, 3 sites) and
`zombie_steer.js` (terminal detection + processed-exit-code, 2 sites).
Now centralised; mirrors Zig-side `STATUS_GATE_BLOCKED` constant per
RULE UFS cross-runtime. **For the next agent: this counts as a new
20-item review row (#21) — fold it into the Step 8 PR Session Notes.**

### Captain's reversals on session-4 declines

- **Review #9 SIGINT** — declined in session 4 ("Node has no built-in")
  was wrong: `os.constants.signals.SIGINT` exists. But the constant is
  an integer (signal number 2), and `process.on(signal, handler)` for
  signal events takes the string name. The CLI now reads `SIGINT` from
  `src/constants/signals.js` (a string export) so the symbolic-name
  intent is satisfied without breaking `process.on` in either Node or
  Bun. Commit `e3c6b1a4`.
- **Review #6 HTTP verbs** — Captain considered then declined again
  in session 5 (*"trivial and not needed"*). Bare `method: "GET"`
  stays as-is. **Do not revisit unless Captain re-opens.**

### One Discovery item surfaced but NOT acted on

Captain spotted `BILLING_DASHBOARD_URL = "https://app.usezombie.com/settings/billing"`
hardcoded in `src/commands/billing.js:28`. Dev operators hitting
`api-dev.usezombie.com` get pointed at the prod app (no override
vector, unlike `apiUrl` which has `--api`/`ZOMBIE_API_URL` precedence).
**Captain's call: leave it as-is for this PR**, surface as Discovery
for a future hygiene PR. Suggested fix shape: env-overridable
`appUrl` in `src/util/url.js` mirroring the `apiUrl` precedence, or
have the server return a `topup_url` on the billing payload.

---

## What the next session does

### Step 7c — close coverage to ≥95% functions (Captain-imposed gate)

Baseline at end of session 5: **93.62% funcs / 95.79% lines** (the
options-metavar spec did NOT meaningfully shift the unit-test
function-coverage number — acceptance specs spawn a subprocess so
the in-process coverage instrument doesn't see executions).

Gap-closer priorities (highest leverage first):

| File | Funcs | Strategy |
|---|---|---|
| `src/program/cli-tree.js` | 65.31% | Each `actionFor(name, fn)` closure fires only when commander dispatches the command. Add a focused unit test that drives `buildProgram({...}).parseAsync([...])` for each leaf — pure parser-level invocation, no real handlers needed (inject `runHandler` no-op via `handlers`). |
| `src/commands/zombie.js` | 72.73% | Direct-handler unit tests for install/status/stop/resume/kill branches not currently exercised. Lines 91, 147, 149–160, 176–177, 191, 195, 203–225 in the source map. |
| `src/output/index.js` | 80% | Trivial — re-exported helpers (`withGlyph`, etc.) not all called. Add a focused test or move into `coverage-fill.unit.test.js`. |
| `src/lib/browser.js` | 81.82% | Platform-fallback paths gated on env. Add WSL/SSH/missing-DISPLAY cases. |
| `src/cli.js` | 81.25% | Error-path tail at lines 243–257 — add a `runCli` test that throws a non-Commander error. |
| `src/commands/workspace.js` | 83.33% | Empty-state branch (post-credentials-redirect placeholder). |
| `src/commands/core.js` | 85.71% | Session-polling edge cases (expired/interrupted) at lines 187–188, 201–205. |
| `src/commands/zombie_steer.js` | 85.71% | SSE early-return paths at 70–72, 121–124. |
| `src/lib/sse.js` | 60% | Lines coverage is 96% — function count is the gap. Add 1–2 targeted tests. |

After adding tests, **re-run `bun test`** and confirm the `All files`
row shows ≥95% function coverage. Pin the before/after numbers in the
Step 8 PR Session Notes.

### Step 8 — CHORE(close) + push + skill chain

Required outputs:

1. **Push** all 5 local-only commits + any Step 7c commits. This is the
   moment Captain wanted the push to happen.
2. Append new `<Update>` block to `~/Projects/docs/changelog.mdx`. Use
   `~/Projects/dotfiles/skills/release-template.md` verbatim — don't
   paraphrase the version-bump matrix.
3. PR `## Session notes` with:
   - All decisions, assumptions, dead ends
   - `/write-unit-test` outcome
   - `/review` outcome
   - `kishore-babysit-prs` final report
   - **The 20-item review table** with session-5 corrections (table
     verbatim below — note the #9 SIGINT row flips to "Done", and a
     new #21 row appears for `gate_blocked`).
   - Before/after coverage deltas (93.62% → ≥95% funcs).
   - The "leave billing dashboard URL hardcoded for now" Discovery
     note + recommended fix shape for the follow-on PR.
4. `git rm HANDOFF.md`
5. `make check-version` passes
6. Orphan sweep complete (RULE ORP)
7. Open the sibling docs PR **only after Captain says "ship it"**.

**Skill chain (mandatory order):**
1. `/write-unit-test` — audit diff coverage. Iterate until clean.
2. `/review` — adversarial diff review. Address or document deferrals.
3. After CHORE(close) commits, `git push`.
4. `/review-pr` — greptile triage. Comments via `gh pr review`.
5. `kishore-babysit-prs` — poll greptile per cadence, fix P0/P1.

---

## Hard constraints (carry forward — read CLAUDE.md for full set)

- **350L cap** stays in force. Current high-water marks: `cli-tree.js`
  345, `options-metavar.spec.js` 290, `cli.js` 279. Step 7c unit tests
  for `cli-tree.js` go in a SEPARATE file (e.g.
  `test/cli-tree.parse.unit.test.js`) — do not edit `cli-tree.js`
  itself just to lift coverage.
- **RULE NLR + NLG**: no parallel paths, no "legacy" framing in any
  new code. Test-only shims in `test/helpers.js` are intentional
  adapters.
- **RULE TST-NAM**: no milestone IDs / § markers in test file source.
  The combined audit in HARNESS VERIFY catches this — already bit
  me once in session 5 (had to strip `M65_002` from a docstring).
- **gitleaks + lint + harness-verify** stay clean before every commit
  (pre-commit hook enforces).
- **External commitment** (sibling docs PR for changelog) needs a
  paired PR at Step 8 — branch already exists on `usezombie/docs`.
- **Coverage ≥95% function** is a Captain-imposed gate before Step 8
  closes. Pin the number in PR Session Notes.

### Operating mode

- Auto mode is active — standing authorization for focused commits +
  non-force pushes to the feature branch + `gh pr update`. You may NOT
  merge the PR, force-push, or open the sibling docs PR without an
  explicit "ship it" / "land it" from Captain.
- Captain is Kishore. Email `kishore.kumar@e2enetworks.com` (work),
  `nkishore@megam.io` (personal).
- Stay inside this worktree; no sibling-worktree reaches.

---

## First 5 actions

1. `cd ~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e`
2. `cat HANDOFF.md` — read this file.
3. `git log --oneline -8` — confirm `9d4cb80e` is HEAD and the 5 local
   commits listed above are present but not on origin
   (`git log origin/chore/m65-002-spec-zombiectl-e2e-lifecycle..HEAD`
   shows 5 commits).
4. `cd zombiectl && bun test --coverage 2>&1 | grep "All files"` —
   confirm 93.62% baseline.
5. Start Step 7c: lift `src/program/cli-tree.js` first (biggest delta —
   65% → ~95%) by adding `test/cli-tree.parse.unit.test.js` that drives
   `buildProgram` with no-op handlers and asserts each action closure
   fires for its argv. That single file should lift All-files funcs
   by several points.

---

## 20-item review table (session-5 corrections — paste verbatim into PR Session Notes at Step 8)

| # | Finding | Resolution | Fix-location |
|---|---|---|---|
| 1 | Static analytics/status strings need consts | **Done (session 4)** — 4 new constants modules + per-emit-site wiring | `src/constants/{analytics-events,zombie-status,doctor-checks}.js` |
| 1b | `workspace_created` → `workspace_added` rename | **Declined this PR** — external PostHog surface; coordinated rename later | M65_002 spec Discovery |
| 2 | `workspaceShow` uses both `workspaceId` + `workspace-id` | **Correct** — commander camelCase + legacy dashed; both forms documented | Spec amendment §Commander refactor |
| 3 | `active: "yes"/"no"` standard? | **Kept** — human surface uses yes/no, JSON uses booleans | n/a (rationale doc only) |
| 4 | `workspaceDelete` uses both `workspace-id` + `workspace_id` | **Correct** — CLI flag vs JSON field name; different roles | Spec amendment §Commander refactor |
| 5 | Rename `workspace_add_completed` → `workspace_added` | **Declined** (same as 1b) | M65_002 spec Discovery |
| 6 | HttpVerb constants (`POST`/`GET`/`PATCH`) | **Declined (session 5 reaffirmed)** — Captain considered, declared "trivial and not needed"; bare string in `method:` is the convention | n/a |
| 7 | `[OK]` / `[FAIL]` constantize | **Declined** — single-file usage in `core-ops.js` | n/a |
| 8 | URL constants for `/healthz`, `/v1/auth/sessions`, `/v1/workspaces`, `/v1/tenants/me/billing` | **Done (session 4)** — `src/lib/api-paths.js` extended | `src/lib/api-paths.js` and call sites |
| 9 | `"SIGINT"` constant | **Done (session 5)** — `src/constants/signals.js` exports the string name; core.js's two sites read from it. `os.constants.signals.SIGINT` is the numeric form (works for syscalls, not for `process.on` listener registration) | `src/constants/signals.js`, `src/commands/core.js` |
| 10 | `{ status: "ok" }` envelope | **Done (session 4)** — `HEALTHZ_STATUS_OK = "ok"` in api-paths.js | `src/lib/api-paths.js` |
| 11 | "credential vault ships once backing feature lands" placeholder | **Done (session 4)** — backing feature IS shipped (`/credentials` route); re-framed as redirect | `src/commands/workspace.js` |
| 12 | `const limit = parsed.options.limit \|\| "20"` const | **Done (session 4)** — `DEFAULT_LOGS_LIMIT = "20"` | `src/commands/zombie_logs.js` |
| 13 | `parsed` null/undefined handling in `commandSteer`? | **No guard needed** — invariant: commander + buildParsed both always supply `{options, positionals}` | Spec amendment §Commander refactor |
| 14 | `"utf8"` built-in const | **Declined** — canonical literal | n/a |
| 15 | Rename `runCli` → `runCLI` | **Declined** — convention call | n/a |
| 16 | `"admin"` const | **Done (session 4)** — `ROLE_ADMIN` / `ROLE_USER` in `src/constants/auth-roles.js` | `src/constants/auth-roles.js` |
| 17 | `VERSION = "0.34.0"` read from package.json | **Done (session 4)** — `cli.js` reads `package.json` at module load | `src/cli.js`, `make/build.mk` |
| 18 | `make sync-version` target | **Verified + updated (session 4)** — 2-file rewrite | `make/build.mk` |
| 19 | `"user"` (commander argv source) const | **Declined** — local literal, low value | n/a |
| 20 | "autonomous agent platform" tagline | **Kept** — generic-descriptive CLI tagline; 5+ tests pin the string | n/a |
| 21 | Event status literals `"processed"` / `"agent_error"` / `"gate_blocked"` | **Done (session 5)** — `src/constants/event-status.js` mirrors Zig-side `STATUS_GATE_BLOCKED`; wired into `zombie_events.js` renderStatus + `zombie_steer.js` terminal detection (5 sites) | `src/constants/event-status.js`, `src/commands/zombie_events.js`, `src/commands/zombie_steer.js` |
| UX | `<token>` / `<n>` metavar convention | **Adopted + verified (session 5)** — `test/acceptance/options-metavar.spec.js` pins --help body, validator reject, and wire-request round-trip for every option-bearing command | `src/program/cli-tree.js`, `validators.js`, `test/acceptance/options-metavar.spec.js` |

**Discovery surfaced this session (NOT acted on; for a follow-on PR):**

- `BILLING_DASHBOARD_URL` hardcoded to `https://app.usezombie.com/settings/billing`
  in `src/commands/billing.js:28`. Breaks dev (`api-dev.usezombie.com` →
  prod app). Suggested fix: env-overridable `appUrl` in `src/util/url.js`
  mirroring `apiUrl` precedence (`--app` > `ZOMBIE_APP_URL` > derive
  `api[-x]` → `app[-x]` > fallback). Captain declared "leave it as-is
  for this PR".
- CLI exit code 1 (not POSIX 2) for `commander.invalidArgument` —
  `COMMANDER_USAGE_CODES` set in `src/cli.js` evidently doesn't match
  the wrapped error code commander 14 emits. Separate cli.js hygiene
  PR can map invalidArgument to exit 2.

---

## Files NOT to touch this session

- `~/Projects/docs/` — sibling PR territory. Wait for Step 8 + "ship it".
- `.github/workflows/auth-e2e-{dev,prod}.yml` — out of scope.
- Any sibling worktree — stay inside this one.
- `src/commands/billing.js` BILLING_DASHBOARD_URL — Captain's call.

---

## Cross-agent note

This is a Claude → Claude handoff. Session 5 stayed inside the worktree
throughout. Session 6 should too.

**Delete this file at the end of CHORE(close):** `git rm HANDOFF.md`
in the final commit.
