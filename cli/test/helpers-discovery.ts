// A stub deployment for scoped-help tests: it answers the identity read with
// the scopes a test chooses per credential, and every other route with the
// 403 a server sends for a missing scope.

import fs from "node:fs/promises";
import path from "node:path";

import { runCli } from "../src/cli.ts";
import { CLI_CREDENTIAL_PREFIX } from "../src/constants/cli-credential.ts";
import { bufferStream, cliEnv, withFreshStateDir } from "./helpers-cli-state.ts";

export const USERS_ME = "/v1/users/me";
export const ENV_KEY = "agt_t_discovery";
export const STORED_TOKEN = `${CLI_CREDENTIAL_PREFIX}${"a".repeat(64)}`;
export const EXIT_OK = 0;
export const EXIT_USAGE = 4;
export const EXIT_SERVER = 3;

export interface Call {
  readonly method: string;
  readonly origin: string;
  readonly path: string;
  readonly bearer: string | null;
}

export interface Setup {
  readonly argv: ReadonlyArray<string>;
  readonly env?: Record<string, string>;
  // A stored login and the deployment it was minted against (null: unbound).
  readonly stored?: { readonly apiUrl: string | null };
  readonly scopes?: (bearer: string | null) => unknown;
  readonly fetchOverride?: (url: string, init?: RequestInit) => Promise<Response>;
  readonly discoveryTimeoutMs?: number;
}

export interface Outcome {
  readonly code: number;
  readonly out: string;
  readonly err: string;
  readonly calls: ReadonlyArray<Call>;
}

const forbidden = () =>
  new Response(
    JSON.stringify({ error_code: "UZ-AUTH-022", user_message: "Missing platform key read scope" }),
    { status: 403, headers: { "content-type": "application/json" } },
  );

const bearerOf = (init?: RequestInit): string | null => {
  const header = new Headers(init?.headers).get("authorization");
  return header?.startsWith("Bearer ") ? header.slice("Bearer ".length) : null;
};

export const run = (setup: Setup): Promise<Outcome> =>
  withFreshStateDir(async (dir) => {
    if (setup.stored !== undefined) {
      await fs.writeFile(
        path.join(dir, "credentials.json"),
        JSON.stringify({ token: STORED_TOKEN, saved_at: Date.now(), session_id: null, api_url: setup.stored.apiUrl, credential_id: "cred-1" }),
        { mode: 0o600 },
      );
    }
    const calls: Call[] = [];
    const fetchImpl = async (url: string, init?: RequestInit) => {
      const parsed = new URL(url);
      calls.push({ method: (init?.method ?? "GET").toUpperCase(), origin: parsed.origin, path: parsed.pathname, bearer: bearerOf(init) });
      if (setup.fetchOverride) return setup.fetchOverride(url, init);
      if (parsed.pathname === USERS_ME) return Response.json(setup.scopes?.(bearerOf(init)) ?? { scopes: [] });
      return forbidden();
    };
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli([...setup.argv], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", ...setup.env }),
      fetchImpl: fetchImpl as unknown as typeof fetch,
      ...(setup.discoveryTimeoutMs !== undefined ? { discoveryTimeoutMs: setup.discoveryTimeoutMs } : {}),
    });
    return { code, out: out.read(), err: err.read(), calls };
  });

export const withKey = (scopes: ReadonlyArray<unknown>, argv: ReadonlyArray<string>): Promise<Outcome> =>
  run({ argv, env: { AGENTSFLEET_API_KEY: ENV_KEY }, scopes: () => ({ scopes }) });

export const rows = (help: string): ReadonlyArray<string> => {
  const at = help.indexOf("SUBCOMMANDS");
  if (at === -1) return [];
  return help.slice(at).split("\n").flatMap((line) => {
    const match = /^ {2}([a-z][a-z-]*)(?:\s{2,}|$)/.exec(line);
    return match?.[1] === undefined ? [] : [match[1]];
  });
};

export const identityReads = (calls: ReadonlyArray<Call>): ReadonlyArray<Call> =>
  calls.filter((call) => call.path === USERS_ME);
