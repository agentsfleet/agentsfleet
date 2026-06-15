// login.ts helpers — extracted so login.ts itself stays under the
// 350-line cap. Owns the workspace-hydration, spinner-handle, and
// SIGINT-abort plumbing that the main login orchestrator calls into.

import { Effect, Option, Redacted } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { Stdin } from "../services/stdin.ts";
import { Workspaces, type WorkspaceItem } from "../services/workspaces.ts";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { TelemetryRuntime } from "../services/telemetry/runtime.service.ts";
import { pingMe } from "../lib/me-ping.ts";
import { getConfigDir } from "../services/telemetry/consent.ts";
import {
  clearDistinctId,
  saveDistinctId,
} from "../services/telemetry/identity.ts";
import {
  EVT_LOGIN_COMPLETED,
  EVT_USER_AUTHENTICATED,
} from "../constants/analytics-events.ts";
import { extractDistinctIdFromToken } from "../program/auth-token.ts";
import { SIGINT } from "../constants/signals.ts";
import {
  InterruptedError,
  type CliError,
  type NetworkError,
  type ServerError,
  type UnexpectedError,
} from "../errors/index.ts";

const FIELD_CREATED_AT = "created_at" as const;
const FIELD_ID = "id" as const;
const FIELD_NAME = "name" as const;
const TYPE_NUMBER = "number" as const;
const TYPE_STRING = "string" as const;
const FIELD_TOKEN = "token" as const;
const FIELD_WORKSPACE_ID = "workspace_id" as const;
const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";

const isNumber = (value: unknown): value is number => typeof value === TYPE_NUMBER;
const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

// login_method analytics dimension — distinguishes the interactive browser
// device flow from a directly-supplied token (--token / env / piped stdin).
export type LoginMethod = "browser" | typeof FIELD_TOKEN;

const normalizeWorkspaceItem = (
  raw: unknown,
  fallbackCreatedAt: number,
): WorkspaceItem | null => {
  if (!raw || typeof raw !== "object") return null;
  const rec = raw as Record<string, unknown>;
  const workspaceId =
    isString(rec[FIELD_WORKSPACE_ID])
      ? rec[FIELD_WORKSPACE_ID]
      : isString(rec[FIELD_ID])
        ? rec[FIELD_ID]
        : null;
  if (!workspaceId) return null;
  return {
    workspace_id: workspaceId,
    name: isString(rec[FIELD_NAME]) ? rec[FIELD_NAME] : null,
    created_at:
      isNumber(rec[FIELD_CREATED_AT]) && Number.isFinite(rec[FIELD_CREATED_AT])
        ? rec[FIELD_CREATED_AT]
        : fallbackCreatedAt,
  };
};

type HydrationError = NetworkError | ServerError | UnexpectedError;

// Render any underlying error as a single-line stderr warn so login still
// exits 0 — workspace hydration is best-effort, not a login dependency.
// The operator can recover by running `agentsfleet workspace list` to
// re-fetch + persist on demand.
const reasonOf = (err: HydrationError): string =>
  err._tag === "ServerError" ? err.code : err._tag === "NetworkError" ? "network" : "unexpected";

const warnHydrationFailure = (
  err: HydrationError,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    yield* output.warn(
      `post-login workspace hydration failed (${reasonOf(err)}) — run \`agentsfleet workspace list\` to retry`,
    );
  });

type FetchOutcome = { readonly ok: true; readonly value: { items?: unknown[] } } | { readonly ok: false; readonly err: HydrationError };
type SaveOutcome = { readonly ok: true } | { readonly ok: false; readonly err: HydrationError };

export const hydrateWorkspacesAfterLogin = (
  token: Redacted.Redacted<string>,
): Effect.Effect<void, never, HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const workspaces = yield* Workspaces;
    const response: FetchOutcome = yield* http
      .request<{ items?: unknown[] }>({ path: TENANT_WORKSPACES_PATH, token })
      .pipe(
        Effect.match({
          onSuccess: (value): FetchOutcome => ({ ok: true, value }),
          onFailure: (err): FetchOutcome => ({ ok: false, err }),
        }),
      );
    if (!response.ok) return yield* warnHydrationFailure(response.err);

    const fallbackCreatedAt = Date.now();
    const items = (Array.isArray(response.value.items) ? response.value.items : [])
      .map((item) => normalizeWorkspaceItem(item, fallbackCreatedAt))
      .filter((x): x is WorkspaceItem => x !== null);
    if (items.length === 0) return;

    const previous = yield* workspaces.load.pipe(
      Effect.orElseSucceed(() => ({ current_workspace_id: null, items: [] })),
    );
    const existingCurrent = items.find(
      (item) => item.workspace_id === previous.current_workspace_id,
    );
    const firstItem = items[0];
    if (!firstItem) return;
    const current = existingCurrent?.workspace_id ?? firstItem.workspace_id;
    const saveResult: SaveOutcome = yield* workspaces
      .save({ current_workspace_id: current, items })
      .pipe(
        Effect.match({
          onSuccess: (): SaveOutcome => ({ ok: true }),
          onFailure: (err): SaveOutcome => ({ ok: false, err }),
        }),
      );
    if (!saveResult.ok) return yield* warnHydrationFailure(saveResult.err);
  });

