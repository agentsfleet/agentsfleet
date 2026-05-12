# Handoff — M65_002 commander refactor (session 4 → session 5)

**Date:** May 13, 2026 (session 4 close)
**Outgoing agent:** Session 4 — landed Steps 6 + 6.1 + a few opportunistic
  fixes Captain greenlit mid-session (Redocly bump, barrel collapse).
  **Step 6.1 IS committed locally but not yet pushed** — Captain's directive
  was to push after Step 7b, not after 6.1.
**Incoming agent:** Picks up at Step 7 (spec amend) → Step 7b (E2E option
  metavar coverage) → commit + push → Step 8 (CHORE close + skill chain).
  Final verification gate added: total function coverage ≥95%.

---

## Where you are

- **Worktree:** `~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e`
- **Branch:** `chore/m65-002-spec-zombiectl-e2e-lifecycle` — 1 commit
  **local-only** (99a7223a), 23 commits pushed to origin.
- **PR:** [#323](https://github.com/usezombie/usezombie/pull/323) — open,
  needs the Step 7 spec amendment + the 20-item review table in
  Session Notes at CHORE(close).
- **Docs sibling PR:** `chore/m65-002-zombiectl-cli-e2e-changelog` on
  `usezombie/docs` — branch exists, changelog block pushed earlier;
  no PR opened yet (waits for Captain's "ship it" at Step 8).

### Session 4 commits

| Commit | Step | What |
|---|---|---|
| `9ad7fdbc` | 6 | `chore(zombiectl): post-swap dead-code sweep (RULE ORP)` — deleted `cli-actions.js` + `helpers-fs.js`, trimmed 8 unused `OPT_*` exports, stripped dead `parseFlags:` deps from 6 test fixtures, scrubbed stale comments. |
| `f60d1b0f` | bonus | `chore(deps): bump @redocly/cli ^2.30.4 → ^2.30.5` (Captain saw the nag in pre-commit and asked) |
| `fa376cff` | bonus | `chore(zombiectl): collapse barrel files + trim http-client surface` — merged `agent_external.js → agent.js`, `tenant_provider.js → tenant.js`, deleted redundant `test/browser.unit.test.js`, trimmed 3 unused exports from `http-client.js`. |
| `9ea6490f` | 6.1 (#17/#18) | `feat(zombiectl): VERSION read from package.json at runtime` — `cli.js` no longer carries the hardcoded string; `make sync-version` + `make check-version` updated. |
| `99a7223a` | 6.1 (#1/#8/#10/#11/#12/#16) | `feat(zombiectl): constantize analytics, status, doctor, roles, paths` — 4 new constants modules, URL constants in `api-paths.js`, fixed workspace-credentials placeholder text (the feature shipped in the UI). **LOCAL ONLY — push after Step 7b.** |

### Current verification state

```
bun run lint        ✅ 0 warnings, 0 errors (128 files, 64 rules)
bun test            ✅ 598 pass / 2 skip / 0 fail / 800 expect() calls
bun run test        ✅ 574 pass / 0 fail (node --test + bun test)
make check-version  ✅ all versions match 0.34.0
LENGTH GATE         ✅ every touched file ≤ 350L (cli-tree.js at 345)
ERROR REGISTRY      ✅ no raw UZ-* literals outside the registry
gitleaks detect     ✅ no leaks found (pre-commit verified each commit)
COVERAGE            ⚠ funcs 93.62% / lines 95.79% — see Step 7b gap notes
```

---

## What landed in Step 6.1 (Captain's 20-item review)

**New constants modules (one named export per fact):**
- `src/constants/analytics-events.js` — `EVT_*` PostHog event names.
- `src/constants/zombie-status.js` — frozen `ZOMBIE_STATUS = {STOPPED, ACTIVE, KILLED}`.
- `src/constants/doctor-checks.js` — frozen `DOCTOR_CHECK = {SERVER_REACHABLE, WORKSPACE_SELECTED, WORKSPACE_BINDING_VALID}`.
- `src/constants/auth-roles.js` — `ROLE_ADMIN` / `ROLE_USER`.

**`src/lib/api-paths.js` extensions** — `HEALTHZ_PATH`, `AUTH_SESSIONS_PATH`,
`WORKSPACES_COLLECTION_PATH`, `TENANT_BILLING_PATH`, `TENANT_PROVIDER_PATH`,
`HEALTHZ_STATUS_OK` body sentinel.

**Workspace credentials placeholder fix (item #11):** the `/credentials`
dashboard route shipped in `ui/packages/app/app/(dashboard)/credentials/page.tsx`,
so the "coming soon" CLI text was incorrect. Re-framed as a redirect to
`/credentials` (dashboard) or `zombiectl zombie credential` (per-zombie).
Tests in `cli-alignment.unit.test.js` + `workspace.unit.test.js` updated
to assert the new wording.

**Resolved 20-item review table** — every row resolved in HANDOFF.md
session-3 had a recommendation; session 4 acted on the ones that needed
code (#1, #8, #10, #11, #12, #16, #17, #18) and declined the rest with
documented rationale. The full table needs to land in the PR's
`## Session notes` block at Step 8 — drop the table verbatim from
session-3 HANDOFF.md (the resolutions there match what landed).

---

## What the next session does

### Step 7 — Amend M65_002 spec Verification Evidence

`docs/v2/done/M65_002_P1_TESTING_ZOMBIECTL_E2E_LIFECYCLE.md` — append a
**"Commander refactor"** subsection. Cover:

- Parser swap (`parseFlags` → commander 14 + `validators.js`)
- Help-output shape change (`printHelp` deleted, `ZombieHelp` subclass owns rendering)
- The test-shim approach in `test/helpers.js` (`buildParsed`, `commandZombieDispatch`,
  `createCoreHandlers`, `commandBilling`, `commandTenant`) with the rationale
  that direct-handler tests still verify leaf behavior. Call out that these
  are intentional test-only adapters so a future reviewer doesn't flag them
  as RULE NLR violations.
- The `[--option <value>]` metavar convention adopted across the tree
  (Captain's UX directive: angle-bracket form is fine as long as the user
  is explained — Step 7b verifies the explanation surface).
- Captain's 20-item review outcomes (link to the tabulated table in the
  PR body).
- The barrel-collapse cleanup (`agent.js`/`tenant.js` are now single-file
  per group; `_external`/`_provider` suffixes retired).
- The `VERSION` read-from-package.json change (`make sync-version` is now
  a 2-file rewrite, not 3).

Mark Discovery rows resolved:
- #10 `printHelp(jsonMode)` JSON help → resolved; structured JSON help
  is a future enhancement, the CLI surfaces commander's text body today.
- #14 / #15 — handler signature consistency closes the inline-validation
  scatter (every option now flows through `validators.js`).

### Step 7b — E2E coverage for `--option <value>` round-trips

After Step 7, before Step 8. **NEW required file:**
`zombiectl/test/acceptance/options-metavar.spec.js`

Prove every option that takes a value:
1. Is documented with the consistent `<metavar>` convention in `zombiectl <cmd> --help`.
2. Rejects invalid input via the validator with a clear error
   (`--limit 0` → "must be ≥ 1").
3. Flows through to the handler in a real CLI invocation.

Minimum touchpoints:
- `zombiectl list --limit 25` / `--cursor abc123`
- `zombiectl logs --limit 50`
- `zombiectl events --limit 100 --since 2h --actor "steer:*"`
- `zombiectl install --from <existing-path>`
- `zombiectl login --timeout-sec 5 --poll-ms 100`
- `zombiectl billing show --limit 5 --cursor xyz`
- `zombiectl agent add --workspace <id> --zombie <id> --name <s>`
- `zombiectl tenant provider add --credential <name> --model <override>`

Each must round-trip end-to-end (commander parses → validator validates
→ handler observes). Captain's exact words from session 3:
*"as long as we explain to the user we are fine. and those options must
be tested end to end and work"*.

### Coverage gate (≥95% functions) — NEW from Captain in session 4

Baseline at end of session 4: **93.62% funcs / 95.79% lines**.

Gap-closer priorities (highest leverage first):
| File | Funcs | Strategy |
|---|---|---|
| `src/program/cli-tree.js` | 65.31% | Step 7b's E2E coverage will lift this substantially (action closures fire only when a command is run end-to-end) |
| `src/commands/zombie.js` | 72.73% | Add direct-handler unit tests for the install/status/stop/resume/kill branches that aren't currently exercised. Lines 91, 147, 149-160, 176-177, 191, 195, 203-225 in the source map. |
| `src/output/index.js` | 80% | Trivial — re-exported helpers (`withGlyph` etc.) not all called. Add a focused test or move into `coverage-fill.unit.test.js`. |
| `src/lib/browser.js` | 81.82% | Platform-fallback paths gated on env. Add WSL/SSH/missing-DISPLAY cases. |
| `src/cli.js` | 81.25% | Error-path tail at lines 243-257. Add a runCli test that throws a non-Commander error. |
| `src/commands/workspace.js` | 83.33% | The new redirect placeholder added test coverage; small remaining gap on the empty-state branch. |
| `src/commands/core.js` | 85.71% | Session-polling edge cases (expired/interrupted) at lines 187-188, 201-205. |
| `src/commands/zombie_steer.js` | 85.71% | SSE early-return paths at 70-72, 121-124. |
| `src/lib/sse.js` | 60% | The `lines coverage is 96%` so this is a small function count thing — add 1-2 targeted tests. |

After Step 7b, **re-run `bun test`** and confirm `All files` row shows
≥95% function coverage before continuing to Step 8.

### Step 8 — CHORE(close) + skill chain

Required outputs:

1. **Push** the local-only commit (`99a7223a`) PLUS the Step 7 + 7b commits.
   This is the moment Captain wanted the push to happen.
2. Append new `<Update>` block to `~/Projects/docs/changelog.mdx`. Use
   `~/Projects/dotfiles/skills/release-template.md` verbatim — don't
   paraphrase the version-bump matrix.
3. PR `## Session notes` with:
   - All decisions, assumptions, dead ends
   - `/write-unit-test` outcome
   - `/review` outcome
   - `kishore-babysit-prs` final report
   - **Captain's 20-item review table** — copy from session-3 HANDOFF.md
     verbatim. The "fix-location" column entries are accurate as committed.
   - Before/after coverage deltas (93.62% → ≥95% funcs).
4. `git rm HANDOFF.md`
5. `make check-version` passes
6. Orphan sweep complete (RULE ORP — re-run the three sweeps from
   session-3 HANDOFF for paranoia)
7. Open the sibling docs PR **only after Captain says "ship it"**.

**Skill chain (mandatory order):**
1. `/write-unit-test` — audit diff coverage vs the spec's Test
   Specification. Iterate until clean.
2. `/review` — adversarial diff review. Address or document deferrals.
3. After CHORE(close) commits, `git push`.
4. `/review-pr` — greptile triage. Comments via `gh pr review`.
5. `kishore-babysit-prs` — poll greptile per cadence, fix P0/P1.

---

## Hard constraints (carry forward — read CLAUDE.md for full set)

- **350L cap** stays in force. Current high-water marks: `cli-tree.js`
  at 345 (5 lines from the cap — watch this when wiring Step 7b
  validators if you add new option declarations), `cli.js` at 279
  (room to grow).
- **RULE NLR + NLG**: no parallel paths, no "legacy" framing in any
  new code. Test-only shims in `test/helpers.js` are intentional
  adapters — call out the rationale in the Step 7 spec amendment.
- **RULE TST-NAM**: no milestone IDs / § markers in test file source.
  The combined audit in HARNESS VERIFY catches this.
- **gitleaks + lint + harness-verify** stay clean before every commit
  (pre-commit hook enforces).
- **External commitment** (sibling docs PR for changelog) needs a
  paired PR at Step 8 — branch already exists on `usezombie/docs`.
- **Coverage ≥95% function** is now a Captain-imposed gate before
  Step 8 closes. Pin the number in PR Session Notes.

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
2. `cat HANDOFF.md` — read the full brief (this file).
3. `git log --oneline -8` — confirm `99a7223a` is HEAD and is
   **local-only** (not on origin).
4. `git status` — should be clean.
5. Start Step 7: open `docs/v2/done/M65_002_P1_TESTING_ZOMBIECTL_E2E_LIFECYCLE.md`,
   find the Verification Evidence section, append the Commander-refactor
   subsection covering everything in session 4's scope.

---

## 20-item review table (verbatim from session 3, all rows resolved)

(Paste this block into the PR's `## Session notes` at CHORE(close) — the
"Resolution" column reflects what actually landed in session 4.)

| # | Finding | Resolution | Fix-location |
|---|---|---|---|
| 1 | Static analytics/status strings need consts | **Done** — 4 new constants modules + per-emit-site wiring | `src/constants/{analytics-events,zombie-status,doctor-checks}.js` |
| 1b | `workspace_created` → `workspace_added` rename | **Declined this PR** — external PostHog surface; coordinated rename later | M65_002 spec Discovery |
| 2 | `workspaceShow` uses both `workspaceId` + `workspace-id` | **Correct** — commander camelCase + legacy dashed; both forms documented | Spec amendment §Commander refactor |
| 3 | `active: "yes"/"no"` standard? | **Kept** — human surface uses yes/no, JSON uses booleans. Future: `BOOLEAN_DISPLAY` const if a second surface appears | n/a (rationale doc only) |
| 4 | `workspaceDelete` uses both `workspace-id` + `workspace_id` | **Correct** — CLI flag vs JSON field name; different roles | Spec amendment §Commander refactor |
| 5 | Rename `workspace_add_completed` → `workspace_added` | **Declined** (same as 1b) | M65_002 spec Discovery |
| 6 | HttpVerb constants (`POST`/`GET`/`PATCH`) | **Declined** — convention is bare string in `method:` field; no rename risk | n/a |
| 7 | `[OK]` / `[FAIL]` constantize | **Declined** — single-file usage in `core-ops.js`; trivial cost to inline | n/a |
| 8 | URL constants for `/healthz`, `/v1/auth/sessions`, `/v1/workspaces`, `/v1/tenants/me/billing` | **Done** — `src/lib/api-paths.js` extended with flat routes + `HEALTHZ_STATUS_OK` envelope sentinel | `src/lib/api-paths.js`, `core-ops.js`, `core.js`, `workspace.js`, `tenant.js`, `billing.js` |
| 9 | `"SIGINT"` constant | **Declined** — Node has no built-in; literal is the convention | n/a |
| 10 | `{ status: "ok" }` envelope | **Done** — `HEALTHZ_STATUS_OK = "ok"` in api-paths.js | `src/lib/api-paths.js` |
| 11 | "credential vault ships once backing feature lands" placeholder | **Done** — backing feature IS shipped (`/credentials` route). Re-framed CLI message as a redirect | `src/commands/workspace.js`, tests updated |
| 12 | `const limit = parsed.options.limit \|\| "20"` const | **Done** — `DEFAULT_LOGS_LIMIT = "20"` | `src/commands/zombie_logs.js` |
| 13 | `parsed` null/undefined handling in `commandSteer`? | **No guard needed** — invariant: commander + buildParsed both always supply `{options, positionals}` | Spec amendment §Commander refactor |
| 14 | `"utf8"` built-in const | **Declined** — canonical literal | n/a |
| 15 | Rename `runCli` → `runCLI` | **Declined** — convention call; ~15 import sites for casing preference | n/a |
| 16 | `"admin"` const | **Done** — `ROLE_ADMIN` / `ROLE_USER` in `src/constants/auth-roles.js`, wired in `cli.js` | `src/constants/auth-roles.js` |
| 17 | `VERSION = "0.34.0"` read from package.json | **Done** — `cli.js` reads `package.json` at module load; `make sync-version` updated | `src/cli.js`, `make/build.mk` |
| 18 | `make sync-version` target | **Verified + updated** — now a 2-file rewrite (build.zig.zon, zombiectl/package.json) since cli.js reads pkg at runtime | `make/build.mk` |
| 19 | `"user"` (commander argv source) const | **Declined** — local literal, low value | n/a |
| 20 | "autonomous agent platform" tagline | **Kept** — README owns the marketing line ("Your deploy failed. The agent already knows why."), the CLI tagline stays generic-descriptive; 5+ tests pin the string | n/a |
| UX | `<token>` / `<n>` metavar convention | **Adopted across tree** — Step 7b verifies the explanation surface (help body, README, error messages) | `src/program/cli-tree.js`, validators.js |

---

## Files NOT to touch this session

- `~/Projects/docs/` — sibling PR territory. Wait for Step 8.
- `.github/workflows/auth-e2e-{dev,prod}.yml` — out of scope.
- Any sibling worktree — stay inside this one.

---

## Cross-agent note

This is a Claude → Claude handoff. Session 4 stayed inside the worktree
throughout. Session 5 should too.

**Delete this file at the end of CHORE(close):** `git rm HANDOFF.md`
in the final commit.
