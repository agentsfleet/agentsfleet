// D23 — hydrateWorkspacesAfterLogin's fail-loud branches. Errors during
// the workspace-list fetch (transport or server) or the workspaces.save
// step (disk) must surface a single stderr warn line; the Effect itself
// stays on the success channel so login still exits 0.

import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer, Redacted } from "effect";
import { hydrateWorkspacesAfterLogin } from "../src/commands/login-helpers.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import {
  NetworkError,
  ServerError,
  UnexpectedError,
} from "../src/errors/index.ts";

interface Rec {
  readonly stderr: string[];
  saved: number;
}

const makeRec = (): Rec => ({ stderr: [], saved: 0 });

const outputLayer = (rec: Rec): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: () => Effect.void,
    info: () => Effect.void,
    success: () => Effect.void,
    warn: (msg) => Effect.sync(() => rec.stderr.push(msg)),
    error: () => Effect.void,
    outro: () => Effect.void,
    printJson: () => Effect.void,
    printJsonErr: () => Effect.void,
    printKeyValue: () => Effect.void,
    printSection: () => Effect.void,
    printTable: () => Effect.void,
  });

const httpLayer = (
  responder: () => Effect.Effect<unknown, NetworkError | ServerError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input: HttpRequestInput) =>
      input.path === "/v1/tenants/me/workspaces"
        ? (responder() as Effect.Effect<never, NetworkError | ServerError>)
        : Effect.die(`unexpected ${input.path}`),
  });

const workspacesLayer = (
  rec: Rec,
  saveResult: Effect.Effect<void, UnexpectedError> = Effect.void,
): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.succeed({ current_workspace_id: null, items: [] }),
    save: () =>
      saveResult.pipe(
        Effect.tap(() =>
          Effect.sync(() => {
            rec.saved += 1;
          }),
        ),
      ),
  });

const tok = Redacted.make("tok");

describe("hydrateWorkspacesAfterLogin", () => {
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
    expect(rec.stderr[0]).toContain("workspace list");
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

  test("empty items array is silent (not a failure)", async () => {
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(httpLayer(() => Effect.succeed({ items: [] }))),
        Effect.provide(outputLayer(rec)),
        Effect.provide(workspacesLayer(rec)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stderr).toHaveLength(0);
    expect(rec.saved).toBe(0);
  });

  test("save failure → warn carrying 'unexpected'", async () => {
    const rec = makeRec();
    const failingSave = Effect.fail(
      new UnexpectedError({ detail: "disk full", suggestion: "free space" }),
    );
    const items = [{ workspace_id: "ws_1", name: "n", created_at: 1 }];
    const exit = await Effect.runPromiseExit(
      hydrateWorkspacesAfterLogin(tok).pipe(
        Effect.provide(httpLayer(() => Effect.succeed({ items }))),
        Effect.provide(outputLayer(rec)),
        Effect.provide(workspacesLayer(rec, failingSave)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stderr[0]).toContain("(unexpected)");
  });
});
