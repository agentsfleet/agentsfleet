import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { hydrateWorkspacesForToken } from "./workspace-hydration.ts";

const API_URL = "https://api.test";
const SESSION_TOKEN = "fixture-session-token";
const TENANT_ID = "tenant_fixture";
const WORKSPACE_ID = "workspace_fixture";
const WORKSPACE_NAME = "Fixture workspace";
const CREATED_AT = 1_785_172_000_000;
const CLI_CREDENTIAL_ID = "01989abc-def0-7123-8abc-def012345678";
const CLI_CREDENTIAL = `afc_${"a".repeat(64)}`;

let originalFetch: typeof globalThis.fetch;
let stateDir = "";

function installResponses(...bodies: ReadonlyArray<unknown>): void {
  let index = 0;
  globalThis.fetch = Object.assign(
    async (): Promise<Response> => Response.json(bodies[index++] ?? {}),
    { preconnect: originalFetch.preconnect },
  );
}

beforeEach(async () => {
  originalFetch = globalThis.fetch;
  stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-hydration-"));
});

afterEach(async () => {
  globalThis.fetch = originalFetch;
  await fs.rm(stateDir, { recursive: true, force: true });
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
