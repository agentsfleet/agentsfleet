import { expect, test } from "bun:test";
import { Effect, Layer, Option } from "effect";
import { CliConfig } from "../src/services/config.ts";
import { HttpClient, httpClientLayer } from "../src/services/http-client.ts";

test("a pre-encoded request body reaches the server unchanged", async () => {
  const bodies: string[] = [];
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: async request => {
    bodies.push(await request.text());
    return Response.json({ saved: true });
  } });
  const origin = `http://127.0.0.1:${server.port}`;
  const config = Layer.succeed(CliConfig, {
    apiUrl: origin, dashboardUrl: origin, accessToken: Option.none(), jsonMode: false, noOpen: true,
    telemetryPosthogKey: "", telemetryPosthogHost: origin,
  });
  const encoded = '{"message":"Support triage"}';
  try {
    const response = await Effect.runPromise(Effect.gen(function* () {
      const http = yield* HttpClient;
      return yield* http.request({ path: "/save", method: "POST", body: encoded });
    }).pipe(Effect.provide(Layer.provide(httpClientLayer, config))));
    expect(response).toEqual({ saved: true });
    expect(bodies).toEqual([encoded]);
  } finally { await server.stop(true); }
});
