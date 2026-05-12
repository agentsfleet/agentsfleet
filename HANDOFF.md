# Handoff — M65_002 commander refactor (session 3 → session 4)

**Date:** May 12, 2026 (session 3 close)
**Outgoing agent:** Session 3 — landed Steps 1-5 of the commander refactor;
  Step 5 (atomic swap) shipped via commit `9071d5c1`.
**Incoming agent:** Picks up at Step 6 (post-swap dead-code sweep) →
  Captain's 20-item review → Step 7 (spec amend) → Step 7b (E2E option
  metavar coverage) → Step 8 (CHORE close + skill chain).

---

## Where we are

- **Worktree:** `~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e`
- **Branch:** `chore/m65-002-spec-zombiectl-e2e-lifecycle` (12 commits
  ahead of `main`, all pushed)
- **PR:** [#323](https://github.com/usezombie/usezombie/pull/323) — open,
  body still says "M65_002 zombiectl e2e full lifecycle scenarios";
  needs the commander section appended at CHORE(close).
- **Docs sibling PR:** `chore/m65-002-zombiectl-cli-e2e-changelog` on
  `usezombie/docs` — changelog block pushed, no PR opened yet.

### Session 3 commits (all pushed)

| Commit | Step | What |
|---|---|---|
| `bbf9785c` | — | Fix `run-tests.mjs` recursive walk (the blocker) |
| `e52fe886` | 1 | `chore(zombiectl): add commander@^14.0.3 dependency` |
| `7d9e7ef6` | 2 | `feat(zombiectl): commander option validators` (8 parsers, 51 tests, 96.88% coverage) |
| `e2c8553f` | 3 | `feat(zombiectl): ZombieHelp commander.Help subclass` (10 tests, 100% line coverage) |
| `9071d5c1` | 4+5 | `feat(zombiectl): atomic commander swap + delete legacy dispatch` (the big one) |

### Pre-session-3 commits (already on the branch before this session)

| Commit | What |
|---|---|
| `e9381a16` | CHORE(close) — spec → done/, AUTH.md CLI carve-out |
| `e43db310` | `cli-acceptance-{dev,prod}` workflow jobs |
| `76c408d5` | §4 + §5 acceptance specs |
| `7a6332b1` | uuidv7 validator swap + UFS constants |
| `8f58e281` | flags-and-env spec + SIGINT handler |
| `894edbe5` | help-and-errors spec + CLI carve-outs |
| `4ca719ea` | scaffold acceptance harness |
| `19a7c607` | CHORE(open) — spec promoted to active/ |

### Current verification state

  bun run lint        ✅ 0 warnings, 0 errors (137 files, 64 rules)
  bun test            ✅ 601 pass / 2 skip / 0 fail / 800 expect()
  bun run test        ✅ 577 pass / 0 fail (node --test + bun test)
  gitleaks detect     ✅ no leaks found
  LENGTH GATE         ✅ every touched file ≤ 350L
  ERROR REGISTRY      ✅ no raw UZ-* literals outside the registry

The PR has CI running against this commit — should arrive green
(but Captain has not yet rerun `cli-acceptance-{dev,prod}` against the
new dispatch path; that's a Step 7b verification item).

---

## What landed in Step 5 (the atomic swap)

### New files
- `src/program/cli-tree.js` (345 lines) — `buildProgram({handlers, version, state})`. Every command + option declared once; commander's `optsWithGlobals()` + a dashed-key normaliser feeds the legacy OPT_* string-key readers.
- `src/program/handlers-bind.js` (136 lines) — extracted from cli.js to keep the entry-point under the 350 cap. Maps each commander frame through `runCommand()` so ApiError → friendly remap + analytics stay co-located.
- `src/util/url.js` (9 lines) — `normalizeApiUrl` + `DEFAULT_API_URL` extracted from the deleted `program/args.js`.

### Rewritten
- `src/cli.js` — ~230 lines. Pre-scans argv for `--version`, promotes `[]` → `["--help"]`, installs auth-guard via `program.hook("preAction", …)`, maps commander usage codes to exit 2.
- `src/program/io.js` — `printHelp`/`helpRow`/`HELP_NAME_WIDTH` deleted; ZombieHelp owns help rendering.
- Every command handler now takes `(ctx, parsed, workspaces, deps)` with `parsed = {options, positionals}`.

### Deleted
- `src/program/args.js` · `routes.js` · `command-registry.js` · `suggest.js`
- 7 test files (per audit): `args.unit` / `cli-dispatch-sweep` / `command-dispatchers-unknown-action` / `help-coverage` / `parse` / `registry` / `suggest`

### Test shims (in `test/helpers.js` — TEST-ONLY)
Added to keep direct-handler tests passing without rewriting every call site:
- `buildParsed(tokens)` — synthesises `{options, positionals}` from a flat token array.
- `commandZombieDispatch(ctx, args, ws, deps)` — re-creates the deleted `commandZombie` switch from the new leaf exports.
- `createCoreHandlers(ctx, ws, deps)` — re-creates the deleted core handlers map.
- `commandBilling(ctx, args, ws, deps)` — re-creates the deleted billing dispatcher.
- `commandTenant(ctx, args, ws, deps)` — re-creates the deleted tenant dispatcher (honours `deps.parseFlags`/`parseFlagsImpl` if injected).

**The shims are intentional** — direct-handler tests still verify leaf behavior (validation paths, error stems, return codes). Step 7b (option metavar e2e coverage) is the place to migrate the tests off the shims if you want a clean break, but the shims aren't blocking.

---

## What the next session does

### Step 6 — Post-swap dead-code sweep (priority: HIGH)

Run the three sweeps from HANDOFF.md (session-2 version, captured below):

```bash
cd ~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e/zombiectl

# 1. Unused exports across the package
bunx knip --workspace . || bun x ts-prune

# 2. Orphan-import sweep — anything still referencing the deleted modules
rg -n 'from ["'"'"'].*\b(args|routes|command-registry|suggest)\.js["'"'"']' src test || echo "OK: no orphan imports"

# 3. Symbol-reference sweep for the deleted internals
for sym in parseFlags parseGlobalArgs findRoute registerProgramCommands printHelp suggestCommand; do
  hits=$(rg -nw "$sym" src test --glob '!test/helpers.js' 2>/dev/null)
  if [ -n "$hits" ]; then
    echo "FAIL: $sym still referenced:"
    echo "$hits"
  else
    echo "OK: $sym — no production references (test/helpers.js shims allowed)"
  fi
done

# Length-gate audit (paranoia check)
find src -name "*.js" -exec wc -l {} \; | awk '$1 > 350 {print}'
```

All three must come back clean. RULE ORP is non-negotiable.

Anything they surface goes in a single follow-up commit `chore(zombiectl): post-swap dead-code sweep` before Step 7.

### Step 6.1 — Captain's 20-item review

**This came in mid-session-3 (May 12, 2026) and is queued for the next agent to address with a tabulated response.** Below is the full review verbatim + my recommended resolution for each.

The Captain wants: tabulated finding → fix-location response. Build it as you address each item.

| # | Finding | My recommendation | Where to fix |
|---|---|---|---|
| 1 | Static strings (`stopped`, `active`, `killed`, `complete`, `expired`, `interrupted`, `logout_completed`, `server_reachable`, `workspace_selected`, `workspace_binding_valid`, `workspace_added`, `workspace_list_viewed`, `workspace_id`, `workspace_used`, `workspace_deleted`, `login`, `unknown`, `workspace_created`, `user_authenticated`) need constantization | Create `src/constants/analytics-events.js` for analytics event names and `src/constants/zombie-status.js` for status values. The doctor check-name strings (`server_reachable` etc.) live in core-ops.js — own a `DOCTOR_CHECK` const. Don't constantize `workspace_id` field names (they're JSON keys, not logic constants — UFS exempts object key names per the audit). | `src/constants/{analytics-events,zombie-status,doctor-checks}.js` + readers in `commands/*.js` |
| 1b | `workspace_created` vs `workspace_added` rename | These are external surfaces (PostHog events). Rename **breaks downstream dashboards** unless coordinated. Recommend: do NOT rename in this PR. Capture as Discovery in the M65_002 spec for a follow-up that drops a deprecation window. | M65_002 spec Discovery |
| 2 | `workspaceShow` uses both `workspaceId` and `workspace-id` — is that correct? | YES. `workspaceId` is commander's camelCase form (from `--workspace-id`); `workspace-id` is the legacy dashed form (matches `OPT_WORKSPACE_ID` constant). The `resolveOption` helper tries both so OPT_* readers keep working. **Standard:** commander emits camelCase on `cmd.opts()`; the dashed-key normaliser in cli-tree.js's `actionFor` mirrors every key. | Already correct; document in spec amendment |
| 3 | `active: detail.active ? "yes" : "no"` standard? | Mixed. The JSON shape uses booleans; the human-text fallback uses "yes"/"no". Industry convention is "yes"/"no" or ✓/✗ for human output. Recommend: keep "yes"/"no" but constantize as `BOOLEAN_DISPLAY.yes/no` in `src/constants/display.js` if we add other surfaces. | `src/commands/workspace.js` (workspaceShow) + new `src/constants/display.js` |
| 4 | `workspaceDelete` uses both `workspace-id` and `workspace_id` | DIFFERENT THINGS. `workspace-id` is the **CLI flag** (`--workspace-id`); `workspace_id` is the **JSON field name** in API responses + the validator's argument name (`validateRequiredId(id, "workspace_id")`). The first feeds option lookup; the second drives the error message stem. Both are correct in their roles. | Document in spec amendment |
| 5 | Rename `workspace_add_completed` → `workspace_added` AND `workspace_created` → `workspace_added` | Same as #1b — external surface, breaks dashboards. Do NOT in this PR. | M65_002 spec Discovery |
| 6 | HttpVerb constants (`POST`/`GET`/`PATCH`/`DELETE`) | Convention in this codebase: bare string literals as the `method:` field of fetch-like calls. JS doesn't ship a built-in HttpMethod enum. Recommend: keep as-is; adding a constant module would touch ~40 sites for negligible gain (no rename ever happens, no typo risk because the strings are checked by the runtime). | DECLINE — document rationale in spec amendment |
| 7 | `[OK]` / `[FAIL]` constantize | Used in 1 file (core-ops.js, doctor output). Recommend: inline OK; `DOCTOR_BADGE.ok = "[OK]"` if we add a second doctor surface. | DECLINE for now |
| 8 | URL constants for `/healthz` `/v1/workspaces` | Mixed. `wsZombiesPath` etc. ARE in `src/lib/api-paths.js`. The bare strings still inline (`/healthz`, `/v1/auth/sessions`, `/v1/workspaces`, `/v1/tenants/me/billing`, etc.) should migrate to api-paths.js. | `src/lib/api-paths.js` + readers in `core-ops.js` (doctor), `core.js` (login + hydrate), `workspace.js` (add), `tenant_provider.js`, `billing.js` |
| 9 | `"SIGINT"` constant | Node has no built-in. The convention is the literal string. Decline. | DECLINE |
| 10 | `{ status: "ok", … }` — HTTP.status.200 or OK? | Server's healthz returns `{status: "ok"}` (the LITERAL string, not HTTP status). It's the body envelope. Recommend: constantize as `HEALTHZ_STATUS_OK = "ok"` if you want symmetry with the analytics-events sweep. Low priority. | `src/constants/healthz.js` (or document) |
| 11 | "workspace credential vault ships once backing feature lands" — verify against `ui/packages/app/` | Need to check whether the workspace-credential UI actually exists today. Run: `rg -nw "workspace.*credential" ui/packages/app/src` — if a real UI exists, update the placeholder text; if not, keep. | Investigate before changing |
| 12 | `const limit = parsed.options.limit \|\| "20"` — const | Yes. Add `const DEFAULT_LOGS_LIMIT = 20;` at top of `zombie_logs.js`. | `src/commands/zombie_logs.js` |
| 13 | `parsed` null/undefined handling in `commandSteer`? | commander always builds parsed from `cmd.opts()` + `cmd.args` — never null. The shim in helpers.js always calls buildParsed which returns `{options: {}, positionals: []}` — never null. **The handler doesn't need a null guard** in production. If you want to be defensive at the test-shim boundary, the shim could `parsed ?? buildParsed([])`. Recommend: document the invariant; no guard. | Document in spec amendment |
| 14 | `"utf8"` built-in const? | Node has `buffer.constants.MAX_LENGTH` etc., but the encoding name is canonically the literal string "utf8" / "utf-8". `Buffer.from(x, "utf8")` is THE form. Decline. | DECLINE |
| 15 | Rename `runCli` → `runCLI` or `executeCLI` | This is the public entry-point exported from `src/cli.js`. Tests, the bin shim, and acceptance fixtures all import `runCli` by that exact name. Renaming touches ~15 import sites for a casing preference. Recommend: keep `runCli` — JS convention is camelCase for functions even when acronyms appear. | DECLINE — convention call |
| 16 | `"admin"` const | YES. Add to a new `src/constants/auth-roles.js`: `export const ROLE_ADMIN = "admin"; export const ROLE_USER = "user";`. | `src/constants/auth-roles.js` + reader in cli.js |
| 17 | `VERSION = "0.34.0"` read from package.json | YES — package.json is the source of truth. The current literal exists because the legacy printVersion takes a `version` arg; commander's `.version()` accepts the same. Recommend: read at runtime via `fs.readFileSync('package.json')` (no top-level await needed), OR use `import pkg from "../package.json" assert {type: "json"}` (Node 22+). `make sync-version` handles the file already. | `src/cli.js` |
| 18 | `make sync-version` target | EXISTS — `make sync-version` propagates VERSION → `build.zig.zon` + `zombiectl/package.json` + `zombiectl/src/cli.js`. Verify it still works after the cli.js rewrite (line numbers shifted). | `make/build.mk` (verify) |
| 19 | `"user"` (commander argv source) const | Commander 14's API: `program.parseAsync(argv, {from: "user" \| "node" \| "electron"})`. `"user"` means "argv is already stripped of node + script paths". Recommend: name it locally: `const ARGV_FROM_USER = "user";` at top of cli.js. Low value. | OPTIONAL — `src/cli.js` |
| 20 | "autonomous agent platform" tagline — verify against website + README | Need to check `~/Projects/docs/usezombie.com/` (or wherever the marketing site is) + `README.md` for the canonical tagline. The CLI tagline must match the marketing voice. | Investigate first; update if it drifted |
| (UX) | `--cursor TOKEN` (uppercase) vs `<token>` (angle brackets) | Captain's directive: keep `<token>` / `<n>` convention if explained to user. Commander's auto-generated help uses these everywhere. Step 7b (below) verifies the explanation surface (help body, README, error messages). | Already aligned in commander tree + test shim |

**Build the response as a markdown table in the PR body's "Session notes"** when CHORE(close) lands.

### Step 7 — Amend M65_002 spec Verification Evidence

`docs/v2/done/M65_002_…md` — append a "Commander refactor" subsection covering:
- Parser swap (parseFlags → commander 14 + validators.js)
- Help-output shape change (printHelp deleted, commander.Help + ZombieHelp subclass owns rendering)
- The test-shim approach in helpers.js (with the rationale that direct-handler tests still verify leaf behavior)
- The `[--cursor <token>]` metavar convention adopted across the tree
- Captain's 20-item review outcomes (link to the tabulated table in the PR body)

Mark Discovery rows:
- #10 `printHelp(jsonMode)` JSON help → mark resolved; structured JSON help is what `--help --json` emits today (commander default output is parseable as text; structured JSON tree is a future enhancement if needed).
- #14 / #15 — handler signature consistency closes the inline-validation scatter (every option now goes through `validators.js`).

### Step 7b — E2E coverage for `--limit` / `--cursor` / option metavars (NEW from Captain)

After Step 7, before Step 8. Add acceptance coverage proving every option that takes a value:
1. Is documented with the consistent `<metavar>` convention in `zombiectl <cmd> --help`.
2. Rejects invalid input via the validator with a clear error (`--limit 0` → "must be ≥ 1").
3. Flows through to the handler in a real CLI invocation.

Touch points: `zombiectl/test/acceptance/options-metavar.spec.js` (new file). Cover at minimum:
- `zombiectl list --limit 25` / `--cursor abc123`
- `zombiectl logs --limit 50`
- `zombiectl events --limit 100 --since 2h --actor "steer:*"`
- `zombiectl install --from <existing-path>`
- `zombiectl login --timeout-sec 5 --poll-ms 100`
- `zombiectl billing show --limit 5 --cursor xyz`
- `zombiectl agent add --workspace <id> --zombie <id> --name <s>`
- `zombiectl tenant provider add --credential <name> --model <override>`

Each option must round-trip end-to-end (commander parses, validator validates, handler observes).

Captain's exact words: *"as long as we explain to the user we are fine. and those options must be tested end to end and work"*.

### Step 8 — CHORE(close) + skill chain

Required outputs at CHORE(close):
1. Move `docs/v2/active/M65_002_…md` → `docs/v2/done/M65_002_…md` (or update existing — already in done/ per pre-session-3 state).
2. Append new `<Update>` block to `~/Projects/docs/changelog.mdx` covering the commander refactor.
3. PR `## Session notes` with:
   - All decisions, assumptions, dead ends
   - `/write-unit-test` outcome
   - `/review` outcome
   - `kishore-babysit-prs` final report
   - **Captain's 20-item review table** (verbatim with each item's fix-location resolution)
