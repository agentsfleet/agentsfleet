import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { CliCredentialRevokeFailed, REVOKE_LABEL } from "./cli-credential-revoke.ts";
import {
  hydrateWorkspacesForToken,
  mintCliCredential,
  revokeHydratedCliCredentials,
} from "./workspace-hydration.ts";
import { MAX_ATTEMPTS } from "./retryable.ts";

const API_URL = "https://api.test";
const SESSION_TOKEN = "fixture-session-token";
const TENANT_ID = "tenant_fixture";
const WORKSPACE_ID = "workspace_fixture";
const WORKSPACE_NAME = "Fixture workspace";
const CREATED_AT = "2026-07-27T17:06:40Z";
const CLI_CREDENTIAL_ID = "01989abc-def0-7123-8abc-def012345678";
const SECOND_CLI_CREDENTIAL_ID = "01989abc-def0-7123-8abc-def012345679";
const CLI_CREDENTIAL = `afc_${"a".repeat(64)}`;
const MACHINE_NAME = "acceptance-retry";
const SECOND_MACHINE_NAME = "acceptance-retry-2";
const ANSWER_DETAIL = "temporary failure";
const HTTP_METHOD_POST = "POST";
const HTTP_NO_CONTENT = 204;
const HTTP_FORBIDDEN = 403;
const HTTP_SERVICE_UNAVAILABLE = 503;
const HTTP_OK = 200;
const HTTP_UNAUTHORIZED = 401;
const HTTP_BAD_GATEWAY = 502;
const HTTP_GATEWAY_TIMEOUT = 504;
/** What the Cloudflare edge answers for a daemon that is not up yet. */
const EDGE_PAGE = "<html><head><title>502 Bad Gateway</title></head></html>";
/** The un-jittered first wait: `retryable.ts`'s 500 ms base at `random() = 0.5`. */
const FIRST_BACKOFF_MS = 500;
/** Every test leaves the credential map empty, so the hook never waits. */
const NO_WAIT = async (): Promise<void> => {};
const NO_JITTER = (): number => 0.5;
const REVOKE_OPTIONS = { sleep: NO_WAIT, random: NO_JITTER };
const STILL_PENDING = "still pending";
const SETTLE_WINDOW_MS = 20;

let originalFetch: typeof globalThis.fetch;
let stateDir = "";

function installResponses(...bodies: ReadonlyArray<unknown>): void {
  let index = 0;
  globalThis.fetch = Object.assign(
    async (): Promise<Response> => Response.json(bodies[index++] ?? {}),
    { preconnect: originalFetch.preconnect },
  );
}

/**
 * Mints ids in order on POST; answers each credential's DELETEs from its own
 * list, the last answer repeating. Returns the attempts made per id.
 */
function installRevokeAnswers(
  answersById: Readonly<Record<string, ReadonlyArray<number>>>,
): Readonly<Record<string, number>> {
  const ids = Object.keys(answersById);
  const attempts: Record<string, number> = {};
  let minted = 0;
  globalThis.fetch = Object.assign(
    async (input: string | URL | Request, init?: RequestInit): Promise<Response> => {
      if (init?.method === HTTP_METHOD_POST) {
        return Response.json({ id: ids[minted++], credential: CLI_CREDENTIAL });
      }
      const id = decodeURIComponent(String(input).split("/").pop() ?? "");
      const answers = answersById[id] ?? [];
      const answer = answers[Math.min(attempts[id] ?? 0, answers.length - 1)];
      attempts[id] = (attempts[id] ?? 0) + 1;
      if (answer === undefined) throw new Error(`no revoke answer scripted for ${id}`);
      return answer === HTTP_NO_CONTENT
        ? new Response(null, { status: answer })
        : new Response(ANSWER_DETAIL, { status: answer });
    },
    { preconnect: originalFetch.preconnect },
  );
  return attempts;
}

beforeEach(async () => {
  originalFetch = globalThis.fetch;
  stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-hydration-"));
});

afterEach(async () => {
  try {
    await revokeHydratedCliCredentials(API_URL, REVOKE_OPTIONS);
  } finally {
    globalThis.fetch = originalFetch;
    await fs.rm(stateDir, { recursive: true, force: true });
  }
});

