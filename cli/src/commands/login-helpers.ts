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
import { TENANT_WORKSPACES_PATH } from "../lib/api-paths.ts";
import { SIGINT } from "../constants/signals.ts";
import { decodeWorkspacePage } from "./workspace-response-decoders.ts";
import {
  InterruptedError,
  UnexpectedError,
  type CliError,
  type NetworkError,
  type ServerError,
} from "../errors/index.ts";

const FIELD_TOKEN = "token" as const;
const SIGN_IN_AGAIN = "sign in again" as const;

const invalidWorkspacePage = (detail: string): UnexpectedError =>
  new UnexpectedError({ detail, suggestion: SIGN_IN_AGAIN });
// login_method analytics dimension — distinguishes the interactive browser
// device flow from a directly-supplied token (--token / env / piped stdin).
export type LoginMethod = "browser" | typeof FIELD_TOKEN;

type HydrationError = NetworkError | ServerError | UnexpectedError;

// Render any underlying error as a single-line stderr warn so login still
// exits 0 — workspace hydration is best-effort, not a login dependency.
// The operator can recover by signing in again to repeat hydration.
const reasonOf = (err: HydrationError): string =>
  err._tag === "ServerError"
    ? err.code
    : err._tag === "NetworkError"
      ? "network"
      : "unexpected";

const warnHydrationFailure = (
  err: HydrationError,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    yield* output.warn(
      `post-login workspace hydration failed (${reasonOf(err)}) — ${SIGN_IN_AGAIN} to retry`,
    );
  });

type FetchOutcome =
  | {
      readonly ok: true;
      readonly value: {
        items: WorkspaceItem[];
        tenant_id: string;
      };
    }
  | { readonly ok: false; readonly err: HydrationError };
type SaveOutcome =
  { readonly ok: true } | { readonly ok: false; readonly err: HydrationError };

const workspacePagePath = (startingAfter?: string): string => {
  const query = new URLSearchParams({ limit: "100" });
  if (startingAfter) query.set("starting_after", startingAfter);
  return `${TENANT_WORKSPACES_PATH}?${query.toString()}`;
};

export const hydrateWorkspacesAfterLogin = (
  token: Redacted.Redacted<string>,
): Effect.Effect<void, never, HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const workspaces = yield* Workspaces;
    const response: FetchOutcome = yield* Effect.gen(function* () {
      const items: WorkspaceItem[] = [];
      const seenCursors = new Set<string>();
      let tenantId: string | null = null;
      let startingAfter: string | undefined;

      while (true) {
        const rawPage = yield* http.request<unknown>({
          path: workspacePagePath(startingAfter),
          token,
        });
        const page = decodeWorkspacePage(rawPage);
        if (page === null) {
          return yield* Effect.fail(
            invalidWorkspacePage("workspace pagination response is invalid"),
          );
        }
        const pageTenantId = page.tenant_id;
        if (tenantId !== null && tenantId !== pageTenantId) {
          return yield* Effect.fail(
            invalidWorkspacePage(
              "workspace pagination changed the resolved tenant identifier",
            ),
          );
        }
        tenantId = pageTenantId;
        items.push(
          ...page.items.map((item) => ({
            workspace_id: item.id,
            name: item.name,
            created_at: item.created_at,
          })),
        );

        if (page.next_cursor === null) break;
        if (
          seenCursors.has(page.next_cursor)
        ) {
          return yield* Effect.fail(
            invalidWorkspacePage(
              "workspace pagination returned an invalid cursor",
            ),
          );
        }
        seenCursors.add(page.next_cursor);
        startingAfter = page.next_cursor;
      }
      return { items, tenant_id: tenantId };
    })
      .pipe(
        Effect.match({
          onSuccess: (value): FetchOutcome => ({ ok: true, value }),
          onFailure: (err): FetchOutcome => ({ ok: false, err }),
        }),
      );
    if (!response.ok) return yield* warnHydrationFailure(response.err);

    const items = response.value.items;
    const previous = yield* workspaces.load.pipe(
      Effect.orElseSucceed(() => ({
        tenant_id: null,
        current_workspace_id: null,
        items: [],
      })),
    );
    const tenantId = response.value.tenant_id;
    if (items.length === 0) {
      const saveResult: SaveOutcome = yield* workspaces
        .save({
          tenant_id: tenantId,
          current_workspace_id: null,
          items: [],
        })
        .pipe(
          Effect.match({
            onSuccess: (): SaveOutcome => ({ ok: true }),
            onFailure: (err): SaveOutcome => ({ ok: false, err }),
          }),
        );
      if (!saveResult.ok) return yield* warnHydrationFailure(saveResult.err);
      return;
    }

    const sameTenant = previous.tenant_id === tenantId;
    const persistedItems = items;
    const existingCurrent = persistedItems.find(
      (item) =>
        sameTenant && item.workspace_id === previous.current_workspace_id,
    );
    const firstItem = persistedItems[0];
    if (!firstItem) return;
    const current = existingCurrent?.workspace_id ?? firstItem.workspace_id;
    const saveResult: SaveOutcome = yield* workspaces
      .save({
        tenant_id: tenantId,
        current_workspace_id: current,
        items: persistedItems,
      })
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
    yield* analytics.capture(EVT_LOGIN_COMPLETED, {
      session_id: sessionId,
      login_method: method,
    });
  });

const trimToUndefined = (value: string | undefined): string | undefined => {
  if (typeof value !== "string") return undefined;
  const t = value.trim();
  return t.length > 0 ? t : undefined;
};

// Non-interactive token resolution: --token flag → piped stdin (non-TTY).
// `none` means "no direct token" → the caller falls through to the browser
// device flow. A non-TTY shell with no token cannot complete the device
// flow (the verification code is typed by a human), so it fails fast with
// the same advice supabase's NoTtyError carries.
export const resolveDirectToken = (opts: {
  readonly tokenFlag: string | undefined;
}): Effect.Effect<Option.Option<string>, CliError, Stdin> =>
  Effect.gen(function* () {
    const flag = trimToUndefined(opts.tokenFlag);
    if (flag !== undefined) return Option.some(flag);
    const stdin = yield* Stdin;
    if (stdin.isTTY) return Option.none();
    const piped = trimToUndefined(yield* stdin.readToEnd);
    if (piped !== undefined) return Option.some(piped);
    return yield* Effect.fail(
      new InterruptedError({
        detail: "no token provided and stdin is not a terminal",
        suggestion: "pass --token or pipe the token on stdin",
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
  | Analytics
  | CliConfig
  | Credentials
  | HttpClient
  | Output
  | TelemetryRuntime
  | Workspaces
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
      // This client did not mint the supplied value and holds no identifier
      // for it, so there is nothing for a later logout to revoke by name.
      credentialId: null,
    });
    yield* hydrateWorkspacesAfterLogin(redacted);
    yield* captureLoginCompleted("", rawToken, FIELD_TOKEN);
  });
