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
**Status:** PENDING
**Priority:** P1 — substrate for M74_002 (CLI auth handshake hardening) and a foundation for every subsequent CLI command. Captain-mandated as the runtime model for all CLI commands going forward.
**Categories:** CLI
**Batch:** B1
**Branch:** feat/m74-001-effect-ts-migration (to be created on CHORE(open))
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
| `zombiectl/package.json` | EDIT | Add `effect` dependency. Bump TS target if needed. |
| `zombiectl/src/lib/run-command.ts` | REWRITE | Becomes the Effect dispatcher — receives an `Effect`, runs to exit code, owns error formatting. |
| `zombiectl/src/program/io.ts` | EDIT | Lifted to an Effect `Layer` so handlers access stdout/stderr/prompts via DI. |
| `zombiectl/src/program/auth-guard.ts` | EDIT | Returns an `Effect<AuthContext, AuthError, AuthLayer>`; composes via `Effect.gen` instead of throwing. |
| `zombiectl/src/program/auth-token.ts` | EDIT | Same pattern — Effect-returning. |
| `zombiectl/src/commands/auth.js` (or .ts) | REWRITE | First command migrated, paired with M74_002. Becomes `loginEffect`, `logoutEffect`, `authStatusEffect`. |
| `zombiectl/src/commands/*` | REWRITE | Every remaining command — `install`, `status`, `logs`, `events`, `steer`, `stop`, `resume`, `kill`, `delete`, `list`, `credential`, `workspace`, `agent`, `grant`, `billing`, `doctor`, `tenant provider`. One Effect per command. |
| `zombiectl/src/program/cli-tree.ts` | EDIT | Commander's `.action()` wiring receives the Effect from the command module and hands it to the dispatcher. |
| `zombiectl/src/errors/` | CREATE | Shared discriminated-union error types (`AuthError`, `NetworkError`, `ValidationError`, `ServerError`, …). Each command's error union is a subset. |
| `zombiectl/test/**/*.test.ts` | EDIT | Tests rewritten to use `Effect.provide` for layer mocking; no more `process.exit` stubs. |
| `zombiectl/README.md` (or `CONTRIBUTING.md` if it exists) | EDIT | One-page Effect-TS conventions: how to write a command, how to add an error variant, how to mock layers in tests. |

---

## Sections (implementation slices)

### §1 — Substrate: dispatcher + layers

`run-command.ts` becomes the Effect dispatcher. Defines the `MainLayer` composed of `IoLayer`, `HttpClientLayer`, `AuthTokenLayer`, `ConfigLayer`. Translates `Effect.runPromiseExit` results into process exit codes via one shared formatter that knows every error variant's preferred exit code + user-facing message. No command file calls `process.exit` after this slice.

### §2 — Error taxonomy

`zombiectl/src/errors/` exports a closed set of discriminated-union error types. Every command's Effect signature names which subset it can fail with. The shared formatter switches on `_tag` to choose exit code + message; adding a new error variant fails the formatter at compile time until the case is handled.

**Implementation default:** Match the existing `UZ-<CAT>-<NNN>` taxonomy from the server side. Each server-defined code maps to one TypeScript error variant; the formatter renders the same user-facing string for both client-side failures (e.g. `NetworkError`) and server-side failures relayed from the API.

### §3 — First-command migration (paired with M74_002)

`zombiectl auth login / logout / auth status` migrate first because M74_002 lands the ECDH + verification-code hardening on the same code paths. The pair ships together: M74_001 provides the substrate; M74_002 provides the security work that exercises it.

### §4 — Bulk migration

Remaining commands migrate in alphabetical groups. Each PR migrates one group (e.g. `install` + `status` + `logs`; or `workspace` subcommands as a group). The dispatcher accepts both Effect-returning and legacy command bodies during the migration window via a thin shim; the shim is deleted in the last migration PR.

**Implementation default:** Group by subcommand surface, not alphabetically — e.g. all `workspace ${verb}` commands migrate together so their shared layer composition is reused, not duplicated.

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
| Mid-migration legacy command bypasses the dispatcher | An author copies an old command and forgets to wrap. | Lint rule (custom ESLint or grep in `make lint`): no command file may call `process.exit`; CI gate fails. |
| Effect adds substantial bundle size | Bun-bundled CLI binary grows. | Measure before/after with `du`; if growth is unacceptable, narrow imports to tree-shakable subpaths (`effect/Effect`, `effect/Layer`, etc.) rather than the umbrella import. |
| Async stack traces become harder to read | Effect's runtime adds frames. | Use Effect's built-in `Effect.withSpan` for operation labels; surface in `--debug` mode. Document in CONTRIBUTING. |

