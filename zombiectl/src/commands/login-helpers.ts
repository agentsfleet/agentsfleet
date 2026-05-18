// login.ts helpers — extracted so login.ts itself stays under the
// 350-line cap. Owns the workspace-hydration, spinner-handle, and
// SIGINT-abort plumbing that the main login orchestrator calls into.

import { Effect, Option, Redacted } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { Spinner } from "../services/spinner.ts";
import { CliConfig } from "../services/config.ts";
import { Workspaces, type WorkspaceItem } from "../services/workspaces.ts";
import { SIGINT } from "../constants/signals.ts";

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

export const hydrateWorkspacesAfterLogin = (
  token: Redacted.Redacted<string>,
): Effect.Effect<void, never, HttpClient | Workspaces> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const workspaces = yield* Workspaces;
    const response = yield* http
      .request<{ items?: unknown[] }>({
        path: TENANT_WORKSPACES_PATH,
        token,
      })
      .pipe(Effect.option);
    if (Option.isNone(response)) return;
    const fallbackCreatedAt = Date.now();
    const items = (Array.isArray(response.value.items) ? response.value.items : [])
      .map((item) => normalizeWorkspaceItem(item, fallbackCreatedAt))
      .filter((x): x is WorkspaceItem => x !== null);
    if (items.length === 0) return;

    const previous = yield* workspaces.load.pipe(
      Effect.catchAll(() => Effect.succeed({ current_workspace_id: null, items: [] })),
    );
    const existingCurrent = items.find(
      (item) => item.workspace_id === previous.current_workspace_id,
    );
    const firstItem = items[0];
    if (!firstItem) return;
    const current = existingCurrent?.workspace_id ?? firstItem.workspace_id;
    yield* workspaces
      .save({ current_workspace_id: current, items })
      .pipe(Effect.catchAll(() => Effect.void));
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
