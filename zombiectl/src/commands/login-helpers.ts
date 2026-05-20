// login.ts helpers — extracted so login.ts itself stays under the
// 350-line cap. Owns the workspace-hydration, spinner-handle, and
// SIGINT-abort plumbing that the main login orchestrator calls into.

import { Effect, Redacted } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Spinner } from "../services/spinner.ts";
import { CliConfig } from "../services/config.ts";
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
import { SIGINT } from "../constants/signals.ts";
import type { NetworkError, ServerError, UnexpectedError } from "../errors/index.ts";

const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";

const normalizeWorkspaceItem = (
  raw: unknown,
  fallbackCreatedAt: number,
): WorkspaceItem | null => {
  if (!raw || typeof raw !== "object") return null;
  const rec = raw as Record<string, unknown>;
  const workspaceId =
    typeof rec["workspace_id"] === "string"
      ? rec["workspace_id"]
      : typeof rec["id"] === "string"
        ? rec["id"]
        : null;
  if (!workspaceId) return null;
  return {
    workspace_id: workspaceId,
    name: typeof rec["name"] === "string" ? rec["name"] : null,
    created_at:
      typeof rec["created_at"] === "number" && Number.isFinite(rec["created_at"])
        ? rec["created_at"]
        : fallbackCreatedAt,
  };
};

type HydrationError = NetworkError | ServerError | UnexpectedError;

// Render any underlying error as a single-line stderr warn so login still
// exits 0 — workspace hydration is best-effort, not a login dependency.
// The operator can recover by running `zombiectl workspace list` to
// re-fetch + persist on demand.
const reasonOf = (err: HydrationError): string =>
  err._tag === "ServerError" ? err.code : err._tag === "NetworkError" ? "network" : "unexpected";

const warnHydrationFailure = (
  err: HydrationError,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    yield* output.warn(
      `post-login workspace hydration failed (${reasonOf(err)}) — run \`zombiectl workspace list\` to retry`,
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

export interface SpinnerHandles {
  readonly succeed: Effect.Effect<void>;
  readonly fail: Effect.Effect<void>;
}

export const startSpinner = (
  label: string,
): Effect.Effect<SpinnerHandles, never, CliConfig | Spinner> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const spinner = yield* Spinner;
    const enabled = !config.jsonMode && Boolean((process.stderr as { isTTY?: boolean }).isTTY);
    const handle = yield* spinner.start({
      enabled,
      stream: process.stderr,
      label,
    });
    return { succeed: handle.succeed(), fail: handle.fail() };
  });

// Identify under the post-login distinct id so subsequent emits in the
// same fiber attribute correctly, then persist via saveDistinctId so
// later CLI invocations inherit the same identity from telemetry.json.
// Mirrors supabase login.handler.ts resolveAuthenticatedDistinctId.
export const captureLoginCompleted = (
  sessionId: string,
  token: string,
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
    yield* analytics.capture(EVT_LOGIN_COMPLETED, { session_id: sessionId });
  });
