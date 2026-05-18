# Contributing to `zombiectl`

`zombiectl` is the usezombie command-line interface, written in TypeScript and structured around [Effect-TS](https://effect.website/) 4.x as the runtime model. The shape mirrors Supabase's `apps/cli/src/next/` reference (visible at `~/Projects/oss/cli` when working in this repo). Read the reference once before adding a new command — most patterns there transfer directly.

## Runtime model — at a glance

Every command is an `Effect.Effect<A, CliError, Services>`. The dispatcher (`src/lib/run-effect.ts`) provides `MainLayer` (`src/runtime/main-layer.ts`) at the boundary, runs the Effect via `Effect.runPromiseExit`, and translates the result into a process exit code through one shared formatter that knows every error variant.

**No command file imports `process.exit`.** Errors flow up the Effect cause channel; the dispatcher is the single seam.

## Adding a new command

1. **Write the handler as an `Effect.gen`** in the relevant `src/commands/<group>.ts`:
   ```ts
   export const myCommandEffect = Effect.gen(function* () {
     const http = yield* HttpClient;
     const output = yield* Output;
     const result = yield* http.request({ path: "/v1/things", method: "GET" });
     yield* output.printJson(result);
   });
   ```
2. **Declare the error subset** in the signature. The formatter switches on `_tag`, so an untyped `unknown` error channel is rejected by `audit-runtime-imports.mjs`.
3. **Wire the command into `src/program/cli-tree.ts`** and bind the handler in `src/program/handlers-bind.ts` via `wrapE("name", myCommandEffect)`. The wrapper applies `withCommandInstrumentation()` so every command emits `cli_command_executed` analytics + a tracing span automatically.

## Services and layers

`MainLayer` provides every service a command can need:

| Service | Owns |
|---|---|
| `Output` | stdout/stderr write, JSON / table / spinner rendering |
| `HttpClient` | API requests, retry, error classification |
| `Credentials` | on-disk token (Redacted) |
| `CliConfig` | `--api`/`--json`/`--no-open` + `ZOMBIE_API_URL` resolution |
| `Workspaces` | persistent workspace selection |
| `Browser` | `xdg-open` / `open` wrapper for login URLs |
| `Spinner` | TTY-aware indeterminate progress |
| `Analytics` | PostHog capture/identify/alias/groupIdentify (consent-gated) |
| `TelemetryRuntime` | device/session/distinct ID, consent state, isFirstRun |
| `AiTool` | detected wrapping agent (Claude Code, Cursor, etc.) via `@vercel/detect-agent` |
| `CommandRuntime` | per-invocation command name + run ID for instrumentation |

Each service follows the same split: `<name>.service.ts` declares the `Context.Service` tag + shape; `<name>.layer.ts` builds the implementation. Tests `Effect.provide(...)` an in-memory `Layer.succeed(<Tag>, <stub>)`.

## Adding an error variant

Errors live in `src/errors/index.ts` as `Data.TaggedError` classes (e.g. `AuthError`, `NetworkError`, `ServerError`, `ValidationError`). Add a new class, then update the formatter in `src/lib/run-effect.ts`. Adding a variant without updating the formatter fails the type check.

Server-relayed errors carry `UZ-<CAT>-<NNN>` codes from `docs/architecture/error_codes.md`. Match the variant to the category, not to a single endpoint.

## Testing

Unit tests use `bun:test`. Two patterns:

1. **Layer test** — compose the command Effect with stub layers, run via `Effect.runPromiseExit`, assert on the resulting Exit + the recorder's captured side effects. See `test/login-effect.unit.test.ts` for the canonical shape.
2. **Service test** — exercise one layer in isolation by providing its dependencies as stubs. See `test/telemetry/analytics.layer.unit.test.ts` for the PostHog-mock shape.

Acceptance tests in `test/acceptance/` spawn the built binary and assert on stdout/exit code. They run against `ZOMBIE_ACCEPTANCE_TARGET` when set to an https URL; otherwise the live-API cases register as skip-only placeholders.

Coverage floor is enforced by `scripts/enforce-coverage.mjs` (currently `function ≥96 / line ≥97`); see `bunfig.toml` for the per-file ignore list (Context.Tag-only shapes + pre-existing zombie/* command surface deferred to a follow-up coverage spec).

## Effect-TS conventions

- **`Effect.gen` for sequential composition, `Effect.pipe` for one-line transforms.** A 3+ step pipeline reads better as `Effect.gen`; a single `.pipe(Effect.map(...))` reads better inline.
- **`Effect.fn("name")(function*() { ... })`** for traceable named effects. Plain `function*` is fine for inline helpers.
- **No `Effect.catch(() => Effect.void)` to swallow telemetry.** Trust the layer (`Analytics` is consent-gated and posthog-node fire-and-forget); a `.catchCause` at the call site is only justified if a specific failure must not propagate.
- **`Redacted.Redacted<string>`** for any token / API key. Never log a `Redacted` value directly — use `Redacted.value()` only at the HTTP boundary.

## Telemetry

Anonymous usage data is on by default and gated by `getEffectiveConsent` (env → file → default). Three knobs:

- `ZOMBIE_TELEMETRY_DISABLED=1` — kill switch
- `DO_NOT_TRACK=1` — industry-standard signal (same effect)
- `~/.config/zombiectl/telemetry.json` `consent: "denied"` — persistent

Adding a new event: declare the name in `src/constants/analytics-events.ts`, then `yield* analytics.capture(EVT_NAME, properties)` inside the command Effect. `withCommandInstrumentation` already attaches `command`, `command_run_id`, `flags_used`, and `flag_values` to the span context; per-event properties merge on top.

### PostHog projects — prod bundled, dev for e2e

The production PostHog project key is bundled in `src/services/telemetry/analytics.layer.ts:DEFAULT_POSTHOG_KEY` (write-only `phc_…` credential, public-by-design — same model as Stripe `pk_live_…`). End-to-end suites that actually emit telemetry must NOT pollute the prod project. Convention:

```bash
# Local dev — point telemetry at the dev project before running acceptance:
export ZOMBIE_TELEMETRY_POSTHOG_KEY="$(op read 'op://ops/posthog-dev-cli/phc_key')"
export ZOMBIE_TELEMETRY_POSTHOG_HOST="https://us.i.posthog.com"
bun test test/acceptance

# CI — the e2e job exports the same vars from the vault secret before
# the acceptance step. Unit tests stay opted out via
# ZOMBIE_TELEMETRY_DISABLED=1 and never need the override.
```

Tests in `test/` set `ZOMBIE_TELEMETRY_DISABLED=1` in `beforeAll` so they hit the analytics layer's noop branch without any network call. The dev-key override only matters when the suite is configured to make real PostHog requests (acceptance against `ZOMBIE_ACCEPTANCE_TARGET=https://...`).

## Cross-runtime constants

Identifiers that cross the Zig/TypeScript boundary (event names, error codes, env var keys, schema versions) live in `src/constants/*.ts` and must be spelled the same as their Zig counterparts. The UFS gate (`audit-const-names.mjs`) enforces this.

## Style

- File length ≤ 350 lines; function ≤ 50 lines; method ≤ 70 lines. Enforced by the gate.
- No `process.exit` in `src/commands/`. Use the error channel.
- No `legacy`/`compat`/`shim`/`deprecated` framing in committed code (RULE NLG, pre-2.0).
- Imports at the top of the file; no `await import()` inside function bodies (`audit-runtime-imports.mjs`).