describe("workspace fixture hydration", () => {
  it("persists tenant identity with the normalized workspace list", async () => {
    installResponses(
      {
        tenant_id: TENANT_ID,
        items: [{
          workspace_id: WORKSPACE_ID,
          name: WORKSPACE_NAME,
          created_at: CREATED_AT,
        }],
      },
      { id: CLI_CREDENTIAL_ID, credential: CLI_CREDENTIAL },
    );

    await hydrateWorkspacesForToken({
      apiUrl: API_URL,
      token: SESSION_TOKEN,
      stateDir,
    });

    const persisted = JSON.parse(
      await fs.readFile(path.join(stateDir, "workspaces.json"), "utf8"),
    ) as Record<string, unknown>;
    expect(persisted.tenant_id).toBe(TENANT_ID);
    expect(persisted.current_workspace_id).toBe(WORKSPACE_ID);

    const credentials = JSON.parse(
      await fs.readFile(path.join(stateDir, "credentials.json"), "utf8"),
    ) as Record<string, unknown>;
    expect(credentials.token).toBe(CLI_CREDENTIAL);
    expect(credentials.credential_id).toBe(CLI_CREDENTIAL_ID);
  });

  it("refuses a workspace response without tenant identity", async () => {
    installResponses({
      items: [{ workspace_id: WORKSPACE_ID, name: WORKSPACE_NAME }],
    });

    await expect(hydrateWorkspacesForToken({
      apiUrl: API_URL,
      token: SESSION_TOKEN,
      stateDir,
    })).rejects.toThrow("response missing tenant_id");
  });
});

describe("revoking the credentials this process minted", () => {
  it("forgets a credential once the API has revoked it", async () => {
    const attempts = installRevokeAnswers({ [CLI_CREDENTIAL_ID]: [HTTP_NO_CONTENT] });
    await mintCliCredential(API_URL, SESSION_TOKEN, MACHINE_NAME);

    await revokeHydratedCliCredentials(API_URL, REVOKE_OPTIONS);
    await revokeHydratedCliCredentials(API_URL, REVOKE_OPTIONS);

    expect(attempts[CLI_CREDENTIAL_ID]).toBe(1);
  });

  it("keeps a credential whose revoke gave up, for a later revoke", async () => {
    const attempts = installRevokeAnswers({ [CLI_CREDENTIAL_ID]: [HTTP_SERVICE_UNAVAILABLE] });
    await mintCliCredential(API_URL, SESSION_TOKEN, MACHINE_NAME);

    await expect(revokeHydratedCliCredentials(API_URL, REVOKE_OPTIONS))
      .rejects.toThrow(`answered ${HTTP_SERVICE_UNAVAILABLE}`);

    const recovered = installRevokeAnswers({ [CLI_CREDENTIAL_ID]: [HTTP_NO_CONTENT] });
    await revokeHydratedCliCredentials(API_URL, REVOKE_OPTIONS);
    expect(attempts[CLI_CREDENTIAL_ID]).toBeGreaterThan(1);
    expect(recovered[CLI_CREDENTIAL_ID]).toBe(1);
  });

  it("waits for a sibling still retrying before it reports a failure", async () => {
    const attempts = installRevokeAnswers({
      [CLI_CREDENTIAL_ID]: [HTTP_FORBIDDEN],
      [SECOND_CLI_CREDENTIAL_ID]: [HTTP_SERVICE_UNAVAILABLE, HTTP_NO_CONTENT],
    });
    await mintCliCredential(API_URL, SESSION_TOKEN, MACHINE_NAME);
    await mintCliCredential(API_URL, SESSION_TOKEN, SECOND_MACHINE_NAME);
    // The second credential's backoff sleeps until the test opens the gate.
    const gate = { open: (): void => {} };
    const backoff = new Promise<void>((resolve) => { gate.open = resolve; });

    const outcome = revokeHydratedCliCredentials(API_URL, { sleep: () => backoff, random: NO_JITTER })
      .then(() => null, (error: unknown) => error);

    const early = await Promise.race([outcome, Bun.sleep(SETTLE_WINDOW_MS).then(() => STILL_PENDING)]);
    expect(early).toBe(STILL_PENDING);
    expect(attempts[CLI_CREDENTIAL_ID]).toBe(1);
    expect(attempts[SECOND_CLI_CREDENTIAL_ID]).toBe(1);

    gate.open();
    const failure = await outcome;
    expect(failure).toBeInstanceOf(CliCredentialRevokeFailed);
    expect((failure as CliCredentialRevokeFailed).message).toContain(`answered ${HTTP_FORBIDDEN}`);
    expect(attempts[SECOND_CLI_CREDENTIAL_ID]).toBe(2);

    const remaining = installRevokeAnswers({
      [CLI_CREDENTIAL_ID]: [HTTP_NO_CONTENT],
      [SECOND_CLI_CREDENTIAL_ID]: [HTTP_NO_CONTENT],
    });
    await revokeHydratedCliCredentials(API_URL, REVOKE_OPTIONS);
    expect(remaining[CLI_CREDENTIAL_ID]).toBe(1);
    expect(remaining[SECOND_CLI_CREDENTIAL_ID]).toBeUndefined();
  });

  it("aggregates the failures when more than one credential stays live", async () => {
    installRevokeAnswers({
      [CLI_CREDENTIAL_ID]: [HTTP_FORBIDDEN],
      [SECOND_CLI_CREDENTIAL_ID]: [HTTP_FORBIDDEN],
    });
    await mintCliCredential(API_URL, SESSION_TOKEN, MACHINE_NAME);
    await mintCliCredential(API_URL, SESSION_TOKEN, SECOND_MACHINE_NAME);

    const failure = await revokeHydratedCliCredentials(API_URL, REVOKE_OPTIONS)
      .then(() => null, (error: unknown) => error);

    expect(failure).toBeInstanceOf(AggregateError);
    expect((failure as AggregateError).errors).toHaveLength(2);
    expect((failure as AggregateError).message).toContain(`${REVOKE_LABEL}: 2 credentials`);

    installRevokeAnswers({
      [CLI_CREDENTIAL_ID]: [HTTP_NO_CONTENT],
      [SECOND_CLI_CREDENTIAL_ID]: [HTTP_NO_CONTENT],
    });
    await revokeHydratedCliCredentials(API_URL, REVOKE_OPTIONS);
  });
});

