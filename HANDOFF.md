# HANDOFF — M68_001 §14 TS migration · D42+ continuation

**Branch:** `feat/m68-trigger-dx-and-free-trial` · **+14 commits ahead of main, -54 behind** · no PR yet
**Active spec:** `docs/v2/active/M68_001_P1_API_CLI_UI_DOCS_WEBSITE_TRIGGER_REGISTRATION_AND_FREE_TRIAL.md` · Status: IN_PROGRESS
**Worktree:** `~/Projects/usezombie-m68-trigger-dx/` (cwd usually `zombiectl/`)
**Tip commit:** `761d8e19 chore(ts-migration): §14 D41 — cli.ts + drag-ins + type-system unification`
**VERSION:** `0.35.0` (no bump this session; D43 will likely cut 0.36.0)

---

## Session summary — May 16, 2026

Landed **D40 + dedup + D41** in three commits. The D41 wave forced a broader-than-expected type-system unification (Workspaces / Credentials / CredentialFile), captured Captain-spotted during execution.

### Commits added this session

| SHA | Title |
|---|---|
| `c38adebb` | §14 D40 — src/program/** migration (7 files + new types module) |
| `e321657f` | merge validate.ts into validators.ts — one module, two flavors |
| `761d8e19` | §14 D41 — cli.ts + drag-ins + type-system unification |

### Cumulative §14 state

| D | Files | State |
|---|---|---|
| D33 | tsconfig + cross-package alignment | DONE |
| D34 | constants/** (10 files) | DONE |
| D35 | util/* + output/* (6 files) | DONE |
| D36 | program/auth-token + lib/state | DONE |
| D37 | lib/http + lib/sse + program/http-client | DONE |
| D38 | lib/error-map-presets + lib/run-command | DONE |
| D39 | src/commands/** (14 files) | DONE |
| D40 | src/program/** (7 files + cli-tree-types.ts) | DONE |
| **D41** | src/cli.ts + lib/browser.ts + ui-progress.ts + Workspaces/Credentials unification | **DONE this session** |
| D42 | test/** (~79 files) | PENDING — **next** |
| D43 | Build pipeline: tsc emit to dist/, package.json bin/files, spawner → node | PENDING |

---

## Carry-forward risks (READ FIRST)

Active spec `docs/v2/active/M68_001_*.md` § "§14 Risks & Gotchas (carry into D38–D43)" at line ~585. **Read it before doing D42 work.** Items still load-bearing for D42/D43:

1. **`bun test` picks up `*.spec.*`** — 5 acceptance specs run on every `bun test` (~7s overhead). Stub-mode: 51 pass + 2 skip. Deliberate consolidation; don't unwind.
2. **`analytics.d.ts` still has 1 remaining drift item** — the `CliAnalyticsContext` interface (lines 12–16) is the only fictional shape left after D38/D40/D41 swept the function declarations. It's currently unused by typed code; will dissolve when `analytics.js` itself migrates.
3. **`src/lib/http.ts` at 327/350 lines (23L FLL headroom)** — D42 won't touch it, but if any test migration drags helpers into http.ts, split into sibling.
4. **No re-export shims** — Captain enforced this through D37 (streamFetch), D39 (zombie_install split), D40 (cli-tree-types extraction). Same rule for D42 if any test helper exceeds the cap.
5. **Bun-vs-Node spawner until D43** — `test/acceptance/fixtures/cli.js` spawns `bun` because the source tree mixes `.js` and `.ts`. **Don't migrate `fixtures/cli.js → .ts`** during D42 — the spawner file is consumed by Node-spec orchestration, and the spawner switches back to `node` only after D43 emits `dist/`. D41's sed batch over-reached on this; reverted in a second pass. Watch for the same trap in D42.
6. **`bin/zombiectl.js` stays `.js`** — published Node entry. D43 replaces with `dist/bin/zombiectl.js`. Do NOT migrate to `.ts` during D42. Adjusted `package.json build` script + `make/quality.mk:_zombiectl_lint` to drop the now-stale `node --check ./src/cli.js`.

---

## D41 type-system changes you'll inherit in D42

**Single `Workspaces` type now** — `lib/state.ts` is the source of truth. `commands/types.ts` re-exports `Workspaces` + `WorkspaceItem` + `Credentials`. Tests that imported `Workspaces` from `commands/types.ts` are still valid (transparent re-export). Tests that constructed fake workspaces with the fictional `current_workspace_label?` or `workspaces?` fields will fail typecheck — those fields are GONE. Real shape:

```ts
interface Workspaces { current_workspace_id: string | null; items: WorkspaceItem[] }
interface WorkspaceItem { workspace_id: string; name: string | null; created_at: number | null }
```

**`CredentialFile` is retired.** Use `Credentials` directly. `auth.ts:192` lost its `as CredentialFile | null` cast; tests fabricating credential objects should match the real shape (`{token, saved_at, session_id, api_url}`, all `string|null` or `number|null`).

**`CommandCtx.apiUrl` is required.** Tests that built a partial `CommandCtx` without `apiUrl` will fail typecheck.

**`Lifecycle.distinctId: string | null`** (was `string`). `handlers-bind.ts` translates `null → undefined` at the run-command seam. Tests asserting on analytics events for unauthenticated runs expect the `"anonymous"` fallback — the run-command path applies it via `deps.distinctId ?? handlerCtx.distinctId ?? "anonymous"`. Watch for tests that pre-coerce distinctId; they may now hit the fallback path differently.

**`SpinnerOptions`/`SpinnerHandle` widened.** `SpinnerOptions.style?: string` is new; `SpinnerHandle.succeed`/`fail` are now required (were optional `?`) and accept `message?: string`. Tests mocking SpinnerHandle need to declare succeed/fail.

**`analytics.d.ts` fixed:**
- `drainCliAnalyticsEvents(ctx: unknown) → QueuedAnalyticsEvent[]` (was wrongly declared as `(client) → Promise<void>`).
- `trackCliEvent.distinctId` param is `string | null | undefined`.
- `cliAnalytics` is the namespace object (not a function).

---

## Next up: D42 — `test/**` migration

**~79 test files in `test/`** (count from `find test -name '*.js' | wc -l`). All currently `.js` using `bun:test` + `node:assert/strict`. The migration recipe:

1. `git mv test/<file>.js test/<file>.ts` — preserves history.
2. Add types where strict mode requires:
   - Function signatures (most tests are local helpers).
   - Catch blocks: `err` is `unknown` under `useUnknownInCatchVariables`. Narrow before `.message`.
   - Mock/fake constructors should match the now-tightened types (see D41 changes above).
3. `bun run typecheck` → fix what surfaces.
4. `bun run lint` → fix oxlint + audit-runtime-imports + tsc errors.
5. `bun test` → expect **684 pass + 2 skip + 0 fail** (pre-D42 baseline; rename + types is behaviorally neutral).
6. Commit per logical cluster, NOT all at once (79 files is too many for one commit — split into batches like `D42a — test/output-*`, `D42b — test/cli-*`, etc., or per-domain).

### Batching suggestion for D42

The 79 test files break down roughly by domain:
- **Acceptance** (`test/acceptance/`) — spec files + fixtures. **Skip `fixtures/cli.js`** (carry-forward risk #5; spawner stays .js until D43). 5 spec files + 4 fixture files (minus cli.js) ≈ 8.
- **Integration** (`test/*integration*.test.js`) — ~10 files.
- **Unit** (`test/*.unit.test.js`) — ~30 files.
- **Other** (`test/*.test.js` without unit/integration suffix) — ~30 files.

5 batches of ~15 files each, OR 3 batches by domain (acceptance + integration + unit). Captain bias from D38/D39/D40 commits: one commit per logical wave.

### Files that should NOT migrate in D42

- `test/acceptance/fixtures/cli.js` — Node-spec spawner; stays `.js` until D43.

### Likely traps in D42

- **Mock object shapes**: tests build fake `lifecycle`, `workspaces`, `ctx`, `deps` — many need updates after D41's type tightening. Pre-existing test mocks will need real shapes (e.g. `Workspaces.items` is required; `Credentials.api_url` is required).
- **Test helpers that import `WorkspaceListEntry` / `WorkspacesWithItems` / `CredentialFile`** — those types are gone from workspace.ts and commands/types.ts. Update imports.
- **`Lifecycle.distinctId` widening** may flip some analytics-event-assertion tests if they expect a specific string instead of the "anonymous" fallback.
- **`SpinnerHandle.succeed`/`fail` are now required** — mocks that omit them will fail typecheck.

---

## Path to PR after D42

1. **D43** — Build pipeline:
   - `tsc --outDir dist/` (config probably exists; needs `rewriteRelativeImportExtensions: true` already enabled per D33)
   - Update `package.json` `bin: dist/bin/zombiectl.js` (or wherever the compiled entry lands)
   - Update `package.json` `files` to include `dist/` (and exclude raw `src/`)
   - Switch `test/acceptance/fixtures/cli.js` spawner from `bun` back to `node` against compiled output
   - Narrow `scripts/audit-runtime-imports.mjs` back to `.js`-only walk
   - Delete `src/lib/analytics.d.ts` AFTER migrating `analytics.js → analytics.ts` (which is a separate concern — see "deferred" below)
2. **§13 polish** (parallel-safe, independent from §14) — D22 (TTL countdown in login), D23 (workspace hydration fail-loud), D28 (login error taxonomy), D29 (exp-backoff polling with jitter), D30 (polling transient-retry). All touch `commandLogin` in `src/commands/core.ts`.
3. **CHORE(close):**
   - Sync feature branch with main first ([feedback_sync_main_before_pr]) — currently 54 commits behind
   - Move spec `docs/v2/active/` → `docs/v2/done/`, Status `DONE`
   - Add `<Update>` in `~/Projects/docs/changelog.mdx` (cross-repo edit; use the own-branch flow per Operational defaults)
   - Run `/write-unit-test` audit + `/review` against the full diff
   - Bump VERSION to `0.36.0` if D43 changed the published artifact
   - `gh pr create` with `## Summary` + `## Test plan` + Session Notes
   - Run `/review-pr` against the PR
   - `kishore-babysit-prs` polls greptile per cadence
   - Delete stale `HANDOFF*.md` files at repo root before PR open (CHORE(close) discipline — these are ephemeral)

---

## Deferred items (separate small commits or follow-up wave)

Spotted during D41 audit but out of scope:

- **`src/lib/analytics.js → .ts`** — explicitly deferred per HANDOFF. After D41 only the `CliAnalyticsContext` interface in `analytics.d.ts` is still a phantom. Migrating analytics.js will delete the .d.ts.
- **`src/lib/sse-parser.js`** (50L) — has **zero production callers**; only `test/sse-parser.unit.test.js` imports it. Likely dead code (lib/sse.ts has its own `parseSseFrame`). RULE NDC candidate — delete or migrate.
- **`src/lib/contact.js`** (8L) — exports `SUPPORT_EMAIL`. Test-only inside zombiectl, but it's a cross-runtime parity anchor (matches src/config/contact.zig, ui/packages/website/, app/, ~/Projects/docs/snippets/). Trivial 1-line migration.

Total remaining `.js` in src/ + bin/ after D41:
- `bin/zombiectl.js` (stays .js until D43)
- `src/lib/analytics.js` (deferred)
- `src/lib/sse-parser.js` (deletion candidate)
- `src/lib/contact.js` (parity anchor)

---

## Verification commands

From `zombiectl/`:
- `bun run typecheck` — `tsc --noEmit`. Should be clean.
- `bun run lint` — oxlint + `audit-runtime-imports.mjs` + tsc. Should be 0 warnings + 0 errors across 141 files.
- `bun test` — full suite. Baseline: **684 pass + 2 skip + 0 fail** across 62 files. Acceptance specs add ~7s in stub mode.
- `make test` from repo root — runs Zig tests + zombiectl tests.

Pre-commit hook runs gitleaks + HARNESS VERIFY (UFS, DESIGN TOKEN, SPEC TEMPLATE, ERROR REGISTRY, LOGGING, LIFECYCLE, COMBINED audits) + all-package lint + openapi bundle. Expect **22 pre-existing openapi warnings** (svix ambiguous paths + missing 4XX responses — not §14 related, ignore).

---

## Working tree / state hygiene

- Working tree clean as of `761d8e19`.
- Untracked at repo root (NOT in zombiectl/): `HANDOFF*.md` (3 stale from prior sessions + this one), `docs/{CHANGELOG_VOICE,EXECUTE_DOC_READS,HARNESS_VERIFY_OUTPUT,VERIFY_TIERS}.md` (dotfiles symlinks freshly relinked May 14). CHORE(close) deletes the HANDOFFs before PR.
- No tmux sessions.

---

## Things NOT to do (footguns)

- **Don't migrate `bin/zombiectl.js` to `.ts`** — published Node entry; D43 replaces it. (Risk #6 above.)
- **Don't migrate `test/acceptance/fixtures/cli.js`** — Node-spec spawner; D43 switches it back to node-against-compiled. (Risk #5.)
- **Don't migrate `src/lib/analytics.js`** standalone in D42 — its .d.ts has 1 remaining drift; migrating dissolves it cleanly with the right wave. Defer.
- **Don't use `as any` / `@ts-expect-error`** to silence strict-mode errors. (`feedback_ts_migration_intent`.)
- **Don't add re-export shims** to bridge file splits (risk #4).
- **Don't add code to `src/lib/http.ts`** (23L FLL headroom; risk #3).
- **Don't merge a half-typed file.** If typecheck has any errors, fix in same commit. D33–D41 maintained this invariant.
- **Don't `node --check` on `.ts` files.** `package.json build` + `make/quality.mk:_zombiectl_lint` were adjusted in D41 — don't add back the old `node --check ./src/cli.js` pattern.

---

## Spec source of truth

When in doubt, the active spec wins:

```
docs/v2/active/M68_001_P1_API_CLI_UI_DOCS_WEBSITE_TRIGGER_REGISTRATION_AND_FREE_TRIAL.md
```

The §14 section (line ~528+) has the full D-wave table, the carry-forward Risks block (line ~585), and per-D Discovery prose for D33–D41. D41's Discovery (~line 588) captures the type-system unification narrative — read before touching command/test code.
