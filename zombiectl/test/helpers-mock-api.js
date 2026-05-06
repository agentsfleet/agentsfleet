// Loopback HTTP mock for CLI integration tests.
//
// Spawns a real Bun.serve on 127.0.0.1:<random> for the lifetime of `fn`.
// Tests pass ZOMBIE_API_URL=<baseUrl> into runCli — no fetchImpl injection,
// so the request path exercises real globalThis.fetch + request() + http-client.
//
// `routes` maps "METHOD /path" → handler(req, url, body) returning Response.
// `calls` is shared with the test as an ordered log of every request that
// hit the mock — a fully-typed side-effect ledger you can assert against.
//
// See helpers-cli-state.js for the matching ZOMBIE_STATE_DIR scope helper
// and the documented serial-execution assumption that lets these tests
// share `process.env` mutations safely under `bun test`.

export async function withMockApi(routes, fn) {
  const calls = [];
  const server = Bun.serve({
    port: 0,
    hostname: "127.0.0.1",
    fetch: async (req) => {
      const url = new URL(req.url);
      const body = req.body ? await req.text() : null;
      calls.push({
        method: req.method,
        path: url.pathname,
        search: url.search,
        body,
        headers: Object.fromEntries(req.headers),
      });
      const key = `${req.method} ${url.pathname}`;
      const handler = routes[key];
      if (!handler) {
        return new Response(
          JSON.stringify({ error: { code: "NOT_FOUND", message: `no mock route for ${key}` }, request_id: "req_mock_404" }),
          { status: 404, headers: { "content-type": "application/json" } },
        );
      }
      return handler(req, url, body);
    },
  });
  const baseUrl = `http://127.0.0.1:${server.port}`;
  try {
    return await fn(baseUrl, calls);
  } finally {
    server.stop(true);
  }
}

export function jsonResponse(status, payload) {
  return new Response(JSON.stringify(payload), {
    status,
    headers: { "content-type": "application/json" },
  });
}