// Promise+listener bridge: SIGINT during the poll loop aborts the
// controller so the next iteration short-circuits. Effect.interrupt is
// fiber-scoped — for an OS signal we still need a process-level
// listener wrapped in an acquireUseRelease scope.
export const withSigintAbort = <A, E, R>(
  body: (signal: AbortSignal) => Effect.Effect<A, E, R>,
): Effect.Effect<A, E, R> =>
  Effect.acquireUseRelease(
    Effect.sync(() => {
      const controller = new AbortController();
      const handler = (): void => controller.abort();
      process.on(SIGINT, handler);
      return { controller, handler };
    }),
    ({ controller }) => body(controller.signal),
    ({ handler }) =>
      Effect.sync(() => {
        process.removeListener(SIGINT, handler);
      }),
  );

// Identify under the post-login distinct id so subsequent emits in the
// same fiber attribute correctly, then persist via saveDistinctId so
// later CLI invocations inherit the same identity from telemetry.json.
// Mirrors supabase login.handler.ts resolveAuthenticatedDistinctId.
export const captureLoginCompleted = (
  sessionId: string,
  token: string,
  method: LoginMethod,
): Effect.Effect<void, never, Analytics | TelemetryRuntime> =>
  Effect.gen(function* () {
    const analytics = yield* Analytics;
    const runtime = yield* TelemetryRuntime;
    const configDir = yield* getConfigDir;
    const distinctId = extractDistinctIdFromToken(token);
    if (distinctId) {
      yield* analytics.alias(distinctId, runtime.deviceId);
      yield* analytics.identify(distinctId);
      yield* saveDistinctId(configDir, distinctId);
    } else {
      yield* clearDistinctId(configDir);
    }
    yield* analytics.capture(EVT_USER_AUTHENTICATED, { command: "login" });
    yield* analytics.capture(EVT_LOGIN_COMPLETED, { session_id: sessionId, login_method: method });
  });

const trimToUndefined = (value: string | undefined): string | undefined => {
  if (!isString(value)) return undefined;
  const t = value.trim();
  return t.length > 0 ? t : undefined;
};

// Non-interactive token resolution, mirroring supabase login.handler.ts
// resolveToken: --token flag → AGENTSFLEET_TOKEN env → piped stdin (non-TTY).
// `none` means "no direct token" → the caller falls through to the browser
// device flow. A non-TTY shell with no token cannot complete the device
// flow (the verification code is typed by a human), so it fails fast with
// the same advice supabase's NoTtyError carries.
export const resolveDirectToken = (opts: {
  readonly tokenFlag: string | undefined;
  readonly envToken: string | undefined;
}): Effect.Effect<Option.Option<string>, CliError, Stdin> =>
  Effect.gen(function* () {
    const flag = trimToUndefined(opts.tokenFlag);
    if (flag !== undefined) return Option.some(flag);
    const env = trimToUndefined(opts.envToken);
    if (env !== undefined) return Option.some(env);
    const stdin = yield* Stdin;
    if (stdin.isTTY) return Option.none();
    const piped = trimToUndefined(yield* stdin.readToEnd);
    if (piped !== undefined) return Option.some(piped);
    return yield* Effect.fail(
      new InterruptedError({
        detail: "no token provided and stdin is not a terminal",
        suggestion: "pass --token or set AGENTSFLEET_TOKEN",
      }),
    );
  });

// Direct-token login: validate against the API, then persist — never the
// other way round, so an invalid token leaves credentials.json untouched.
// No browser, no session_id (there is no device-flow session to label).
export const saveDirectToken = (
  rawToken: string,
): Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Credentials | HttpClient | Output | TelemetryRuntime | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const credentials = yield* Credentials;
    const redacted = Redacted.make(rawToken);
    yield* pingMe(redacted);
    yield* credentials.saveAccessToken({
      token: redacted,
      sessionId: null,
      apiUrl: config.apiUrl,
    });
    yield* hydrateWorkspacesAfterLogin(redacted);
    yield* captureLoginCompleted("", rawToken, FIELD_TOKEN);
  });
