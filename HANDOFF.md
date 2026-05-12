# Handoff — M65_002 commander refactor (next session)

**Date:** May 12, 2026 (updated session 2 — same day)
**Outgoing agent (session 2):** Cross-worktree starter — Step 1 committed locally,
  Step 2 staged uncommitted, test audit folded into TEST CHURN tables
**Incoming agent:** zombiectl-to-commander refactor — picks up at Step 2 commit
**State:** M65_002 implementation pushed; commander refactor partially started
  (commander dep committed locally, validators.js + tests written but uncommitted)

---

## Where we are

- **Worktree:** `~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e`
- **Branch:** `chore/m65-002-spec-zombiectl-e2e-lifecycle` (8 commits ahead of `main`)
- **PR:** [#323](https://github.com/usezombie/usezombie/pull/323) — retitled to
  `feat(zombiectl): M65_002 — zombiectl e2e full lifecycle scenarios + suite`
- **Docs sibling PR:** `chore/m65-002-zombiectl-cli-e2e-changelog` on
  `usezombie/docs` (changelog block pushed; no PR opened yet).

### Already-landed (do NOT redo)

| Commit | What |
|---|---|
| `e9381a16` | CHORE(close) — spec → done/, AUTH.md CLI carve-out |
| `e43db310` | `cli-acceptance-{dev,prod}` workflow jobs (Captain-approved) |
| `76c408d5` | §4 + §5 acceptance specs (lifecycle-with-token, lifecycle-after-login, browser.js) |
| `7a6332b1` | uuidv7 validator swap + ID-handler hardening + UFS constants (cli-errors/actions/flags) |
| `8f58e281` | flags-and-env spec + SIGINT handler (pre-existing) |
| `894edbe5` | help-and-errors spec + CLI carve-outs (pre-existing) |
| `4ca719ea` | scaffold acceptance harness (pre-existing) |
| `19a7c607` | CHORE(open) — spec promoted to active/ (pre-existing) |

**Greens (as of session 1 close):** 24 acceptance + 592 unit / 2 skip / 0 fail
/ 927 expect calls / lint clean / gitleaks clean / all files ≤ 350L.

---

## Progress so far (session 2 — May 12, 2026)

A cross-worktree session began Step 1 + 2 of the refactor before the Captain
redirected work back to its own worktree. Net state in
`~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e`:

### Committed (local only, NOT pushed)

| Commit | Step | What |
|---|---|---|
| `e52fe886` | Step 1 | `chore(zombiectl): add commander@^14.0.3 dependency` — pure dep add, two-file diff (`zombiectl/package.json` + `zombiectl/bun.lock`). Zero callsites. Lint + gitleaks green. Pre-commit hooks (oxlint, OpenAPI bundle/lint/url-shape) all passed. |

### Uncommitted (staged for Step 2 commit)

| File | State | Coverage |
|---|---|---|
| `zombiectl/src/program/validators.js` | new, ~120 lines | 8 parsers: `parseStringOption` (direct), `parseIntOption({min,max})` (factory), `parseFloatOption` (direct), `parseIdOption` (direct, uuidv7), `parseEnumOption(allowed)` (factory), `parsePathOption({mustExist})` (factory), `parseDurationOption` (direct, ms/s/m/h → ms), `parseJsonObjectOption({maxBytes})` (factory). Mirrors oracle's surface. |
| `zombiectl/test/validators.unit.test.js` | new, 51 tests | One `describe` per parser. Happy + multiple negative paths each. `bun test test/validators.unit.test.js` → 51 pass / 0 fail / 65 expect calls / 96.88% line coverage. Uncovered lines are defensive guards (`Number.isFinite` after `parseInt(regex-validated)`). |

### Design notes (apply when committing Step 2)

- **Direct vs factory split deviates from the handoff template lines 200-287.**
  The template had `parseIntOption(value, name, opts)` as a unified 3-arg form.
  The implemented version uses factory pattern for configurable parsers
  (`parseIntOption({min,max})(value)`) to match commander's
  `.option(flags, description, fn)` callback signature naturally — same
  pattern oracle uses. Rationale: lets you write
  `.option("--n <n>", "...", parseIntOption({min:1, max:3600}))` directly
  instead of wrapping in a closure at every option site. Note this in the
  Step 5 atomic-swap PR body.
- **Integer parsing is stricter than the handoff template** — guards against
  `parseInt("42abc", 10)` silently truncating. Uses regex pre-validation
  (`/^-?\d+$/`) before `Number.parseInt`. Same fix in `parseFloatOption`.
- **`validate.js::validateRequiredId` was NOT modified** (yet). The handoff
  says "Keep `validateRequiredId` in `validate.js` (delegates here)" — that
  delegation hasn't happened. Defer to the Step 5 atomic swap so the
  existing `validate.test.js` doesn't need a message-shape update mid-flight.

### 🚧 Blocker discovered: test runner regression

**`bun run test` is failing on ~50 test files** with
`ERR_UNSUPPORTED_ESM_URL_SCHEME: Only URLs with a scheme in: file, data, and
node are supported by the default ESM loader. Received protocol 'bun:'`.

- The error is `node --test` trying to load a file with `import … from "bun:test"`.
- Affected: `banner.unit.test.js`, `cli-alignment.unit.test.js`, `help.test.js`,
  and many more — all files that import from `bun:test`.
- Root cause appears to be in `zombiectl/scripts/run-tests.mjs` — the
  categorization between node tests and bun tests has bit-rotted, OR something
  about the local Node/Bun versions diverged from what was green at session-1
  close.
- **Not caused by Step 1 or Step 2 changes** — confirmed by running validators
  tests directly via `bun test test/validators.unit.test.js` (clean, 51 pass).
  The dep add commit `e52fe886` only touched `package.json` + `bun.lock`.
- **Action for next session:** investigate `run-tests.mjs` categorization
  BEFORE continuing with Step 5 atomic swap. The handoff's MUST PASS bucket
  is meaningless if `bun run test` can't run end-to-end. Likely a one-line
  fix in `run-tests.mjs`, but verify before assuming.

### Cross-worktree note

Session 2 ran from the main `usezombie` worktree and reached into M65_002 via
absolute paths — a deviation from AGENTS.md "stay inside [worktree]" that the
Captain accepted because the next-session execution started in this thread.
The redirect "operate in your worktree please" arrived after Step 2 file
writes; this handoff update is the only mutation since. The next agent
should start a fresh session **inside** `~/Projects/usezombie-m65-002-spec-zombiectl-cli-e2e/`.

---

## What the next session does

**Captain's call (May 12, 2026):** switch `zombiectl` from the hand-rolled
`parseFlags` to `commander`, all-in-one, in the same PR (#323). Comprehensive
validator surface. Help-output question (commander default vs `printHelp`
override) is **still pending Captain inspection** — they asked to see a
mock first; see `/tmp/commander-preview/preview.mjs` for the comparison
artifact (also reproduced in the handoff message).

### Decisions captured from this session

| Question | Captain's answer |
|---|---|
| Where does the refactor live? | **Same PR #323** (extend M65_002) |
| Coverage? | **All-in-one** — every `zombiectl <cmd>` moves to commander in one shot. Only exception: help rendering, which routes through the `ZombieHelp` subclass (commander.Help + `styleTitle` → `formatHelpHeading`). Legacy / unused / dead files (`args.js`, `routes.js`, `command-registry.js`, `suggest.js`, `printHelp` + its helpers in `io.js`) deleted in the same diff per RULE NLR + RULE NDC. |
| Help/version shape? | Commander default tree, **preserving the existing color scheme** — subclass `commander.Help` and override the `styleTitle(title)` virtual hook to call `formatHelpHeading(title)` from `src/output/index.js`. Wire via `program.configureHelp({ helpFactory: () => new ZombieHelp() })`. The tagline (`palette.subtle("autonomous agent platform")`) lands via `program.description()` — either pre-style the description string or override `styleDescriptionText` if commander renders it through a separate hook. NO_COLOR / isTty / ColorMode preserved automatically (both helpers accept `{stream, env}` opts). Do NOT override `formatHelp(cmd, helper)` wholesale — `styleTitle` is the idiomatic commander 14 hook and keeps the diff to ~10 lines. Do NOT introduce picocolors, do NOT invent a new palette, do NOT pull in the full `ui` proxy (no glyphs in help). Goal: bold pulse cyan section headers + dim tagline preserved byte-for-byte vs current `zombiectl --help`. The `/tmp/commander-preview/preview-color.mjs` mock is a *shape* reference only — ignore its picocolors palette. |
| Validators surface? | **Comprehensive** — `parseStringOption` (required+trim), `parseIntOption(min,max)`, `parseFloatOption`, `parseIdOption` (uuidv7), `parseEnumOption(allowed)`, `parsePathOption({mustExist})`, `parseDurationOption` (`30m`/`10s`/`500ms`/`2h`), `parseJsonObjectOption({maxBytes})`. Mirrors oracle's surface. |
| Runtime target? | **Both Node and Bun** — `zombiectl/package.json:engines` already accepts both. Customers get Node via `npm i -g`; dev tooling uses Bun. Commander runs identically on both; nothing else changes. |

### Read first (before touching code)

1. `/tmp/commander-preview/preview.mjs` — already mocks the full zombiectl
   command tree under commander 14. `bun preview.mjs --help` /
   `bun preview.mjs workspace --help` / etc. reproduce the help-output
   comparison Captain inspected. Re-run if needed.
2. `~/Projects/oracle/src/cli/options.ts` — the validator pattern Captain
   referenced. `InvalidArgumentError` throws from parsers; commander
   catches and renders the option name + message + exit 2.
3. `zombiectl/src/cli.js` — the current dispatch path (parseGlobalArgs →
   findRoute → registerProgramCommands → runCommand). Commander replaces
   the middle three.
4. `zombiectl/src/program/{routes,args,command-registry,suggest}.js` —
   the four files commander replaces wholesale.
5. `zombiectl/test/acceptance/help-and-errors.spec.js` and
   `flags-and-env.spec.js` — the spec tests that break on commander
   adoption. Specifically the byte-identity help triplet and the
   `--version --help → --version wins` precedence test.

---

## Concrete file-by-file work

### NEW

| File | What |
|---|---|
| `zombiectl/src/program/validators.js` | Comprehensive parser surface. Each function takes `(value, name?)` and either returns the typed value or throws `InvalidArgumentError`. Oracle's `~/Projects/oracle/src/cli/options.ts:75-175` is the reference shape. Keep `validateRequiredId` in `validate.js` (delegates here) so existing callers don't break. |
| `zombiectl/src/program/cli-tree.js` | Single source of truth: builds the commander `Command` tree. One function `buildProgram(handlers, deps): Command`. Action callbacks call into handlers. This is where commander wins us the most — every command, option, arg, validator, description lives here in one declaration block, no hand-rolled dispatch. |

### REWRITE

| File | What |
|---|---|
| `zombiectl/src/cli.js` | Becomes much smaller. Loads creds/workspaces, builds ctx, builds the program from `cli-tree.js`, calls `program.parseAsync(argv)`. Keeps the analytics + pre-action hook (auth-guard, NO_COLOR, pre-release banner) but delegates dispatch. **The `printHelp(stdout, ui, …)` call at line 70 goes away** — commander short-circuits on `--help`/`-h` and invokes the configured `commander.Help` subclass automatically. ~80-100 lines instead of 301. |
| `zombiectl/src/program/io.js` | Remove `printHelp` (and its dead-weight helpers `HELP_NAME_WIDTH` + `helpRow`). Keep `printJson`, `writeError`, `writeLine` — handlers still call those. The commander.Help subclass (defined in `cli-tree.js` or a sibling file like `src/program/help.js`) takes over help rendering, importing `formatHelpHeading` + `palette.subtle` from `src/output/index.js` directly. |
| Each `src/commands/*.js` | Handler signature changes from `(ctx, args, workspaces, deps)` to `(ctx, opts, workspaces, deps)` — commander already parsed the options. No more `parseFlags(args)` inside handlers; no more `parsed.positionals[0]`. Each command file edited. |

### DELETE

Captain's directive: **all zombiectl commands move to commander; every
legacy / unused file goes in the same diff.** No "keep if customisation
needed" hedges — RULE NLR (touch-it-fix-it) + RULE NDC (no dead code at
write time) require unused exports to leave with the code that orphans
them.

| File | Why |
|---|---|
| `zombiectl/src/program/args.js` | `parseFlags` + `parseGlobalArgs` — commander owns option/positional parsing. `normalizeApiUrl` is the only non-commander concern; move it to `zombiectl/src/util/url.js` (or similar) and import where needed. |
| `zombiectl/src/program/routes.js` | `findRoute` + the routes array — commander owns dispatch. The shape it enforced ("every documented command resolves to a handler") moves into `cli-tree.js` and is verified by `cli-tree.unit.test.js`. |
| `zombiectl/src/program/command-registry.js` | `registerProgramCommands` — superseded by `buildProgram(handlers, deps)` in `cli-tree.js`. |
| `zombiectl/src/program/suggest.js` | Commander has built-in did-you-mean (`error: unknown command 'pogo' (Did you mean logs?)`). **Delete unconditionally** — if commander's wording diverges from current output, update the assertion in `help-and-errors.spec.js` to match commander, not the other way around. |
| `zombiectl/src/program/io.js::printHelp` + `helpRow` + `HELP_NAME_WIDTH` | Hand-rolled help renderer — replaced by `ZombieHelp`. Keep the rest of `io.js` (`printJson`, `writeError`, `writeLine`) — handlers still call those. |

**Post-refactor dead-code sweep (mandatory before COMMIT):**

```bash
# 1. Unused exports across the package
bunx knip --workspace zombiectl  # or: bun x ts-prune (whichever the repo already uses)

# 2. Orphan-import sweep — any `import { X } from "./gone-file.js"` left behind
rg -n "from [\"'].*\\b(args|routes|command-registry|suggest)\\.js[\"']" zombiectl/src zombiectl/test

# 3. Symbol-reference sweep for the deleted internals
for sym in parseFlags parseGlobalArgs findRoute registerProgramCommands printHelp; do
  rg -nw "$sym" zombiectl/ && echo "FAIL: $sym still referenced"
done

# All three must come back clean before opening the PR.
```

Anything those sweeps surface gets deleted in the same diff. RULE ORP
(orphan symbol = 0 hits across the codebase) is non-negotiable.

**Why `printHelp` deletes (not just unused):** keeping it alongside the
commander.Help subclass means two sources of truth for help content —
the hand-rolled command list in `printHelp` and the commander tree in
`cli-tree.js`. They drift the moment someone adds a subcommand. RULE
NLR (touch-it-fix-it) and RULE NDC (no dead code at write time) both
require the unused exports to go in the same diff that introduces the
commander.Help replacement.

### TEST CHURN — file-by-file audit (session 2)

Every file in `zombiectl/test/` (62 files) classified into one of five
buckets. The classification went file-by-file, reading each file's
imports + leading docs + assertion pattern. Net counts:

| Bucket | Count | Action |
|---|---|---|
| 🔴 DELETE | 6 | Tests for code that goes away (parseFlags / routes registry / hand-rolled dispatch fall-throughs / printHelp coverage) |
| 🟢 NEW | 3 | One per new module (validators / cli-tree / help) |
| 🛡 MUST PASS | 4 | User-facing contract — non-negotiable, adapt assertions to commander's wording but suite stays green |
| 🟡 UPDATE | ~20 | Handler signature sweep + commander-wording adaptation |
| ✅ UNTOUCHED | ~25 | Tests for src layers commander doesn't touch (output, analytics, sse, http retry, banner, validate, …) |
| 🟠 CONSOLIDATE | 2→1 | `browser-resolve-platforms` + `browser.unit.test` overlap — merge |

#### 🔴 DELETE (Bucket 1) — 6 files

| File | Why |
|---|---|
| `test/args.unit.test.js` | Tests `parseFlags` + `parseGlobalArgs` (both gone) + `normalizeApiUrl` (which migrates to `util/url.js`). Migrate `normalizeApiUrl` cases to `util/url.unit.test.js` first, then delete this file. |
| `test/cli-dispatch-sweep.unit.test.js` | Sweeps `cli.js`'s `handlers` registry — exercises every route arrow. The "every route resolvable" shape is **structurally enforced** by commander once `cli-tree.js` exists. |
| `test/command-dispatchers-unknown-action.unit.test.js` | Asserts hand-rolled "unknown action" fall-through in each `commands/<area>.js`. Commander emits `error: unknown command 'X'` natively — covered by `help-and-errors.spec.js` instead. |
| `test/help-coverage.unit.test.js` | Regression guard that every registered command appears in `printHelp` output. The drift class it protects goes away — commander's help auto-generates from `cli-tree.js`, can't drift. Imports `printHelp` + `registerProgramCommands` (both deleted). |
| `test/parse.test.js` | Tests `parseGlobalArgs` from `cli.js` (re-export). `parseGlobalArgs` is deleted. |
| `test/registry.unit.test.js` | Pins `registerProgramCommands` invariants (`{name, handler, errorMap}`). `command-registry.js` is deleted entirely. Replaced by `cli-tree.unit.test.js`. |

#### 🟢 NEW (Bucket 2) — 3 files

| Module | Test | Status |
|---|---|---|
| `src/program/validators.js` | `test/validators.unit.test.js` | ✅ **already written this session** — 51 tests, all passing in isolation, awaiting Step 2 commit |
| `src/program/cli-tree.js` | `test/cli-tree.unit.test.js` | pending — assert tree shape: every documented `zombiectl <cmd>` resolves, every option has the expected validator wired, `preAction` auth-guard fires before non-public commands |
| `src/program/help.js` (`ZombieHelp`) | `test/help.unit.test.js` | pending — `styleTitle("USAGE")` returns bold-pulse-cyan ANSI under xterm256 / basic16 cyan / plain under NO_COLOR; snapshot help body for `--help` + one subcommand `--help` |

#### 🛡 MUST PASS (Bucket 3) — 4 suites, user-facing contract

If any of these fail post-refactor, the refactor doesn't ship.

| Suite | Why it must pass |
|---|---|
| `test/acceptance/help-and-errors.spec.js` | "Every `zombiectl <cmd> --help` shows usage; unknown commands suggest." Adapt to commander wording, drop byte-identity, keep substring/shape checks. Auth-guard tests move to assert commander's `preAction` hook fires. |
| `test/acceptance/flags-and-env.spec.js` | Global flags (`--api`, `--json`, `--no-input`, `--no-open`) + env vars (`ZOMBIE_API_URL`, `ZOMBIE_TOKEN`, `ZOMBIE_API_KEY`, `ZOMBIE_STATE_DIR`, `NO_COLOR`) must drive behavior identically. `--version --help` precedence: pick commander's default OR override — lock it in a test. `--help --json` → either custom-implement JSON help or drop the test; document choice in PR. |
| `test/onboarding-flow.integration.test.js` | Mocks HTTP not the parser → likely unaffected, but must stay end-to-end green. |
| `.github/workflows/auth-e2e-{dev,prod}.yml` (real Clerk OTP) | The whole point of M65_002. Commander swap touches `auth login`'s argument parsing path; E2E must still drive `zombiectl auth login` → mailinator OTP → workspace add to green. **Non-negotiable.** |

Plus the two new commander-related acceptance suites already in the
worktree:

- `test/acceptance/lifecycle-after-login.spec.js` (§5)
- `test/acceptance/lifecycle-with-token.spec.js` (§4)

Both must stay green — they exercise full lifecycle commands.

#### 🟡 UPDATE (Bucket 4) — ~20 files, mechanical sweep

| File | Update |
|---|---|
| `test/cli-alignment.unit.test.js` | `runCli` integration; handler signature `(ctx, args, …)` → `(ctx, opts, …)`. |
| `test/cli-analytics.unit.test.js` | Analytics events emit via `runCli`; the `user_authenticated` / `workspace_created` events fire from commander's `postAction` hook now, not inline. |
| `test/did-you-mean.integration.test.js` | Adapt assertions to commander's built-in did-you-mean wording (drop `suggestCommand` import). |
| `test/help.test.js` | User-facing help via `runCli`; adapt assertions to commander's exact help body wording. |
| `test/json-contract.test.js` | Imports `findRoute` + `registerProgramCommands` (both deleted); swap iteration source to `program.commands` tree. JSON-mode behavior assertions stay. |
| `test/golden-output.unit.test.js` + `test/golden/*.txt` | Regenerate golden fixtures via `makeGolden` helper after the atomic swap. Document regen in commit message. BUN_RULES §8 carve-out is preserved. |
| `test/api-url-resolution.integration.test.js` | `runCli` integration — verify the `--api` / `ZOMBIE_API_URL` / `creds.api_url` precedence still holds (logic moves to commander option default + preAction). |
| `test/credentials.integration.test.js`, `test/grant.integration.test.js`, `test/agent.integration.test.js`, `test/logs.integration.test.js` | Each exercises `runCli` end-to-end for an area. Handler signature sweep + adapt to commander error wording. |
| `test/login.unit.test.js`, `test/logout.unit.test.js`, `test/billing.unit.test.js`, `test/workspace.unit.test.js`, `test/workspace-add.test.js`, `test/workspace-helpers.unit.test.js`, `test/tenant_provider.unit.test.js`, `test/zombie.unit.test.js`, `test/zombie-install-from-path.unit.test.js`, `test/zombie-steer-fallback.unit.test.js`, `test/zombie_events_pagination.unit.test.js`, `test/zombie_steer_batch_roundtrip.unit.test.js` | Direct command-handler tests. Each gets a mechanical edit: pass `opts` object (commander-parsed) instead of raw `args` array. No behavior change. |
| `test/doctor-json.test.js`, `test/suggest.test.js` | `doctor-json` — adapt to commander's `--json` flag wiring. `suggest.test` — likely deletes if it only tested `suggestCommand`; check before swap. |

#### ✅ UNTOUCHED — ~25 files, no commander coupling

Tests for layers commander never touches. Run them post-swap to confirm
no regression, but they need no edits.

| Category | Files |
|---|---|
| Output system | `output-capability.unit.test.js`, `output-format.unit.test.js`, `output-glyph.unit.test.js`, `output-invariants.unit.test.js`, `output-palette.unit.test.js` — five files, one per `src/output/*` module + the cross-cutting invariants test. |
| Analytics | `analytics.unit.test.js` — pure `cliAnalyticsInternals.resolveConfig` logic. |
| Banner | `banner.unit.test.js` — `printVersion` + `printPreReleaseWarning` (called from cli.js, stays). T1-T11 coverage matrix; well-built. |
| Constants pin tests | `contact.unit.test.js` (SUPPORT_EMAIL), `error-codes.unit.test.js` (UZ-* constants). Cross-runtime pins per UFS GATE carve-out. |
| Error / failure layers | `error-matrix.unit.test.js` (runCommand wrapper matrix), `failure-modes.integration.test.js` (real UZ-* responses E2E). Three layers (constants/wrapper/integration), not redundant. |
| Retry / HTTP | `http-retry.unit.test.js` (primitive `apiRequestWithRetry`), `request-retry.unit.test.js` (propagation through `request(ctx, path)`). Two layers, not redundant. |
| run-command | `run-command.unit.test.js` — wrapper stays. |
| SSE | `sse.unit.test.js` (single-frame `parseSseFrame`), `sse-parser.unit.test.js` (multi-event `parseSseBuffer`), `sse-streamget.unit.test.js` (full `streamGet` lifecycle). Three surfaces, not redundant. |
| State / streamfetch / ui-progress | `state.unit.test.js`, `streamfetch.unit.test.js`, `ui-progress.unit.test.js` — utility layers, unaffected. |
| ID validation | `validate.test.js` — `isValidId` + `validateRequiredId`. Stays unless `validateRequiredId` is delegated to `parseIdOption` (deferred per design note above). |
| Coverage filler | `coverage-fill.unit.test.js` — odd-but-needed line-coverage filler. Verify still works post-swap. |
| Auth | `auth-guard.test.js` (logic), `auth-token.unit.test.js` (JWT parse). Both stay. |
| Test scaffolding | `helpers.js`, `helpers-cli-state.js`, `helpers-fs.js`, `helpers-mock-api.js` — not tests, shared utilities; all stay. |

#### 🟠 CONSOLIDATE — housekeeping

| Files | Action |
|---|---|
| `browser-resolve-platforms.unit.test.js` + `browser.unit.test.js` | Both test `resolveBrowserCommand`. Different *cases* (platform paths vs `BROWSER=false/0`), different assertion styles (`expect` vs `assert`). Merge into one `browser.unit.test.js` using `expect`. Not commander-driven; do whenever convenient. |

#### Definition of done for tests

All five buckets resolved:

1. 🔴 Six DELETE files removed in Step 5 atomic swap.
2. 🟢 Three NEW files committed (validators ✅ done, cli-tree + help pending).
3. 🛡 MUST PASS suites green (incl. real Clerk OTP via auth-e2e workflows).
4. 🟡 UPDATE sweep complete — every direct handler call passes `opts` not `args`.
5. ✅ UNTOUCHED files still green (regression check, no edits).

Plus: `bun run test` end-to-end (after the run-tests.mjs blocker is
investigated and fixed) + `auth-e2e-{dev,prod}` workflows green on the
PR. No bucket may be skipped.

### Acceptance spec amendments

| Spec | Amendment |
|---|---|
| `docs/v2/done/M65_002_…md` Verification Evidence | Append a new "Commander refactor" subsection documenting the parser swap + the help/version shape change. |
| `docs/v2/done/M65_002_…md` Discovery row #10 | Mark resolved (`printHelp(jsonMode)` JSON body lands as part of this refactor IF Captain wants JSON help; otherwise the row stays open). |
| `docs/v2/done/M65_002_…md` Discovery row #14 / #15 | Mark partially resolved — handler signature consistency closes the inline-validation scatter; the constants modules stay relevant. |

---

## Validators module shape (proposed)

```js
// zombiectl/src/program/validators.js
import { InvalidArgumentError } from "commander";
import path from "node:path";
import fs from "node:fs";
import { validate as isValidUuid, version as uuidVersion } from "uuid";

const EXAMPLE_UUIDV7 = "0192a3b4-c5d6-7e8f-9012-345678901234";

export function parseStringOption(value) {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new InvalidArgumentError("must be a non-empty string");
  }
  return value.trim();
}

export function parseIntOption(value, name = "value", { min, max } = {}) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed)) throw new InvalidArgumentError(`${name} must be an integer`);
  if (min !== undefined && parsed < min) throw new InvalidArgumentError(`${name} must be ≥ ${min}`);
  if (max !== undefined && parsed > max) throw new InvalidArgumentError(`${name} must be ≤ ${max}`);
  return parsed;
}

export function parseFloatOption(value) {
  const parsed = Number.parseFloat(value);
  if (!Number.isFinite(parsed)) throw new InvalidArgumentError("must be a number");
  return parsed;
}

export function parseIdOption(value) {
  if (typeof value !== "string" || value.length === 0) {
    throw new InvalidArgumentError("required");
  }
  if (!isValidUuid(value) || uuidVersion(value) !== 7) {
    throw new InvalidArgumentError(`expected uuidv7 format (e.g. ${EXAMPLE_UUIDV7})`);
  }
  return value;
}

export function parseEnumOption(value, allowed) {
  if (!allowed.includes(value)) {
    throw new InvalidArgumentError(`must be one of: ${allowed.join(", ")}`);
  }
  return value;
}

export function parsePathOption(value, { mustExist = false } = {}) {
  if (typeof value !== "string" || value.length === 0) {
    throw new InvalidArgumentError("required");
  }
  const resolved = path.resolve(value);
  if (mustExist && !fs.existsSync(resolved)) {
    throw new InvalidArgumentError(`path does not exist: ${value}`);
  }
  return resolved;
}

// 30m / 10s / 500ms / 2h → milliseconds. Mirrors oracle's parseDuration.
const DURATION_RE = /^(\d+)(ms|s|m|h)$/;
const DURATION_FACTOR = { ms: 1, s: 1000, m: 60_000, h: 3_600_000 };

export function parseDurationOption(value) {
  const match = DURATION_RE.exec(String(value ?? "").trim());
  if (!match) throw new InvalidArgumentError("expected a duration like 30m, 10s, 500ms, or 2h");
  const n = Number.parseInt(match[1], 10);
  if (n <= 0) throw new InvalidArgumentError("duration must be positive");
  return n * DURATION_FACTOR[match[2]];
}

export function parseJsonObjectOption(value, { maxBytes = 4096 } = {}) {
  if (Buffer.byteLength(value, "utf8") > maxBytes) {
    throw new InvalidArgumentError(`payload must be ≤ ${maxBytes} bytes`);
  }
  let parsed;
  try {
    parsed = JSON.parse(value);
  } catch {
    throw new InvalidArgumentError("must be valid JSON");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new InvalidArgumentError("must be a JSON object (not array or primitive)");
  }
  return parsed;
}
```

---

## Hard constraints (carry forward)

- **Length cap ≤ 350L per file** stays in force. `cli-tree.js` will be the largest new file — keep an eye on it; may need to factor by domain (`cli-tree-zombie.js`, `cli-tree-workspace.js`, etc.) before it crosses.
- **RULE NLG**: no parallel paths. Once commander is in, `parseFlags` is gone same-commit. No `if (env.ZOMBIECTL_USE_COMMANDER) {...} else {...}` flag.
- **Tests must stay green**. The accepted breakage is the help-output byte-identity test and the `--version --help` precedence test. Everything else must pass.
- **Gitleaks + lint + harness-verify** stay clean before commit.
- **`--version --json` and `--help --json`** are referenced in `flags-and-env.spec.js`. Commander has no native JSON help — either custom-implement via a `--json` global flag + action hook that prints a structured JSON tree, OR drop those assertions. Captain prefers the implementation; mention in commit message.
- **Auth guard** (`requireAuth` short-circuit in `cli.js`) moves to a commander `preAction` hook installed once at the program level.
- **Analytics** (`cliAnalytics`) wraps commander's `parseAsync` call — install/shutdown around it.
- **`postinstall.mjs`** doesn't change.

## Hard merge-gate (inherited, still applies)

- `op://VAULT_{DEV,PROD}/e2e-fixtures/{regular,admin}/email` must resolve
  to non-mailinator domains AND the workflow `env:` blocks must consume
  them. Sibling M65_001 provisions the vault. Implementation merge waits
  for those to land.

## Help-output shape — commander tree, existing colors preserved

Captain's directive: **maintain the existing color scheme**. The
commander refactor changes the dispatch path and help *layout* (default
commander tree instead of the hand-rolled `printHelp`), but the visible
colors of `zombiectl --help` and `zombiectl <cmd> --help` must match
current output. No new palette, no new color dep.

**Implementation — minimum surface (~10 lines).** Two helpers from
`src/output/index.js` carry the entire existing color surface:

- `formatHelpHeading(s, {stream, env})` → bold pulse cyan (xterm256
  79, basic16 cyan, NO_COLOR plain). Used by today's section headers
  (USAGE, USER COMMANDS, AGENT COMMANDS, GRANT COMMANDS, …).
- `palette.subtle(s, {stream, env})` → grey (xterm256 244, basic16
  dim, NO_COLOR plain). Used by today's tagline only — `dim("autonomous
  agent platform")`. Command names and descriptions in current
  `printHelp` are plain text via `helpRow` (no palette wrap), so they
  stay plain post-refactor.

Use commander 14's style hooks, NOT a full `formatHelp` override:

```js
import { Help } from "commander";
import { formatHelpHeading, palette } from "../output/index.js";

class ZombieHelp extends Help {
  styleTitle(title) { return formatHelpHeading(title); }
  // If commander renders program.description() through a separate
  // hook (e.g. styleDescriptionText), override it to wrap with
  // palette.subtle. Otherwise pre-style the description string
  // when calling program.description(palette.subtle("...")).
}

program.configureHelp({ helpFactory: () => new ZombieHelp() });
```

Both helpers resolve capability (`NO_COLOR`, `isTty`, `ColorMode`)
per call — no global wiring needed, no per-test setup beyond what
already exists.

**Verification:** snapshot `zombiectl --help` and one subcommand help
(`zombiectl workspace list --help` or similar) pre- and post-refactor.
Tokens to verify match: bold pulse cyan section headers, dim grey
tagline, plain command/option names + descriptions. Re-run with
`NO_COLOR=1` to confirm the strip path produces identical plain output.

Do NOT pull in the broader `ui` proxy (`ui.ok` / `ui.warn` / glyphs,
etc.) — help output has no glyphs today; adding them would *change*
the scheme rather than preserve it.

**Do NOT:**
- Add `picocolors` (or any new color dep) as a runtime dep.
- Use the `/tmp/commander-preview/preview-color.mjs` palette as a
  reference — it was a *shape* demo only. Its picocolors choices
  (bold yellow section headers, cyan command names, green option
  names) are not the existing scheme and should not land.

**Verification before COMMIT:** run `zombiectl --help` and a
representative `zombiectl <cmd> --help` (e.g. `zombiectl auth login --help`)
pre- and post-refactor, eyeball that color tokens are unchanged. Also
re-run with `NO_COLOR=1` to confirm the strip path still produces
identical plain output.

---

## Skill chain (run after refactor lands)

1. `/write-unit-test` — audit diff coverage. Expect heavy churn.
2. `/review` — adversarial diff review before commit. Commander semantics
   shifts (help shape, exit codes, error rendering) are where regressions
   hide.
3. `gh pr update` — refresh the PR body with the commander section.
4. `/review-pr` — greptile triage after push.
5. `kishore-babysit-prs` — poll after every push.

---

## Delete this file at the end of the refactor

`HANDOFF.md` is ephemeral. `git rm HANDOFF.md` as part of the final
CHORE(close) commit. Don't ship it.