4. `git rm HANDOFF.md`
5. `make check-version` passes
6. Orphan sweep complete (RULE ORP)

Skill chain (mandatory order):
1. `/write-unit-test` — audit diff coverage
2. `/review` — adversarial diff review
3. `git push` (already happened)
4. `/review-pr` — greptile triage
5. `kishore-babysit-prs` — poll greptile, fix P0/P1

---

## Hard constraints (carry forward)

- **350L cap** stays in force. Current high-water marks: `cli-tree.js` at 345 (watch this one), `cli.js` at 232.
- **RULE NLR + NLG**: no parallel paths landed; no legacy framing in any new code.
- **RULE TST-NAM**: no milestone IDs / § markers in test file source (the combined audit catches this).
- **gitleaks + lint + harness-verify** stay clean before every commit (pre-commit hook enforces).
- **Test shims in helpers.js** are not "legacy retained" — they're test-only adapters that let direct-handler unit tests target leaf primitives without rewriting 30 call sites. Document this clearly in the spec amendment so a future reviewer doesn't flag them as NLR violations.
- **External commitments** (sibling docs PR for changelog) need a paired PR at Step 8.

---

## Open UX question for Captain (resolved)

Q: `--cursor TOKEN` vs `--cursor <token>` — which convention?
A (Captain, May 12, 2026): *"as long as we explain to the user we are fine. and those options must be tested end to end and work (track as a step before Step 8 a new step after Step 7)"*.

Resolution: angle-bracket convention adopted across the tree. Step 7b verifies the explanation surface. The single legacy `[--cursor TOKEN]` reference in the test shim's usage message has been updated to `[--cursor <token>]`.

---

## Files to NOT touch in this session (handoff scope)

- `~/Projects/docs/` — sibling PR territory. Wait for Step 8.
- `.github/workflows/auth-e2e-{dev,prod}.yml` — already configured, not in the commander-refactor scope.
- Any sibling worktree — operate inside `~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e/`.

---

## Cross-agent note

This is a Claude → Claude handoff. The previous session (session 2) note about cross-worktree paths is resolved; session 3 stayed inside the worktree throughout. The next session should also stay inside.

Delete this file at the end of CHORE(close): `git rm HANDOFF.md` in the final commit.
