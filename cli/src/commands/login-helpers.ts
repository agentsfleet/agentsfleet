// login.ts helpers — extracted so login.ts itself stays under the
// 350-line cap. Owns the workspace-hydration, spinner-handle, and
// SIGINT-abort plumbing that the main login orchestrator calls into.

import { Effect, Redacted } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces, type WorkspaceItem } from "../services/workspaces.ts";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { TelemetryRuntime } from "../services/telemetry/runtime.service.ts";
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
  UnexpectedError,
  type NetworkError,
  type ServerError,
} from "../errors/index.ts";

const SIGN_IN_AGAIN = "sign in again" as const;

const invalidWorkspacePage = (detail: string): UnexpectedError =>
  new UnexpectedError({ detail, suggestion: SIGN_IN_AGAIN });
// login_method analytics dimension. It once separated the interactive device
// flow from a directly-supplied token; seeding is retired, so the device flow
// is the only method that writes credentials. Kept as a named type, and still
// sent on the event, so the analytics field holds its shape for existing
// queries rather than disappearing from the payload.
export type LoginMethod = "browser";

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

// Direct-token seeding lived here until it was removed. `AGENTSFLEET_API_KEY`
// already carries a tenant key on every request and outranks the stored
// credential, so the flag was a second path to the same outcome — and the
// only one that could write a value the credential loader would later
// refuse. Unattended callers use the environment variable; the device flow
// is the only thing that writes `credentials.json`.
