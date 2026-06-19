// End-to-end config-precedence coverage observed AT THE WIRE.
//
// The pure resolvers are already unit/integration-tested in isolation and
// this file deliberately does NOT re-walk them:
//   - test/services-http-client.unit.test.ts → resolveToken (the pure
//     env-API-key-vs-stored-login precedence picker).
//   - test/api-url-resolution.integration.test.ts → the 16-case
//     (--api / AGENTSFLEET_API_URL / API_URL / creds.api_url / default)
//     API-URL matrix, driven through `doctor` /healthz.
//
// What this file pins instead is the *cross-layer* behaviour a pure resolver
// test can't see — what the other command surfaces actually emit once every
// layer composes (cli.ts global resolve → ctx → Effect CliConfig override →
// workspace-guards.resolveAuthToken → HttpClient). The mock records every
// inbound request (host + Authorization header + path), so each precedence
// rung is asserted on the actual side effect.
//
//   (a) API URL: flag > AGENTSFLEET_API_URL env > API_URL env > creds.api_url
//       > default — observed on an *authed, workspace-scoped* command (`list`,
//       cli-tree-agent.ts) via the inbound Host header, not just the `doctor`
//       probe. (resolveGlobalApiUrl in cli.ts L91, normalizeApiUrl in url.ts.)
//   (b) Auth token: which Bearer the CLI actually sends. The headline
//       behaviour is non-obvious: the Effect-layer
//       `resolveToken(config.accessToken, storedToken)`
//       (services/http-client.ts) is env-first, so the env-slot service
//       API key (AGENTSFLEET_API_KEY) WINS over the on-disk login token at
//       the wire. The on-disk login token reaches the wire only when no env
//       API key is set.
//   (c) Active workspace: the --workspace-id flag overrides the persisted
//       current_workspace_id in the request path; absent the flag the
//       persisted id is used.
//
// `list` makes exactly one `http.request` (no client-side pagination loop —
// src/commands/agent_list.ts), so every test pins `calls` to length 1: the
// side-effect ledger is asserted in full, not just `calls[0]`, so a stray
// retry or duplicate request can't hide behind a first-element check.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { saveCredentials, saveWorkspaces } from "../src/lib/state.ts";
import { bufferStream, withAuthedStateDir, withFreshStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

// `list` (top-level, cli-tree-agent.ts) is the authed, workspace-scoped
// surface under test — it GETs /v1/workspaces/<wsId>/agents with a Bearer
// header and honours --workspace-id.
const LIST = "list" as const;
const FLAG_API = "--api" as const;
const FLAG_WORKSPACE_ID = "--workspace-id" as const;

// Persisted (current_workspace_id) vs the --workspace-id override target.
// Both are real uuidv7 values — parseIdOption (src/program/validators.ts)
// rejects anything else, so a malformed id would fail before the wire.
const WS_PERSISTED = "01900000-0000-7000-8000-0000000ab1de";
const WS_OVERRIDE = "01900000-0000-7000-8000-0000000fffff";

const DISK_TOKEN = "disk.payload.sig";
const ENV_API_KEY = "agt_t_envkey";

// A creds.api_url that can never answer: if a precedence rung wrongly lets it
// win, the request never reaches the mock and `calls` is empty — a louder,
// more specific failure than a wrong-host assertion.
const UNROUTABLE_CREDS_URL = "http://127.0.0.1:1/stale-creds";
// An AGENTSFLEET_API_URL / API_URL value that must lose to a higher rung.
// Same unroutable trick: if it wrongly wins, the mock sees nothing.
const UNROUTABLE_ENV_URL = "http://127.0.0.1:1/stale-env";

const AUTH_HEADER = "authorization" as const;
const HOST_HEADER = "host" as const;
const BEARER_PREFIX = "Bearer " as const;
const EMPTY_LIST = { items: [] } as const;

// Single source of the wire path for a `list` against a workspace — used
// both to register the mock route and to assert the inbound path, so the
// route key and the expectation can never drift apart.
const agentsPath = (wsId: string): string => `/v1/workspaces/${wsId}/agents`;
const bearer = (token: string): string => `${BEARER_PREFIX}${token}`;

// Routes that answer a `list` against an arbitrary workspace id with an
// empty agent set, so the command exits 0 and we can read the side-effect
// ledger rather than an error path.
function listRoutes(...wsIds: ReadonlyArray<string>): MockRoutes {
  const routes: MockRoutes = {};
  for (const wsId of wsIds) {
    routes[`GET ${agentsPath(wsId)}`] = () => jsonResponse(200, EMPTY_LIST);
  }
  return routes;
}

// Seed credentials.json with an explicit api_url. withAuthedStateDir seeds a
// token + workspace; this overwrites the credential so a test can pin the
// api_url rung deterministically.
async function seedCreds(apiUrl: string | null, sessionId: string): Promise<void> {
  await saveCredentials({
    token: DISK_TOKEN,
    saved_at: Date.now(),
    session_id: sessionId,
    api_url: apiUrl,
  });
}

describe("config precedence — API URL at the wire (authed list)", () => {
  test("creds.api_url drives the request host when no --api flag and no env override", async () => {
    await withAuthedStateDir({ workspaceId: WS_PERSISTED, token: DISK_TOKEN }, async () => {
      await withMockApi(listRoutes(WS_PERSISTED), async (apiUrl, calls) => {
        // Persist the mock's URL as creds.api_url, then invoke with an empty
        // env so the only API-URL source is the on-disk credential.
        await seedCreds(apiUrl, "sess_creds_url");
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli([LIST], { stdout: out.stream, stderr: err.stream, env: {} });
        expect(code).toBe(0);
        expect(calls).toHaveLength(1);
        expect(calls[0]?.headers[HOST_HEADER]).toBe(new URL(apiUrl).host);
        expect(calls[0]?.path).toBe(agentsPath(WS_PERSISTED));
      });
    });
  });

  test("AGENTSFLEET_API_URL env beats a stale creds.api_url at the wire", async () => {
    await withAuthedStateDir({ workspaceId: WS_PERSISTED, token: DISK_TOKEN }, async () => {
      await withMockApi(listRoutes(WS_PERSISTED), async (apiUrl, calls) => {
        // creds.api_url is unroutable; AGENTSFLEET_API_URL points at the live
        // mock. Proves the env rung sits above the persisted credential.
        await seedCreds(UNROUTABLE_CREDS_URL, "sess_apiurl_env");
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli([LIST], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: apiUrl },
        });
        expect(code).toBe(0);
        expect(calls).toHaveLength(1);
        expect(calls[0]?.headers[HOST_HEADER]).toBe(new URL(apiUrl).host);
        expect(calls[0]?.path).toBe(agentsPath(WS_PERSISTED));
      });
    });
  });

  test("AGENTSFLEET_API_URL env beats API_URL env at the wire", async () => {
    await withAuthedStateDir({ workspaceId: WS_PERSISTED, token: DISK_TOKEN }, async () => {
      await withMockApi(listRoutes(WS_PERSISTED), async (apiUrl, calls) => {
        // Both env vars present: AGENTSFLEET_API_URL = live mock, API_URL =
        // unroutable. resolveGlobalApiUrl reads AGENTSFLEET_API_URL before
        // API_URL, so the mock must be the one that answers.
        await seedCreds(UNROUTABLE_CREDS_URL, "sess_zenv_over_aenv");
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli([LIST], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: apiUrl, API_URL: UNROUTABLE_ENV_URL },
        });
        expect(code).toBe(0);
        expect(calls).toHaveLength(1);
        expect(calls[0]?.headers[HOST_HEADER]).toBe(new URL(apiUrl).host);
        expect(calls[0]?.path).toBe(agentsPath(WS_PERSISTED));
      });
    });
  });

  test("--api flag beats both env vars and a stale creds.api_url at the wire", async () => {
    await withAuthedStateDir({ workspaceId: WS_PERSISTED, token: DISK_TOKEN }, async () => {
      await withMockApi(listRoutes(WS_PERSISTED), async (apiUrl, calls) => {
        // Every lower rung points somewhere unroutable; only the --api flag
        // points at the live mock. If precedence is wrong the request never
        // lands and `calls` is empty.
        await seedCreds(UNROUTABLE_CREDS_URL, "sess_flag_url");
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli([FLAG_API, apiUrl, LIST], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: UNROUTABLE_ENV_URL, API_URL: UNROUTABLE_ENV_URL },
        });
        expect(code).toBe(0);
        expect(calls).toHaveLength(1);
        expect(calls[0]?.headers[HOST_HEADER]).toBe(new URL(apiUrl).host);
        expect(calls[0]?.path).toBe(agentsPath(WS_PERSISTED));
      });
    });
  });
});

