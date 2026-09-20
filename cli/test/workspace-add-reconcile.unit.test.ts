// `workspace add` — the cases where a conflict IS reconciled against the
// tenant list: a committed response the network lost, a duplicate the server
// already holds, and rows whose tenant ownership has to be replaced.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Option, Redacted } from "effect";
import { workspaceAddEffect } from "../src/commands/workspace.ts";
import { ERR_WORKSPACE_NAME_EXISTS } from "../src/services/http-client.ts";
import type { WorkspacesValue } from "../src/services/workspaces.ts";
import { NetworkError, ServerError } from "../src/errors/index.ts";
import {
  WS_ID,
  WS_ID_2,
  TENANT_ID,
  OTHER_TENANT_ID,
  HTTP_STATUS_CONFLICT,
  makeRecorder,
  outputLayer,
  analyticsLayer,
  workspacesLayer,
  credentialsLayer,
  httpClientLayer,
  configLayer,
  runWith,
  expectFailure,
} from "./helpers-workspace-effect.ts";

describe("workspaceAddEffect — reconciles a conflict against the tenant list", () => {
  test("reconciles a committed response loss from the tenant list", async () => {
    const rec = makeRecorder();
    const requests: Array<{ path: string; method: string | undefined }> = [];
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("recovered & ready").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((path, method) => {
          requests.push({ path, method });
          if (method === "POST") {
            return Effect.fail(
              new ServerError({
                detail: "response lost",
                suggestion: "retry",
                code: "INTERNAL_ERROR",
                status: 500,
                requestId: "req_lost",
              }),
            );
          }
          return Effect.succeed({
            items: [
              {
                id: WS_ID,
                name: "recovered & ready",
                created_at: 77,
              },
            ],
            tenant_id: TENANT_ID,
            total: null,
            next_cursor: null,
          });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(Exit.isSuccess(await runWith(program))).toBe(true);
    expect(requests).toEqual([
      { path: "/v1/workspaces", method: "POST" },
      {
        path: "/v1/tenants/me/workspaces?name=recovered+%26+ready&limit=1",
        method: undefined,
      },
    ]);
    expect(workspacesState.value).toEqual({
      tenant_id: TENANT_ID,
      current_workspace_id: WS_ID,
      items: [
        {
          workspace_id: WS_ID,
          name: "recovered & ready",
          created_at: 77,
        },
      ],
    });
    expect(rec.events.map(({ event }) => event)).toEqual([
      "workspace_add_completed",
    ]);
  });

  test("preserves a network failure when reconciliation data is malformed", async () => {
    const rec = makeRecorder();
    const original = new NetworkError({
      detail: "socket closed",
      suggestion: "check network",
      url: "https://api.test.local/v1/workspaces",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("network-recovered").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((_path, method) =>
          method === "POST"
            ? Effect.fail(original)
            : Effect.succeed({
                tenant_id: TENANT_ID,
                items: [
                  {
                    id: WS_ID,
                    name: "network-recovered",
                    created_at: "invalid",
                  },
                ],
                total: null,
                next_cursor: null,
              }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBe(original);
    expect(workspacesState.value.items).toEqual([]);
  });

  test("reconciles a registered duplicate from the tenant list", async () => {
    const rec = makeRecorder();
    const requests: Array<{ path: string; method: string | undefined }> = [];
    const original = new ServerError({
      detail: "name exists",
      suggestion: "list or rename",
      code: ERR_WORKSPACE_NAME_EXISTS,
      status: HTTP_STATUS_CONFLICT,
      requestId: "req_duplicate",
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
                items: [{ id: WS_ID, name: "wanted", created_at: 81 }],
                total: null,
                next_cursor: null,
              });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(Exit.isSuccess(await runWith(program))).toBe(true);
    expect(requests).toEqual([
      { path: "/v1/workspaces", method: "POST" },
      {
        path: "/v1/tenants/me/workspaces?name=wanted&limit=1",
        method: undefined,
      },
    ]);
    expect(workspacesState.value).toEqual({
      tenant_id: TENANT_ID,
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "wanted", created_at: 81 }],
    });
    expect(rec.events.map(({ event }) => event)).toEqual([
      "workspace_add_completed",
    ]);
    expect(rec.stdout).toContain("# Workspace selected");
  });

  test("reconciliation replaces workspace state from a different tenant", async () => {
    const rec = makeRecorder();
    const original = new ServerError({
      detail: "name exists",
      suggestion: "list or rename",
      code: ERR_WORKSPACE_NAME_EXISTS,
      status: HTTP_STATUS_CONFLICT,
      requestId: "req_tenant_changed",
    });
    const workspacesState = {
      value: {
        tenant_id: OTHER_TENANT_ID,
        current_workspace_id: WS_ID_2,
        items: [
          {
            workspace_id: WS_ID_2,
            name: "old tenant",
            created_at: 1,
          },
        ],
      } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((_path, method) =>
          method === "POST"
            ? Effect.fail(original)
            : Effect.succeed({
                tenant_id: TENANT_ID,
                items: [{ id: WS_ID, name: "wanted", created_at: 81 }],
                total: null,
                next_cursor: null,
              }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(Exit.isSuccess(await runWith(program))).toBe(true);
    expect(workspacesState.value).toEqual({
      tenant_id: TENANT_ID,
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "wanted", created_at: 81 }],
    });
  });

  test("create replaces cached rows whose tenant ownership is unknown", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID_2,
        items: [
          {
            workspace_id: WS_ID_2,
            name: "unverified",
            created_at: 1,
          },
        ],
      } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer(() =>
          Effect.succeed({
            workspace_id: WS_ID,
            name: "wanted",
            tenant_id: TENANT_ID,
            request_id: "req_wanted",
          }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(Exit.isSuccess(await runWith(program))).toBe(true);
    expect(workspacesState.value).toMatchObject({
      tenant_id: TENANT_ID,
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "wanted" }],
    });
  });
});