---

## Invariants

1. **No `process.exit` in command files.** Enforced by a lint rule + grep in `make lint`. Only the dispatcher calls `process.exit`.
2. **Every command's Effect carries a typed error union.** Enforced by TypeScript — `Effect<void, unknown, …>` is rejected at type-check time via a custom strictness rule (or a wrapper type).
3. **The dispatcher's error formatter is exhaustive.** Enforced by TypeScript's exhaustiveness check on the `switch (err._tag)`.
4. **No mid-migration "legacy" code paths after the last migration PR lands.** Enforced by RULE NLG — the dispatcher's compatibility shim is deleted in the closing PR.

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

---

## Acceptance Criteria

- [ ] `bun test` green in `zombiectl/`.
- [ ] `bun run lint` green; the custom no-`process.exit` rule is in `eslint.config.js` (or `bun run lint:custom` equivalent).
- [ ] `grep -rn 'process\.exit' zombiectl/src/commands/` returns zero matches.
- [ ] `grep -rn 'Effect.Effect<.*,.*unknown,' zombiectl/src/` returns zero matches.
- [ ] `zombiectl --help` output unchanged (regression).
- [ ] Every command listed in `cli/zombiectl.mdx` works against the integration fixture.
- [ ] `zombiectl/README.md` (or `CONTRIBUTING.md`) contains the Effect-TS conventions page.
- [ ] No file in `zombiectl/src/` over 350 lines as a result of the migration.

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
```

---

## Dead Code Sweep

The migration deletes the legacy dispatcher shim in its final PR. Before opening that PR:

| Deleted symbol | Grep | Expected |
|----------------|------|----------|
| `runCommandLegacy` (or whatever the shim is named) | `grep -rn 'runCommandLegacy' zombiectl/` | Zero matches |
| Per-command `try { … } catch (e) { process.exit(…) }` blocks | `grep -rn 'process\.exit' zombiectl/src/commands/` | Zero matches |

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After each migration PR, before CHORE(close) | `/write-unit-test` | Audits layer-mock coverage on the migrated commands. | Clean. |
| After migration complete, before final CHORE(close) | `/review` | Adversarial pass: every command returns a typed Effect, dispatcher is the only exit-code surface, no mid-migration shim survives. | Findings dispositioned. |
| After each `gh pr create` | `/review-pr` | Catches Effect signatures with `unknown` or `any` error channels. | Comments addressed before human review. |

---

## Discovery (consult log)

Empty at creation. Open questions in §Open Questions below need Captain decisions at plan-eng-review.

---

## Open Questions (Captain decides)

- **Q1: Effect-TS major version.** v3.x is current stable; `effect` package is umbrella. Pin range or take `^3.x`?
- **Q2: Layer split.** Single `MainLayer` per dispatcher run vs per-command custom layer subset. The latter is more disciplined (a command can only use what it asked for); the former is simpler boilerplate. Captain picks.
- **Q3: Migration PR cadence.** One PR per command group (workspace / credential / billing / auth …) or one mega-PR? Recommendation: per-group; lands incrementally, each reviewable, dispatcher shim handles the in-flight mixed state.
- **Q4: Codegen for the error taxonomy.** Should the `CliError` union be hand-maintained or generated from the same Zig registry M73_001 will codegen? Recommendation: hand-maintained for v1, revisit after M73_001 ships (the two specs naturally compose).
- **Q5: TypeScript strictness uplift.** Effect-TS works best with `strict: true` + `noUncheckedIndexedAccess: true`. Current `tsconfig.json` strictness?

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| no process.exit (E1) | see Eval | | |
| no untyped Effect (E2) | see Eval | | |
| tests + lint (E3) | `bun test && bun run lint` | | |
| help regression (E4) | `diff` | | |

---

## Out of Scope

- **Server-side runtime changes.** Zig handlers stay as-is; the migration is CLI-only.
- **`ui/packages/app/` Effect-TS adoption.** Dashboard is React + Next; Effect-TS in the browser is a separate decision.
- **Effect-TS for the executor sandbox.** Sandbox is Zig; out of scope.
- **`mintlify` docs generation from Effect signatures.** Docs stay hand-maintained; auto-generating command reference from type signatures is a follow-up.
- **`fp-ts` (the predecessor library).** Not adopted; Effect-TS is the chosen runtime.
- **The auth-handshake-hardening work.** Sibling workstream M74_002 ships on top of this substrate.
