// `workspace add` — the cases where a conflict is PRESERVED rather than
// reconciled. Reading them beside the reconciling cases is what makes the
// boundary between the two legible.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Option, Redacted } from "effect";
import { workspaceAddEffect } from "../src/commands/workspace.ts";
import { ERR_WORKSPACE_NAME_EXISTS } from "../src/services/http-client.ts";
import type { WorkspacesValue } from "../src/services/workspaces.ts";
import { ServerError } from "../src/errors/index.ts";
import {
  WS_ID,
  TENANT_ID,
  HTTP_STATUS_CONFLICT,
  makeRecorder,
  outputLayer,
  analyticsLayer,
  workspacesLayer,
  type FakeCredsState,
  credentialsLayer,
  httpClientLayer,
  configLayer,
  runWith,
  expectFailure,
} from "./helpers-workspace-effect.ts";

describe("workspaceAddEffect — preserves a conflict it cannot reconcile", () => {
  test("preserves a registered duplicate when the list has no exact match", async () => {
    const rec = makeRecorder();
    const requests: Array<{ path: string; method: string | undefined }> = [];
    const original = new ServerError({
      detail: "name exists",
      suggestion: "list or rename",
      code: ERR_WORKSPACE_NAME_EXISTS,
      status: HTTP_STATUS_CONFLICT,
      requestId: "req_duplicate_missing",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((path, method) => {
          requests.push({ path, method });
          return method === "POST"
            ? Effect.fail(original)
            : Effect.succeed({
                tenant_id: TENANT_ID,
                items: [
                  {
                    id: WS_ID,
                    name: "different",
                    created_at: 81,
                  },
                ],
                total: null,
                next_cursor: null,
              });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBe(original);
    expect(requests).toEqual([
      { path: "/v1/workspaces", method: "POST" },
      {
        path: "/v1/tenants/me/workspaces?name=wanted&limit=1",
        method: undefined,
      },
    ]);
    expect(workspacesState.value.items).toEqual([]);
    expect(rec.events).toEqual([]);
  });

  test("preserves a registered duplicate when the list request fails", async () => {
    const rec = makeRecorder();
    const requests: Array<{ path: string; method: string | undefined }> = [];
    const original = new ServerError({
      detail: "name exists",
      suggestion: "list or rename",
      code: ERR_WORKSPACE_NAME_EXISTS,
      status: HTTP_STATUS_CONFLICT,
      requestId: "req_duplicate_list_failure",
    });
    const listFailure = new ServerError({
      detail: "list failed",
      suggestion: "retry",
      code: "INTERNAL_ERROR",
      status: 500,
      requestId: "req_list_failure",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((path, method) => {
          requests.push({ path, method });
          return Effect.fail(method === "POST" ? original : listFailure);
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBe(original);
    expect(requests).toEqual([
      { path: "/v1/workspaces", method: "POST" },
      {
        path: "/v1/tenants/me/workspaces?name=wanted&limit=1",
        method: undefined,
      },
    ]);
    expect(workspacesState.value.items).toEqual([]);
    expect(rec.events).toEqual([]);
  });

  test("does not reconcile an unregistered conflict", async () => {
    const rec = makeRecorder();
    let requestCount = 0;
    const original = new ServerError({
      detail: "conflict",
      suggestion: "inspect the request",
      code: "UZ-OTHER-001",
      status: HTTP_STATUS_CONFLICT,
      requestId: "req_other_conflict",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer(() => {
          requestCount += 1;
          return Effect.fail(original);
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBe(original);
    expect(requestCount).toBe(1);
    expect(workspacesState.value.items).toEqual([]);
    expect(rec.events).toEqual([]);
  });

  test("does not reconcile an ordinary client error", async () => {
    const rec = makeRecorder();
    let requestCount = 0;
    const original = new ServerError({
      detail: "invalid",
      suggestion: "fix input",
      code: "UZ-INVALID-REQUEST",
      status: 400,
      requestId: "req_invalid",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("invalid").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer(() => {
          requestCount += 1;
          return Effect.fail(original);
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBe(original);
    expect(requestCount).toBe(1);
  });

  test("re-adding an already-known workspace keeps the existing item list", async () => {
    // Pre-seed the store with the workspace the API returns. The add-path
    // dedupe runs `state.items.find(...)` over a NON-empty list — that
    // predicate arrow never fires when items start empty — and takes the
    // `existing ? state.items` branch instead of appending a duplicate.
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        tenant_id: TENANT_ID,
        current_workspace_id: null,
        items: [{ workspace_id: WS_ID, name: "pre", created_at: 7 }],
      } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    const program = workspaceAddEffect("pre").pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer(() =>
          Effect.succeed({
            workspace_id: WS_ID,
            name: "pre",
            tenant_id: TENANT_ID,
            request_id: "req_pre",
          }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    // No duplicate appended; the original single item is preserved.
    expect(workspacesState.value.items).toHaveLength(1);
    expect(workspacesState.value.items[0]?.created_at).toBe(7);
    expect(workspacesState.value.current_workspace_id).toBe(WS_ID);
  });
});
