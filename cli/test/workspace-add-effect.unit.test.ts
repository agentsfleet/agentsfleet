// `workspace add` — validation, dispatch, persistence and the JSON envelope.
// The conflict half lives in workspace-add-reconcile and workspace-add-conflict.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Option, Redacted } from "effect";
import { workspaceAddEffect } from "../src/commands/workspace.ts";
import type { HttpRequestInput } from "../src/services/http-client.ts";
import { OUTPUT_FORMAT } from "../src/services/output.ts";
import type { WorkspacesValue } from "../src/services/workspaces.ts";
import { ConfigError, ServerError, ValidationError } from "../src/errors/index.ts";
import {
  WS_ID,
  TENANT_ID,
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

describe("workspaceAddEffect", () => {
  test("trims the name, disables POST retry, persists, and emits analytics", async () => {
    const rec = makeRecorder();
    let requestInput: HttpRequestInput | null = null;
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    const program = workspaceAddEffect("  acme-prod  ").pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer((_path, _method, input) => {
          requestInput = input;
          return Effect.succeed({
            workspace_id: WS_ID,
            name: "acme-prod",
            tenant_id: TENANT_ID,
            request_id: "req_acme",
          });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(workspacesState.value.current_workspace_id).toBe(WS_ID);
    expect(workspacesState.value.items).toHaveLength(1);
    expect(workspacesState.value.items[0]?.workspace_id).toBe(WS_ID);
    expect(requestInput).toMatchObject({
      method: "POST",
      body: { name: "acme-prod" },
      retry: { maxAttempts: 1 },
    });
    expect(rec.events[0]?.event).toBe("workspace_add_completed");
    expect(rec.events[0]?.properties).toEqual({ workspace_id: WS_ID });
    expect(rec.events[1]?.event).toBe("workspace_created");
    expect(rec.stdout.some((line) => line.includes("# Workspace added"))).toBe(
      true,
    );
  });

  test("rejects a missing name before any HTTP request", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    let requestCount = 0;
    const program = workspaceAddEffect(undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer(() => {
          requestCount += 1;
          return Effect.succeed({
            workspace_id: WS_ID,
            name: "unused",
            tenant_id: TENANT_ID,
          });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(failure.detail).toContain("requires <name>");
    expect(requestCount).toBe(0);
  });

  test("rejects a whitespace-only name before any HTTP request", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    let requestCount = 0;
    const program = workspaceAddEffect("   ").pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer(() => {
          requestCount += 1;
          return Effect.succeed({
            workspace_id: WS_ID,
            name: "unused",
            tenant_id: TENANT_ID,
          });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(requestCount).toBe(0);
  });

  test("preserves Unicode whitespace while trimming ASCII edges", async () => {
    const rec = makeRecorder();
    let requestInput: HttpRequestInput | null = null;
    const normalized = "\u00a0acme\u3000";
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect(` \t${normalized}\r\n`).pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((_path, _method, input) => {
          requestInput = input;
          return Effect.succeed({
            workspace_id: WS_ID,
            name: normalized,
            tenant_id: TENANT_ID,
            request_id: "req_unicode_space",
          });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(Exit.isSuccess(await runWith(program))).toBe(true);
    expect(requestInput).toMatchObject({ body: { name: normalized } });
  });

  test("rejects a Unicode-whitespace-only name before dispatch", async () => {
    const rec = makeRecorder();
    let requestCount = 0;
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("\u00a0\u3000").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer(() => {
          requestCount += 1;
          return Effect.succeed({});
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBeInstanceOf(
      ValidationError,
    );
    expect(requestCount).toBe(0);
  });

  test("emits JSON envelope in jsonMode", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    const program = workspaceAddEffect("jolly-harbor").pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer(() =>
          Effect.succeed({
            workspace_id: WS_ID,
            name: "jolly-harbor",
            tenant_id: TENANT_ID,
            request_id: "req_jolly",
          }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stdout.some((line) => line.includes(`"workspace_id":"${WS_ID}"`)),
    ).toBe(true);
  });

  test("rejects a malformed successful create response before saving", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("malformed").pipe(
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
            name: "malformed",
            tenant_id: TENANT_ID,
          }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    const failure = expectFailure(await runWith(program));
    expect(failure._tag).toBe("UnexpectedError");
    expect(workspacesState.value.items).toEqual([]);
    expect(rec.events).toEqual([]);
  });

  test("does not persist on API failure", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    const program = workspaceAddEffect("x").pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer(() =>
          Effect.fail(
            new ServerError({
              detail: "boom",
              suggestion: "retry",
              code: "INTERNAL_ERROR",
              status: 0,
              requestId: "req_test",
            }),
          ),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isFailure(exit)).toBe(true);
    expect(workspacesState.value.items).toEqual([]);
  });
  test("fails ConfigError when no token configured", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("x").pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer({ token: Option.none() })),
      Effect.provide(
        httpClientLayer(() => Effect.succeed({ workspace_id: WS_ID })),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    const failure = expectFailure(exit);
    expect(failure).toBeInstanceOf(ConfigError);
  });
});
