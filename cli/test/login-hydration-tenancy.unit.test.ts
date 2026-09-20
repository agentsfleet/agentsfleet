// `hydrateWorkspacesAfterLogin` — paging and tenant ownership. A complete
// response is the only thing allowed to drop another tenant's cached rows, so
// these assert completeness before deletion, not after.

import { describe, expect, test } from "bun:test";
import { Effect, Exit } from "effect";
import { hydrateWorkspacesAfterLogin } from "../src/commands/login-helpers.ts";
import type { WorkspacesValue } from "../src/services/workspaces.ts";
import { makeRec, outputLayer, httpLayer, workspacesLayer, tok } from "./helpers-login-hydration.ts";

describe("hydrateWorkspacesAfterLogin — paging and tenant ownership", () => {
  test("complete response drops workspaces cached by another tenant", async () => {
    const rec = makeRec();
    const previous: WorkspacesValue = {
      tenant_id: "tenant_previous",
      current_workspace_id: "ws_previous",
      items: [
        {
          workspace_id: "ws_previous",
          name: "previous",
          created_at: 1,
        },
      ],
    };
    const items = [{ id: "ws_current", name: "current", created_at: 2 }];
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer(() =>
            Effect.succeed({
              items,
              tenant_id: "tenant_current",
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
    expect(rec.savedValue).toEqual({
      tenant_id: "tenant_current",
      current_workspace_id: "ws_current",
      items: [
        {
          workspace_id: "ws_current",
          name: "current",
          created_at: 2,
        },
      ],
    });
  });

  test("incomplete workspace pages leave cached state untouched", async () => {
    const malformedPages: unknown[] = [
      null,
      { tenant_id: "tenant_malformed", next_cursor: null },
      { tenant_id: "tenant_malformed", items: "invalid", next_cursor: null },
      { tenant_id: "tenant_malformed", items: [] },
    ];

    for (const page of malformedPages) {
      const rec = makeRec();
      const exit = await Effect.runPromiseExit(
        hydrateWorkspacesAfterLogin(tok).pipe(
          Effect.provide(httpLayer(() => Effect.succeed(page))),
          Effect.provide(outputLayer(rec)),
          Effect.provide(workspacesLayer(rec)),
        ),
      );

      expect(Exit.isSuccess(exit)).toBe(true);
      expect(rec.saved).toBe(0);
      expect(rec.stderr).toHaveLength(1);
      expect(rec.stderr[0]).toContain("(unexpected)");
    }
  });

  test("tenant changes between pages leave cached state untouched", async () => {
    const rec = makeRec();
    let requests = 0;
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(
          httpLayer(() => {
            requests += 1;
            return Effect.succeed(
              requests === 1
                ? {
                    items: [],
                    tenant_id: "tenant_first",
                    total: null,
                    next_cursor: "next",
                  }
                : {
                    items: [],
                    tenant_id: "tenant_second",
                    total: null,
                    next_cursor: null,
                  },
            );
          }),
        ),
        Effect.provide(outputLayer(rec)),
        Effect.provide(workspacesLayer(rec)),
      ),
    );

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(requests).toBe(2);
    expect(rec.saved).toBe(0);
    expect(rec.stderr).toHaveLength(1);
    expect(rec.stderr[0]).toContain("(unexpected)");
  });

  test("invalid cursors stop pagination without changing cached state", async () => {
    for (const cursor of ["", "repeat"]) {
      const rec = makeRec();
      let requests = 0;
      const exit = await Effect.runPromiseExit(
        hydrateWorkspacesAfterLogin(tok).pipe(
          Effect.provide(
            httpLayer(() => {
              requests += 1;
              return Effect.succeed({
                items: [],
                tenant_id: "tenant_cursor",
                total: null,
                next_cursor: cursor,
              });
            }),
          ),
          Effect.provide(outputLayer(rec)),
          Effect.provide(workspacesLayer(rec)),
        ),
      );

      expect(Exit.isSuccess(exit)).toBe(true);
      expect(requests).toBe(cursor === "" ? 1 : 2);
      expect(rec.saved).toBe(0);
      expect(rec.stderr).toHaveLength(1);
      expect(rec.stderr[0]).toContain("(unexpected)");
    }
  });
});
