import { it } from "bun:test";
import assert from "node:assert/strict";
import http from "node:http";
import type { AddressInfo } from "node:net";
import { Schema } from "effect";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import { acceptanceProxy } from "./fixtures/proxy.ts";
import { makeStubbedStateDir, STUB_TOKEN } from "./fixtures/state-dir.ts";

const ORIGIN = "http://origin.invalid";
const PROXY_KEYS = ["HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY", "http_proxy", "https_proxy", "no_proxy"] as const;
const ENV_NAMES = [
  {http: "HTTP_PROXY", https: "HTTPS_PROXY", bypass: "NO_PROXY"},
  {http: "http_proxy", https: "https_proxy", bypass: "no_proxy"},
] as const;
const failure = Schema.Struct({error: Schema.Struct({code: Schema.String})});

it("routes spawned requests through the configured proxy and keeps loopback traffic local", async () => {
  assert.equal(acceptanceProxy({}), undefined, "ordinary runners must keep Chromium's default routing");
  const state = await makeStubbedStateDir();
  const requests: Array<{url: string; authorization: string}> = [];
  const server = http.createServer((request, response) => {
    requests.push({url: request.url ?? "", authorization: request.headers.authorization ?? ""});
    response.writeHead(403, {"content-type": "application/problem+json"});
    response.end(JSON.stringify({docs_uri: "https://docs.agentsfleet.net/errors/UZ-AUTH-022",
      title: "Insufficient scope", detail: "Proxy acceptance probe", error_code: "UZ-AUTH-022",
      request_id: "proxy-routing-probe"}));
  });
  try {
    await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
    const proxy = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
    for (const names of ENV_NAMES) {
      const parent: NodeJS.ProcessEnv = {...process.env, AGENTSFLEET_API_KEY: "parent-credential-must-not-leak"};
      for (const key of PROXY_KEYS) delete parent[key];
      parent[names.http] = proxy; parent[names.https] = proxy; parent[names.bypass] = "localhost,127.0.0.1";
      const env = composeEnv({AGENTSFLEET_STATE_DIR: state.dir, NO_COLOR: "1"}, parent);
      assert.equal(env.AGENTSFLEET_API_KEY, undefined);
      assert.equal(env[names.https], proxy);
      assert.equal(acceptanceProxy(parent)?.server, proxy);
      for (const target of [ORIGIN, proxy]) {
        const result = await runFleetctl(["--json", "--api", target, "whoami"], {env});
        assert.equal(result.code, 3, result.stderr);
        assert.equal(Schema.decodeUnknownSync(failure)(JSON.parse(result.stderr)).error.code, "UZ-AUTH-022");
      }
      assert.deepEqual(requests.splice(0), [
        {url: `${ORIGIN}/v1/users/me`, authorization: `Bearer ${STUB_TOKEN}`},
        {url: "/v1/users/me", authorization: `Bearer ${STUB_TOKEN}`},
      ]);
    }
  } finally {
    await new Promise<void>(resolve => server.close(() => resolve()));
    await state.cleanup();
  }
});
