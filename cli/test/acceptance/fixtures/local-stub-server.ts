/**
 * Tiny local HTTP server for tests that need a backend "shape" but not a
 * real one — `agentsfleet login` SIGINT semantics, --no-open behavior, etc.
 *
 * Responds to:
 *   POST /v1/auth/sessions             → { session_id, login_url, status: "pending" }
 *   GET  /v1/auth/sessions/{id}        → { status: "pending" } (forever — caller decides when to interrupt)
 *   GET  /v1/users/me                  → { scopes } when the policy sets `scopes` (counted)
 *
 * Everything else returns 404. No persistence, no auth, no concurrency
 * concerns — each test gets its own server bound to an ephemeral port.
 */

import http from "node:http";
import type { AddressInfo } from "node:net";

export interface LocalStubPolicy {
  readonly sessionId?: string;
  readonly loginUrl?: string;
  readonly pollStatus?: string;
  // When set, the identity read answers with these scopes; scoped help reads them.
  readonly scopes?: ReadonlyArray<string>;
}

export interface LocalStubHandle {
  readonly baseUrl: string;
  // How many identity reads the server has answered.
  identityReads(): number;
  close(): Promise<void>;
}

export async function startLocalStubServer(opts?: LocalStubPolicy): Promise<LocalStubHandle> {
  const policy: LocalStubPolicy = opts ?? {};
  let identityReads = 0;
  const server = http.createServer((req, res) => {
    const reqUrl = req.url ?? "";
    if (req.method === "GET" && reqUrl === "/v1/users/me" && policy.scopes !== undefined) {
      identityReads += 1;
      respondJson(res, 200, { scopes: policy.scopes });
      return;
    }
    if (req.method === "POST" && reqUrl === "/v1/auth/sessions") {
      respondJson(res, 200, {
        session_id: policy.sessionId ?? "sess_stub_local",
        login_url: policy.loginUrl ?? "http://127.0.0.1:65535/cli-auth/stub",
        status: "pending",
      });
      return;
    }
    if (req.method === "GET" && /^\/v1\/auth\/sessions\/[^/]+$/.test(reqUrl)) {
      respondJson(res, 200, { status: policy.pollStatus ?? "pending" });
      return;
    }
    respondJson(res, 404, { error_code: "NOT_FOUND", detail: `no route: ${req.method} ${reqUrl}` });
  });

  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve());
  });

  const address = server.address() as AddressInfo;
  const baseUrl = `http://127.0.0.1:${address.port}`;

  return {
    baseUrl,
    identityReads: () => identityReads,
    close: () => new Promise<void>((resolve) => server.close(() => resolve())),
  };
}

function respondJson(res: http.ServerResponse, status: number, body: unknown): void {
  const payload = JSON.stringify(body);
  res.writeHead(status, {
    "content-type": "application/json",
    "content-length": Buffer.byteLength(payload),
  });
  res.end(payload);
}
