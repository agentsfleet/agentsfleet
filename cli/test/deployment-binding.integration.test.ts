// Deployment binding, end to end through runCli.
//
// The credential and the deployment that minted it are one fact, and these
// walk the operator journeys where the two can come apart: logging out and
// back in, switching deployments mid-life, and pointing a command somewhere
// the credential was never issued.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import {
  bufferStream,
  withAuthedStateDir,
  withFreshStateDir,
} from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";
import {
  clearCredentials,
  loadCredentials,
  saveCredentials,
} from "../src/lib/state.ts";
import { DEFAULT_API_URL, normalizeApiUrl } from "../src/util/url.ts";
import {
  CLI_CREDENTIAL_BODY_LEN,
  CLI_CREDENTIAL_PREFIX,
} from "../src/constants/cli-credential.ts";

const WS_ID = "01900000-0000-7000-8000-0000005e4e71";
const FLEET_ID = "01900000-0000-7000-8000-0000005e4e72";
const DEV = "https://api-dev.agentsfleet.net";
const CRED = `${CLI_CREDENTIAL_PREFIX}${"a".repeat(CLI_CREDENTIAL_BODY_LEN)}`;
const CRED_2 = `${CLI_CREDENTIAL_PREFIX}${"b".repeat(CLI_CREDENTIAL_BODY_LEN)}`;

// The ladder as `cli.ts` resolves it, with no flag and no environment set:
// stored deployment, else the built-in default.
const inferredTarget = (storedApiUrl: string | null): string =>
  normalizeApiUrl(storedApiUrl ?? DEFAULT_API_URL);

describe("scenario 3 — logout forgets the deployment, so the next bare login is production", () => {
  test("logout clears the stored deployment", async () => {
    await withFreshStateDir(async () => {
      await saveCredentials({
        token: CRED,
        saved_at: Date.now(),
        session_id: "sess_dev",
        api_url: DEV,
        credential_id: "cred_dev",
      });
      expect((await loadCredentials()).api_url).toBe(DEV);

      await clearCredentials();
      expect((await loadCredentials()).api_url).toBeNull();
    });
  });

  test("with the deployment forgotten, an un-flagged invocation resolves to production", async () => {
    await withFreshStateDir(async () => {
      await saveCredentials({
        token: CRED,
        saved_at: Date.now(),
        session_id: "sess_dev",
        api_url: DEV,
        credential_id: "cred_dev",
      });
      await clearCredentials();

      // This is what the next bare `agentsfleet login` would dial: nothing
      // names a target and nothing is stored, so the ladder falls to the
      // built-in default. Intended — logout is a full reset, not a
      // deployment-preserving sign-out.
      const stored = (await loadCredentials()).api_url;
      expect(inferredTarget(stored)).toBe(DEFAULT_API_URL);
    });
  });
});

describe("scenario 4 — switching deployments replaces the whole record", () => {
  test("a second login overwrites token, deployment and credential id together", async () => {
    await withFreshStateDir(async () => {
      await saveCredentials({
        token: CRED,
        saved_at: Date.now(),
        session_id: "sess_prod",
        api_url: DEFAULT_API_URL,
        credential_id: "cred_prod",
      });

      // What `login --api <dev>` persists on success.
      await saveCredentials({
        token: CRED_2,
        saved_at: Date.now(),
        session_id: "sess_dev",
        api_url: DEV,
        credential_id: "cred_dev",
      });

      const after = await loadCredentials();
      expect(after.token).toBe(CRED_2);
      expect(after.api_url).toBe(DEV);
      // KNOWN GAP, pinned deliberately: `cred_prod` is still live on the
      // production deployment and its identifier is gone from this machine,
      // so `logout` can no longer revoke it — only that deployment's
      // dashboard can. Revoking before overwrite needs an HTTP client pinned
      // to the OLD base URL, which is why it is not folded in here.
      expect(after.credential_id).toBe("cred_dev");
      expect(after.credential_id).not.toBe("cred_prod");
    });
  });

  test("after the switch, an un-flagged invocation follows the new deployment", async () => {
    await withFreshStateDir(async () => {
      await saveCredentials({
        token: CRED_2,
        saved_at: Date.now(),
        session_id: "sess_dev",
        api_url: DEV,
        credential_id: "cred_dev",
      });
      const stored = (await loadCredentials()).api_url;
      expect(inferredTarget(stored)).toBe(DEV);
    });
  });
});

describe("scenario 5 — a named target wins, and a wrong pair fails at the server", () => {
  test("--api sends the request to the named deployment even though the credential came from another", async () => {
    await withAuthedStateDir(
      { workspaceId: WS_ID, sessionId: "sess_bind", apiUrl: DEFAULT_API_URL },
      async () => {
        const routes: MockRoutes = {
          [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/events`]: () =>
            jsonResponse(401, {
              title: "Unauthorized",
              error_code: "UZ-AUTH-001",
              detail: "credential not recognised by this deployment",
            }),
        };
        await withMockApi(routes, async (apiUrl, calls) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(["events", FLEET_ID, "--api", apiUrl], {
            stdout: out.stream,
            stderr: err.stream,
            env: {},
          });

          // The guard does NOT refuse: the operator named the target, so the
          // request goes out and the deployment that never issued the
          // credential is the one that rejects it.
          expect(calls.length).toBeGreaterThan(0);
          expect(code).not.toBe(0);

          // And the operator is told where to look.
          const text = `${out.read()}${err.read()}`;
          expect(text).toContain("agentsfleet login");
          expect(text).toContain("AGENTSFLEET_API_URL");
        });
      },
    );
  });
});
