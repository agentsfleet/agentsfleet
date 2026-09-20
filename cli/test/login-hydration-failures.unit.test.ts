// `hydrateWorkspacesAfterLogin` — what a login does when the authoritative
// read goes wrong. Every case ends the same way: one warn line, and cached
// state either cleared deliberately or left exactly as it was. A half-written
// workspace list is the outcome none of these may produce.

import { describe, expect, test } from "bun:test";
import { Effect, Exit } from "effect";
import { hydrateWorkspacesAfterLogin } from "../src/commands/login-helpers.ts";
import type { WorkspacesValue } from "../src/services/workspaces.ts";
import { NetworkError, ServerError, UnexpectedError } from "../src/errors/index.ts";
import { makeRec, outputLayer, httpLayer, workspacesLayer, tok } from "./helpers-login-hydration.ts";

describe("hydrateWorkspacesAfterLogin — a failed or partial read", () => {
  test("ServerError on /workspaces → single warn line carrying the UZ code", async () => {
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer(() =>
            Effect.fail(
              new ServerError({
                detail: "rate-limited",
                suggestion: "later",
                code: "UZ-RATELIMIT-001",
                status: 429,
                requestId: null,
              }),
            ),
          ),
        ),
        Effect.provide(outputLayer(rec)),
        Effect.provide(workspacesLayer(rec)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stderr).toHaveLength(1);
    expect(rec.stderr[0]).toContain("UZ-RATELIMIT-001");
    expect(rec.stderr[0]).toContain("sign in again");
  });

  test("NetworkError → warn line uses 'network' as the reason", async () => {
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer(() =>
            Effect.fail(
              new NetworkError({
                detail: "fetch failed",
                suggestion: "check",
                url: "https://api.test/v1/tenants/me/workspaces",
              }),
            ),
          ),
        ),
        Effect.provide(outputLayer(rec)),
        Effect.provide(workspacesLayer(rec)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stderr[0]).toContain("(network)");
  });

  test("empty authoritative response clears stale workspace state", async () => {
    const rec = makeRec();
    const previous: WorkspacesValue = {
      tenant_id: "tenant_previous",
      current_workspace_id: "ws_previous",
      items: [{ workspace_id: "ws_previous", name: "previous", created_at: 1 }],
    };
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer(() =>
            Effect.succeed({
              items: [],
              tenant_id: "tenant_empty",
              total: null,
              next_cursor: null,
            }),
          ),
        ),
        Effect.provide(outputLayer(rec)),
        Effect.provide(
          workspacesLayer(rec, Effect.void, Effect.succeed(previous)),
        ),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stderr).toHaveLength(0);
    expect(rec.savedValue).toEqual({
      tenant_id: "tenant_empty",
      current_workspace_id: null,
      items: [],
    });
  });

  test("missing authoritative tenant leaves cached state untouched", async () => {
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer(() =>
            Effect.succeed({
              items: [{ id: "ws_new", name: "new", created_at: 2 }],
              total: null,
              next_cursor: null,
            }),
          ),
        ),
        Effect.provide(outputLayer(rec)),
        Effect.provide(workspacesLayer(rec)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.saved).toBe(0);
    expect(rec.stderr).toHaveLength(1);
    expect(rec.stderr[0]).toContain("(unexpected)");
  });

  test("a malformed workspace item leaves cached state untouched", async () => {
    const rec = makeRec();
    const items = [
      null,
      {},
      { id: "ws_valid", name: "valid", created_at: 2 },
    ];
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer(() =>
            Effect.succeed({
              items,
              tenant_id: "tenant_default",
              total: null,
              next_cursor: null,
            }),
          ),
        ),
        Effect.provide(outputLayer(rec)),
        Effect.provide(workspacesLayer(rec)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.saved).toBe(0);
    expect(rec.stderr).toHaveLength(1);
    expect(rec.stderr[0]).toContain("(unexpected)");
  });

  test("workspace load failure falls back before saving hydrated items", async () => {
    const rec = makeRec();
    const items = [{ id: "ws_valid", name: "valid", created_at: 2 }];
    const failingLoad = Effect.fail(
      new UnexpectedError({ detail: "read failed", suggestion: "retry" }),
    );
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer(() =>
            Effect.succeed({
              items,
              tenant_id: "tenant_default",
              total: null,
              next_cursor: null,
            }),
          ),
        ),
        Effect.provide(outputLayer(rec)),
        Effect.provide(workspacesLayer(rec, Effect.void, failingLoad)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.saved).toBe(1);
    expect(rec.stderr).toHaveLength(0);
  });

  test("save failure → warn carrying 'unexpected'", async () => {
    const rec = makeRec();
    const failingSave = Effect.fail(
      new UnexpectedError({ detail: "disk full", suggestion: "free space" }),
    );
    const items = [{ id: "ws_1", name: "n", created_at: 1 }];
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer(() =>
            Effect.succeed({
              items,
              tenant_id: "tenant_default",
              total: null,
              next_cursor: null,
            }),
          ),
        ),
        Effect.provide(outputLayer(rec)),
        Effect.provide(workspacesLayer(rec, failingSave)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stderr[0]).toContain("(unexpected)");
  });

  test("canonical workspace item fields are persisted without fallbacks", async () => {
    const rec = makeRec();
    const items = [{ id: "ws_id_form", name: "from-id", created_at: 3 }];
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer(() =>
            Effect.succeed({
              items,
              tenant_id: "tenant_default",
              total: null,
              next_cursor: null,
            }),
          ),
        ),
        Effect.provide(outputLayer(rec)),
        Effect.provide(workspacesLayer(rec)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.saved).toBe(1);
    expect(rec.stderr).toHaveLength(0);
  });

  test("cursor pages preserve an active workspace returned on a later page", async () => {
    const rec = makeRec();
    const paths: string[] = [];
    const previous: WorkspacesValue = {
      tenant_id: "tenant_same",
      current_workspace_id: "ws_middle",
      items: [
        { workspace_id: "ws_oldest", name: "old", created_at: 1 },
        { workspace_id: "ws_middle", name: "middle", created_at: 2 },
      ],
    };
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer((input) => {
            paths.push(input.path);
            return Effect.succeed(
              input.path.includes("starting_after")
                ? {
                    items: [
                      {
                        id: "ws_middle",
                        name: "middle updated",
                        created_at: 2,
                      },
                      { id: "ws_newest", name: "new", created_at: 3 },
                    ],
                    tenant_id: "tenant_same",
                    total: null,
                    next_cursor: null,
                  }
                : {
                    items: [
                      {
                        id: "ws_oldest",
                        name: "updated",
                        created_at: 1,
                      },
                    ],
                    tenant_id: "tenant_same",
                    total: null,
                    next_cursor: "1:ws_oldest",
                  },
            );
          }),
        ),
        Effect.provide(outputLayer(rec)),
        Effect.provide(
          workspacesLayer(rec, Effect.void, Effect.succeed(previous)),
        ),
      ),
    );

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(paths).toEqual([
      "/v1/tenants/me/workspaces?limit=100",
      "/v1/tenants/me/workspaces?limit=100&starting_after=1%3Aws_oldest",
    ]);
    expect(rec.savedValue?.current_workspace_id).toBe("ws_middle");
    expect(rec.savedValue?.items).toEqual([
      { workspace_id: "ws_oldest", name: "updated", created_at: 1 },
      {
        workspace_id: "ws_middle",
        name: "middle updated",
        created_at: 2,
      },
      { workspace_id: "ws_newest", name: "new", created_at: 3 },
    ]);
  });
});
