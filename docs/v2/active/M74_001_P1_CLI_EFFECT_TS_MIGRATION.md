<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M74_001: Effect-TS migration across `zombiectl`

**Prototype:** v2.0.0
**Milestone:** M74
**Workstream:** 001
**Date:** May 17, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — substrate for M74_002 (CLI auth handshake hardening) and a foundation for every subsequent CLI command. Captain-mandated as the runtime model for all CLI commands going forward.
**Categories:** CLI
**Batch:** B1
**Branch:** feat/m74-001-effect-ts-migration
**Depends on:** None.
**Provenance:** human-written (Kishore decision, May 17, 2026) — surfaced from M74 HANDOFF discussion. Reference: [`supabase/cli`'s `apps/cli/src/next/commands/login/login.handler.ts`](https://github.com/supabase/cli/blob/main/apps/cli/src/next/commands/login/login.handler.ts) (Effect-TS shape we are adopting).

**Canonical architecture:** N/A — substrate migration; the resulting runtime model becomes canon for every future CLI spec.

---

## Implementing agent — read these first

1. [`supabase/cli`'s `apps/cli/src/next/commands/login/login.handler.ts`](https://github.com/supabase/cli/blob/main/apps/cli/src/next/commands/login/login.handler.ts) — 228L Supabase CLI login handler. The reference shape: typed error channels via `Effect.fail`, composable side-effects via `Effect.gen`, runtime via `Effect.runPromise`. The patterns transfer one-for-one; the runtime choice (Effect-TS) is what M74_001 lands.
2. `zombiectl/src/commands/*.{js,ts}` — every command file. Each becomes an `Effect`-returning function with a typed error union.
3. `zombiectl/src/program/io.ts`, `zombiectl/src/program/auth-guard.ts`, `zombiectl/src/program/auth-token.ts` — existing helper layer. Effect-TS adoption needs to plug into (or replace) these.
4. `zombiectl/src/lib/run-command.ts` — current dispatcher wrapper. Migrates to an `Effect.runPromiseExit`-based dispatcher with structured error reporting.
5. Effect-TS docs at https://effect.website — specifically the **Getting Started** + **Error Management** + **Testing** chapters. The library is the runtime; this spec ports the patterns, not invents them.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal discipline. **RULE NLR** (touch-it-fix-it) and **RULE NLG** (no new legacy framing pre-`2.0.0`) both fire: the migration must NOT leave a `legacy_` / `V2` shim; commands either run on Effect or stay on the pre-migration path until cut over, never both.
- `docs/REST_API_DESIGN_GUIDELINES.md` — N/A (no HTTP server surface touched).
- `docs/ZIG_RULES.md`, `docs/SCHEMA_CONVENTIONS.md` — N/A.
- TypeScript: every Effect must carry a discriminated-union error type, never `unknown` / `Error`.

---

## Overview

**Goal (testable):** Every command exposed by `zombiectl` runs through an Effect-TS Effect graph. `zombiectl/src/lib/run-command.ts` (or its successor) is the single dispatcher, takes an `Effect` and returns a process exit code derived from `Effect.runPromiseExit`. No command file imports `process.exit` directly; every error path is typed and rendered through one shared error-formatter. `bun test` and `bun run lint` are green; `zombiectl --help` and the existing happy-path commands behave identically from the user's perspective.

**Problem:** The current CLI is plain Node.js with ad-hoc `try` / `catch` per command, `process.exit(N)` peppered through handler bodies, and untyped `err.code` strings flowing into shell exit codes. The result is three friction points:

1. **No typed error channel.** A command can throw `any`; the dispatcher catches and prints whatever. Adding a new failure mode means remembering to update both the throw site and the formatter — drift is the default.
2. **Composability is manual.** Sequential side-effects (auth-check → HTTP call → format → write) thread through callbacks or promise chains; cancellation, retry, and structured-concurrency primitives are written per-command.
3. **Testing is hard.** Mocking `process.exit` and HTTP layers requires per-test scaffolding because there's no shared dispatcher entry point. The Supabase CLI reference solved this with Effect-TS; we are adopting the same runtime so M74_002's auth handler (the immediate consumer) can be implemented in the upstream-validated shape and every subsequent command inherits the substrate.

**Solution summary:** Migrate `zombiectl/` to Effect-TS for the command dispatch path. Add Effect as a dependency. Rewrite `run-command.ts` as an Effect dispatcher. Migrate commands one-by-one to `Effect`-returning handlers with typed error unions. Adopt Effect's `Layer` for shared services (HTTP client, vault, output writer) so handlers receive them via DI rather than module-side-effect imports. M74_002 (auth handshake hardening) lands on top of this substrate; future command specs follow the same shape by default.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/package.json` | EDIT | Add `effect@^3` dependency (latest stable). |
| `zombiectl/src/lib/run-command.ts` | REWRITE | Becomes the Effect dispatcher — receives an `Effect`, runs via `Effect.runPromiseExit`, owns exit-code translation + error formatting. |
| `zombiectl/src/lib/analytics.ts` | DELETE | Replaced by `services/analytics.ts`. Per-call-site `trackCliEvent` wiring deleted in the group commits that consumed it. |
| `zombiectl/src/program/handlers-bind.ts` | DELETE | Per-handler `runCommand(…)` binding goes away; cli-tree.ts wires Effects to the dispatcher directly. |
| `zombiectl/src/program/io.ts` | EDIT | Folded into `services/output.ts`; this file shrinks to a stream-write primitive or is deleted. |
| `zombiectl/src/program/auth-guard.ts` | EDIT | Becomes an Effect returning `AuthContext`; composes via `Effect.gen` instead of returning `{ok}`. |
| `zombiectl/src/program/auth-token.ts` | EDIT | Same pattern — Effect-returning. Token storage moves into `services/credentials.ts`. |
| `zombiectl/src/runtime/main-layer.ts` | CREATE | The `MainLayer` composition that the dispatcher provides at the runtime boundary. |
| `zombiectl/src/services/analytics.ts` | CREATE | `Analytics` service wrapping `posthog-node`. Owns `capture`/`alias`/`identify`; preserves the `cli_session_id`/`cli_device_id` props from M71_001. |
| `zombiectl/src/services/telemetry-runtime.ts` | CREATE | `TelemetryRuntime` service — `deviceId`, `sessionId` are read from a service, not threaded through `HandlerCtx`. |
| `zombiectl/src/services/output.ts` | CREATE | `Output` service — `intro`/`info`/`success`/`warn`/`error`/`promptText`/`promptConfirm`. Every emit carries audit metadata (`{command, …}`). Single audit-bearing surface. |
| `zombiectl/src/services/credentials.ts` | CREATE | `Credentials` service. Access token stored as `Redacted<string>` — accidental log/print = type error. |
| `zombiectl/src/services/crypto.ts` | CREATE | `Crypto` service — ECDH keypair, session-id generation. Substrate for M74_002 hardening. |
| `zombiectl/src/services/browser.ts` | CREATE | `Browser` service — `open(url)` effect. |
| `zombiectl/src/services/stdin.ts` | CREATE | `Stdin` service — `isTTY`, `readPipedText`. |
| `zombiectl/src/services/http-client.ts` | CREATE | `HttpClient` service replacing the direct `http.ts` call sites; carries `RetryConfig` + structured errors. |
| `zombiectl/src/services/config.ts` | CREATE | `CliConfig` service — `apiUrl`, `accessToken` (env), `dashboardUrl`. |
| `zombiectl/src/commands/auth.ts` | REWRITE | First group migrated (commit 1, paired with substrate per Q-A). `loginEffect`, `logoutEffect`, `authStatusEffect`. |
| `zombiectl/src/commands/*` | REWRITE | Every remaining command — `install`, `status`, `logs`, `events`, `steer`, `stop`, `resume`, `kill`, `delete`, `list`, `credential`, `workspace`, `agent`, `grant`, `billing`, `doctor`, `tenant provider`. One Effect per command; per-command `errorMap` deleted as its group migrates. |
| `zombiectl/src/program/cli-tree.ts` | EDIT | Commander's `.action()` receives the Effect from the command module and hands it to the dispatcher. |
| `zombiectl/src/errors/` | CREATE | Shared discriminated-union error types (`AuthError`, `NetworkError`, `ValidationError`, `ServerError`, `ConfigError`) as `Data.TaggedError` subclasses with `{detail, suggestion}`. |
| `zombiectl/test/**/*.test.ts` | EDIT | Tests rewritten to use `Effect.provide(testLayers)`; no `process.exit` stubs. |
| `zombiectl/CONTRIBUTING.md` | CREATE | Effect-TS conventions page: how to write a command, how to add an error variant, how to mock layers in tests, the no-`process.exit` rule. |

---

## Sections (implementation slices)

### §1 — Substrate: dispatcher + layers

`run-command.ts` becomes the Effect dispatcher. `MainLayer` (defined in `src/runtime/main-layer.ts`) composes — taken verbatim from Supabase's `next/` shape — `IoLayer` (folded into `Output`), `HttpClient`, `AuthToken`, `CliConfig`, `Analytics`, `TelemetryRuntime`, `Output`, `Credentials`, `Crypto`, `Browser`, `Stdin`. The dispatcher translates `Effect.runPromiseExit` results into process exit codes via one shared formatter that knows every error variant's preferred exit code + user-facing message. **No command file calls `process.exit` after this slice.**

**Why these layers (not just IO + HTTP):** the user-visible win — audit/support-friendly errors, telemetry that can't be forgotten, secrets that can't be accidentally logged — comes from structural DI of the side-effecting capabilities, not from the dispatcher alone. Skipping `Analytics`/`TelemetryRuntime`/`Output`/`Credentials`/`Crypto` reduces the migration to a mechanical refactor with no end-user benefit. The Supabase `login.handler.ts` reference shows the full set in 228 lines.

### §2 — Error taxonomy

`zombiectl/src/errors/` exports a closed set of discriminated-union error types. Every command's Effect signature names which subset it can fail with. The shared formatter switches on `_tag` to choose exit code + message; adding a new error variant fails the formatter at compile time until the case is handled.

**Implementation default:** Match the existing `UZ-<CAT>-<NNN>` taxonomy from the server side. Each server-defined code maps to one TypeScript error variant; the formatter renders the same user-facing string for both client-side failures (e.g. `NetworkError`) and server-side failures relayed from the API.

### §3 — First-group migration (auth — bundled into commit 1)

`zombiectl auth login / logout / auth status` migrate in **the same commit as §1's substrate** (Q-A decision). Rationale: the substrate must not exist without a working consumer — a substrate-only commit with zero callers is exactly the "two horses mid-stream" framing the rules prohibit. Commit 1 ships the dispatcher, the layer set, the error taxonomy scaffold (`AuthError` + the formatter), and the auth-group rewrite together.

**M74_002 sequencing (Q-B decision):** M74_001 lands first. M74_002's sibling worktree (`feat/m74-002-cli-browser-authorization-flow`) rebases onto this branch after merge, then writes its ECDH + verification-code hardening in Effect shape from day one. No auth code is ever written twice.

### §4 — Bulk migration (single PR, per-group commits, no shim)

The remaining command groups migrate as separate commits **on this same branch, in this same PR**. Each commit:

1. Rewrites that group's command bodies as Effect-returning handlers consuming the relevant services.
2. Updates `cli-tree.ts` to wire them to the Effect dispatcher.
3. **Deletes** the old code paths (per-command `errorMap`, manual `trackCliEvent` triplet wiring, `handlers-bind.ts` entries) that the migrated group no longer needs.
4. CI must be green at the commit's HEAD — no broken-build interstitial commits.

There is **no dual-acceptance dispatcher, no compat shim, no rename**. Between commits, un-migrated groups continue to route through their pre-existing helper code — that code is not renamed, not gated, and not framed as "legacy"; it is simply the code that has not yet been replaced. The **final commit** before PR-open deletes whatever remains of the pre-Effect dispatch surface (`lib/run-command.ts`'s pre-Effect body, the old `lib/analytics.ts`, `program/handlers-bind.ts`) so what merges to `main` is Effect-only — no transitional state on the integration branch, ever.

**Group order:** `auth` (commit 1, with §1) → `zombie` core (`install`, `status`, `logs`, `events`, `steer`, `stop`, `resume`, `kill`, `delete`, `list`) → `workspace` → `credential` → `agent` → `grant` → `billing` → `tenant provider` → `doctor`. Group-by-subcommand-surface (Supabase pattern) — each group shares a layer composition.

**Touch-it-fix-it sweep (RULE NLR):** as each file is migrated, the ~10 pre-existing `// legacy …` / `// shim …` comments in committed code (`commands/billing.ts`, `commands/core.ts`, `lib/analytics.ts`, `output/*.ts`, `program/cli-tree*.ts`) are removed in the same commit. No separate sweep.

### §5 — Test harness migration

Existing `bun test` files rewritten to use `Effect.provide(testLayers)` for layer mocking. The shared `testLayers` module provides an in-memory IO, a mocked HTTP client, a fake auth-token store, and a fixture config. Every test becomes "compose the command's Effect with `testLayers`, run, assert on the resulting Exit." No more `process.exit` stubs; no more module-level mocking.

### §6 — Contributor docs

A short Effect-TS conventions page (1-2 pages) lands in `zombiectl/README.md` (or a new `zombiectl/CONTRIBUTING.md`): how to write a command, how to add an error variant, how to mock layers in tests, when to use `Effect.gen` vs `Effect.pipe`, the rule that no command file imports `process.exit`.

---

## Interfaces

No HTTP / OpenAPI / wire surface added or changed — internal CLI runtime only. The contracts the migration locks in:

```typescript
// zombiectl/src/lib/run-command.ts (post-migration)
export const runCommand: <E extends CliError>(effect: Effect.Effect<void, E, MainLayer>) => Promise<never>;

// zombiectl/src/errors/index.ts
export type CliError =
  | AuthError       // unauthorized / token expired / forbidden
  | NetworkError    // timeout / DNS / TLS
  | ServerError     // 5xx from API, UZ-<CAT>-NNN preserved
  | ValidationError // bad input / missing arg
  | ConfigError;    // local config drift

// Pattern every command follows
export const installCommand = (args: InstallArgs): Effect.Effect<void, CliError, MainLayer> => Effect.gen(function* () {
  const auth   = yield* AuthGuard;
  const http   = yield* HttpClient;
  const result = yield* http.post('/v1/.../zombies', { ... });
  yield* Output.print(result);
});
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Dispatcher cannot map an error variant to an exit code | A new error variant lands without updating the shared formatter switch. | Compile-time: the formatter's `switch (err._tag)` is exhaustive; missing case → TS2367. CI gate catches it before merge. |
| Layer not provided | A command needs `VaultLayer` but the test composition omits it. | Compile-time: Effect's `R` parameter unifies; missing layer surfaces as `Effect<…, …, MissingLayer>` and the test won't compile. |
| Un-migrated command outlives its group's commit | An author adds a new command using the pre-Effect helper after that helper was supposed to be deleted. | Lint rule + grep gate in `make lint`: no command file may `import { runCommand }` from `lib/run-command.ts`'s pre-Effect entry, and no command file may call `process.exit`. CI fails. |
| Effect adds substantial bundle size | Bun-bundled CLI binary grows. | Measure before/after with `du`; if growth is unacceptable, narrow imports to tree-shakable subpaths (`effect/Effect`, `effect/Layer`, etc.) rather than the umbrella import. |
| Async stack traces become harder to read | Effect's runtime adds frames. | Use Effect's built-in `Effect.withSpan` for operation labels; surface in `--debug` mode. Document in CONTRIBUTING. |

---

## Invariants

1. **No `process.exit` in command files.** Enforced by a lint rule + grep in `make lint`. Only the dispatcher calls `process.exit`.
2. **Every command's Effect carries a typed error union.** Enforced by TypeScript — `Effect<void, unknown, …>` is rejected at type-check time via a custom strictness rule (or a wrapper type).
3. **The dispatcher's error formatter is exhaustive.** Enforced by TypeScript's exhaustiveness check on the `switch (err._tag)`.
4. **No pre-Effect dispatch surface survives PR-open.** Enforced by RULE NLG — the old `runCommand` body, `lib/analytics.ts`, and `program/handlers-bind.ts` are deleted in the final commit on this branch. There is no transitional state on `main` — what merges is Effect-only. Within-branch dev state may carry un-migrated groups; what ships does not.
5. **No "legacy"/"compat"/"shim"/"deprecated" framing anywhere in committed `src/`.** Enforced by Eval E5 (`grep -wi`) — zero matches required at PR-open. Pre-existing comment hits are swept under RULE NLR as their file is touched.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_dispatcher_exit_code_per_error` | Every variant in `CliError` maps to a documented exit code; dispatcher returns that code when the Effect fails with that variant. |
| `test_no_process_exit_in_commands` | `grep -rn 'process\.exit' zombiectl/src/commands/` returns zero matches after migration. |
| `test_command_layer_composition` | Per command: composing the command's Effect with `testLayers` succeeds (compile-time + runtime). |
| `test_auth_status_happy_path` | `zombiectl auth status` against a fixture HTTP layer returns the expected formatted output and exit code 0 (regression for D31 from M68 across the migration). |
| `test_auth_status_error_paths` | Each documented `auth status` failure (UZ-AUTH-001 / UZ-AUTH-002 / TOKEN_EXPIRED) routes to the correct exit code and message after migration. |
| `test_dispatcher_unknown_error_compile_fails` | A synthetic test that adds a new error variant without updating the formatter MUST fail to compile. |
| `test_telemetry_session_device_props_preserved` | Every `cli_*` analytics event captured through `Analytics` carries `cli_session_id` + `cli_device_id` (M71_001 regression). |
| `test_credentials_redacted_never_logged` | A test that calls `Output.success` after `Credentials.getAccessToken` MUST NOT leak the token string into captured stderr/stdout (Redacted enforcement). |
| `test_legacy_framing_grep` | `grep -rn -wi 'legacy\|compat\|shim\|deprecated' zombiectl/src/` returns zero matches (Eval E5 as a test gate). |

---

## Acceptance Criteria

- [ ] `bun test` green in `zombiectl/`.
- [ ] `bun run lint` green; the custom no-`process.exit` rule is wired into `oxlint` config or the existing `audit-*.mjs` script chain.
- [ ] `grep -rn 'process\.exit' zombiectl/src/commands/` returns zero matches.
- [ ] `grep -rnE 'Effect\.Effect<[^,]*,[[:space:]]*unknown' zombiectl/src/` returns zero matches.
- [ ] `grep -rn -wi 'legacy\|compat\|shim\|deprecated' zombiectl/src/` returns zero matches (Eval E5).
- [ ] `zombiectl --help` output unchanged (regression).
- [ ] Every command listed in `cli/zombiectl.mdx` works against the integration fixture.
- [ ] `zombiectl/CONTRIBUTING.md` exists and contains the Effect-TS conventions page.
- [ ] No file in `zombiectl/src/` over 350 lines as a result of the migration.
- [ ] M71_001's `cli_session_id` + `cli_device_id` analytics props remain present on every `cli_*` event (served from `TelemetryRuntime`).
- [ ] CI green at **every commit** on the integration branch (no broken-build interstitials).

---

## Eval Commands (Post-Implementation Verification)

```bash
cd zombiectl

# E1: no process.exit in commands
grep -rn 'process\.exit' src/commands/ && echo FAIL || echo PASS

# E2: no untyped Effect signatures
grep -rnE 'Effect\.Effect<[^,]*,[[:space:]]*unknown' src/ && echo FAIL || echo PASS

# E3: tests + lint
bun test && bun run lint

# E4: help output regression
diff <(zombiectl --help) tests/golden/help.txt

# E5: no legacy/compat/shim/deprecated framing
grep -rn -wi 'legacy\|compat\|shim\|deprecated' src/ && echo FAIL || echo PASS
```

---

## Dead Code Sweep

Each group commit deletes the old code paths it replaces — no separate cleanup commit, no rename, no shim. Before PR-open the final commit on the branch must have produced these zero-match states:

| Deleted surface | Grep | Expected |
|-----------------|------|----------|
| Pre-Effect `runCommand` body / `HandlerCtx` shape | `grep -rn 'HandlerCtx\|RunCommandDeps' zombiectl/src/` | Zero matches |
| `lib/analytics.ts` (replaced by `services/analytics.ts`) | `git ls-files zombiectl/src/lib/analytics.ts` | Empty |
| `program/handlers-bind.ts` (replaced by direct Effect wiring in `cli-tree.ts`) | `git ls-files zombiectl/src/program/handlers-bind.ts` | Empty |
| Per-command `errorMap` exports | `grep -rn 'export.*errorMap\|export.*ErrorMap' zombiectl/src/commands/` | Zero matches |
| Per-command `try { … } catch (e) { process.exit(…) }` blocks | `grep -rn 'process\.exit' zombiectl/src/commands/` | Zero matches |
| "legacy"/"compat"/"shim"/"deprecated" framing in committed code | `grep -rn -wi 'legacy\|compat\|shim\|deprecated' zombiectl/src/` | Zero matches |

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After each migration PR, before CHORE(close) | `/write-unit-test` | Audits layer-mock coverage on the migrated commands. | Clean. |
| After migration complete, before CHORE(close) | `/review` | Adversarial pass: every command returns a typed Effect, dispatcher is the only exit-code surface, no pre-Effect dispatch surface survives, no legacy framing in committed code. | Findings dispositioned. |
| After each `gh pr create` | `/review-pr` | Catches Effect signatures with `unknown` or `any` error channels. | Comments addressed before human review. |

---

## Discovery (consult log)

- **May 17, 2026 — Spec audit vs Supabase `next/` reference.** Original §1 layer list (`IoLayer`, `HttpClientLayer`, `AuthTokenLayer`, `ConfigLayer`) omitted the user-visible-value layers — `Analytics`, `TelemetryRuntime`, `Output` (audit-bearing), `Credentials` (`Redacted` secrets), `Crypto`, `Browser`, `Stdin`. Without them the migration reduces to a mechanical refactor with no end-user benefit. Added in this amendment.
- **May 17, 2026 — RULE NLG sweep in spec prose.** Original §4 used "thin shim" / "compatibility shim" / "legacy command bodies" / "runCommandLegacy" — all of which violate `[[feedback_pre_v2_api_drift]]` for a pre-2.0 codebase. Rewritten to: single PR, per-group commits, each commit deletes the code it replaces, no shim, no rename, no transitional state on `main`. Eval E5 added to enforce zero `grep -wi legacy|compat|shim|deprecated` matches at PR-open.
- **May 17, 2026 — Q5 (tsconfig strictness) is already met.** `zombiectl/tsconfig.json` currently has `strict: true`, `noUncheckedIndexedAccess: true`, `exactOptionalPropertyTypes: true`, `useUnknownInCatchVariables: true`. No uplift required; Effect composes on top.
- **May 17, 2026 — M71_001 analytics regression risk noted.** `cli_session_id` + `cli_device_id` props were added recently (commit 97705654). Migration must preserve them — they now flow from `TelemetryRuntime` service, not `HandlerCtx`. Added as Acceptance Criterion + Test Spec implication.
- **May 17, 2026 — handlers-bind.ts identified as deletion target.** Its sole purpose is wiring leaf handlers through `runCommand`; the Effect dispatcher consumes Effects directly from each command module, making this file obsolete. Marked DELETE in Files Changed.

---

## Open Questions — RESOLVED (Captain decisions, May 17, 2026)

- **Q1 — Effect-TS major version:** Latest stable `effect@^3` (caret range; bump on minor/patch).
- **Q2 — Layer split:** Follow Supabase's `next/` shape verbatim. Per-handler `yield* Service` for what the handler needs; `MainLayer` is the runtime-boundary composition only. Services are atomic; TypeScript tracks each handler's `R`.
- **Q3 — PR cadence:** **One PR for M74_001.** Multiple commits within the PR — one per subcommand group. CI green at every commit; no broken-build interstitials.
- **Q4 — Codegen of error taxonomy:** Hand-maintained for v1. M73_001 may later codegen the union from the Zig registry without breaking the discriminated-union shape.
- **Q5 — TypeScript strictness:** No-op. Current `tsconfig.json` already at the target strictness (`strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `useUnknownInCatchVariables`). Effect composes on top.
- **Q-A — Commit 1 content:** Substrate (§1) + error taxonomy (§2) + auth group (§3) ship together. The substrate is born with a working consumer; no dead-substrate commit.
- **Q-B — M74_002 sequencing:** M74_001 lands first. The sibling worktree (`feat/m74-002-cli-browser-authorization-flow`) rebases onto the merged substrate and writes its ECDH + verification-code hardening in Effect shape from day one — no auth code written twice.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| no process.exit (E1) | see Eval | | |
| no untyped Effect (E2) | see Eval | | |
| tests + lint (E3) | `bun test && bun run lint` | | |
| help regression (E4) | `diff` | | |
| no legacy framing (E5) | `grep -wi 'legacy\|compat\|shim\|deprecated' src/` | | |

---

## Out of Scope

- **Server-side runtime changes.** Zig handlers stay as-is; the migration is CLI-only.
- **`ui/packages/app/` Effect-TS adoption.** Dashboard is React + Next; Effect-TS in the browser is a separate decision.
- **Effect-TS for the executor sandbox.** Sandbox is Zig; out of scope.
- **`mintlify` docs generation from Effect signatures.** Docs stay hand-maintained; auto-generating command reference from type signatures is a follow-up.
- **`fp-ts` (the predecessor library).** Not adopted; Effect-TS is the chosen runtime.
- **The auth-handshake-hardening work.** Sibling workstream M74_002 ships on top of this substrate.