describe("config precedence — auth token Bearer at the wire (authed list)", () => {
  test("env AGENTSFLEET_API_KEY WINS over the on-disk login token at the wire", async () => {
    await withAuthedStateDir({ workspaceId: WS_PERSISTED, token: DISK_TOKEN }, async () => {
      await withMockApi(listRoutes(WS_PERSISTED), async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        // Both credentials present: a stored login JWT on disk (seeded) plus an
        // exported service API key. resolveToken's env-first precedence sends
        // the API key as the wire Bearer, overriding the on-disk login.
        const code = await runCli([LIST], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_API_KEY: ENV_API_KEY },
        });
        expect(code).toBe(0);
        expect(calls).toHaveLength(1);
        expect(calls[0]?.headers[AUTH_HEADER]).toBe(bearer(ENV_API_KEY));
        expect(calls[0]?.headers[AUTH_HEADER]).not.toBe(bearer(DISK_TOKEN));
      });
    });
  });

  test("the on-disk login token reaches the wire when no API key is set", async () => {
    await withAuthedStateDir({ workspaceId: WS_PERSISTED, token: DISK_TOKEN }, async () => {
      await withMockApi(listRoutes(WS_PERSISTED), async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        // Env slot empty → resolveToken falls through to the file-slot login.
        const code = await runCli([LIST], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: apiUrl },
        });
        expect(code).toBe(0);
        expect(calls).toHaveLength(1);
        expect(calls[0]?.headers[AUTH_HEADER]).toBe(bearer(DISK_TOKEN));
      });
    });
  });

  test("AGENTSFLEET_API_KEY authenticates the wire with no on-disk login (machine path)", async () => {
    await withFreshStateDir(async () => {
      // Logged-out on disk (no credentials.json token) but a workspace is
      // selected so `list` is workspace-resolvable. The exported API key is
      // the only credential — the env slot carries it to the wire.
      await saveWorkspaces({
        current_workspace_id: WS_PERSISTED,
        items: [{ workspace_id: WS_PERSISTED, name: "ws", created_at: Date.now() }],
      });
      await withMockApi(listRoutes(WS_PERSISTED), async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli([LIST], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_API_KEY: ENV_API_KEY },
        });
        expect(code).toBe(0);
        expect(calls).toHaveLength(1);
        expect(calls[0]?.headers[AUTH_HEADER]).toBe(bearer(ENV_API_KEY));
      });
    });
  });
});