/** Everything one mint call did, compared whole so no field passes unread. */
interface MintOutcome {
  readonly minted: { readonly id: string; readonly credential: string } | null;
  readonly error: string | null;
  readonly calls: number;
  readonly slept: ReadonlyArray<number>;
}

/**
 * Mints against a fetch stub that answers each POST from `statuses` in order,
 * the last repeating, 200 minting; the backoff is recorded, never waited.
 */
async function attemptMint(statuses: ReadonlyArray<number>): Promise<MintOutcome> {
  let calls = 0;
  const slept: number[] = [];
  globalThis.fetch = Object.assign(
    async (): Promise<Response> => {
      const status = statuses[Math.min(calls, statuses.length - 1)] ?? HTTP_OK;
      calls += 1;
      if (status === HTTP_OK) {
        return Response.json({ id: CLI_CREDENTIAL_ID, credential: CLI_CREDENTIAL });
      }
      return new Response(EDGE_PAGE, { status });
    },
    { preconnect: originalFetch.preconnect },
  );
  const options = { sleep: async (ms: number): Promise<void> => { slept.push(ms); }, random: NO_JITTER };
  try {
    const minted = await mintCliCredential(API_URL, SESSION_TOKEN, MACHINE_NAME, options);
    return { minted, error: null, calls, slept };
  } catch (error: unknown) {
    return { minted: null, error: (error as Error).message, calls, slept };
  }
}

/** The un-jittered schedule after the first `n` failed attempts: 500, 1000, 2000, 4000 ms. */
function backoffAfter(n: number): number[] {
  return Array.from({ length: n }, (_, attempt) => FIRST_BACKOFF_MS * 2 ** attempt);
}

describe("minting a CLI credential through an edge that is not ready", () => {
  it("mints after an edge 502", async () => {
    expect(await attemptMint([HTTP_BAD_GATEWAY, HTTP_OK])).toEqual({
      minted: { id: CLI_CREDENTIAL_ID, credential: CLI_CREDENTIAL },
      error: null,
      calls: 2,
      slept: backoffAfter(1),
    });
  });

  it("throws after five retryable answers", async () => {
    expect(await attemptMint([HTTP_SERVICE_UNAVAILABLE])).toEqual({
      minted: null,
      error: `CLI credential mint ${HTTP_SERVICE_UNAVAILABLE}: ${EDGE_PAGE} (gave up after ${MAX_ATTEMPTS} attempts)`,
      calls: MAX_ATTEMPTS,
      slept: backoffAfter(MAX_ATTEMPTS - 1),
    });
  });

  it("does not retry a rejection", async () => {
    expect(await attemptMint([HTTP_UNAUTHORIZED])).toEqual({
      minted: null,
      error: `CLI credential mint ${HTTP_UNAUTHORIZED}: ${EDGE_PAGE}`,
      calls: 1,
      slept: [],
    });
  });

  it("does not retry a gateway timeout, which may follow a mint that landed", async () => {
    expect(await attemptMint([HTTP_GATEWAY_TIMEOUT])).toEqual({
      minted: null,
      error: `CLI credential mint ${HTTP_GATEWAY_TIMEOUT}: ${EDGE_PAGE}`,
      calls: 1,
      slept: [],
    });
  });
});