describe("config precedence — active workspace in the request path (authed list)", () => {
  test("persisted current_workspace_id drives the path when no --workspace-id flag", async () => {
    await withAuthedStateDir({ workspaceId: WS_PERSISTED, token: DISK_TOKEN }, async () => {
      await withMockApi(listRoutes(WS_PERSISTED), async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli([LIST], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: apiUrl },
        });
        expect(code).toBe(0);
        expect(calls).toHaveLength(1);
        expect(calls[0]?.path).toBe(agentsPath(WS_PERSISTED));
      });
    });
  });

  test("--workspace-id flag overrides the persisted current_workspace_id in the path", async () => {
    await withAuthedStateDir({ workspaceId: WS_PERSISTED, token: DISK_TOKEN }, async () => {
      // Mock both ids so a wrong-precedence call still resolves (200) and
      // the assertion — not a 404 — is what fails on regression.
      await withMockApi(listRoutes(WS_PERSISTED, WS_OVERRIDE), async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli([LIST, FLAG_WORKSPACE_ID, WS_OVERRIDE], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: apiUrl },
        });
        expect(code).toBe(0);
        expect(calls).toHaveLength(1);
        expect(calls[0]?.path).toBe(agentsPath(WS_OVERRIDE));
        // The persisted id must NOT have been used.
        expect(calls.every((c) => c.path !== agentsPath(WS_PERSISTED))).toBe(true);
      });
    });
  });
});
