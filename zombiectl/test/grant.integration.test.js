import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.js";
import { saveCredentials, saveWorkspaces } from "../src/lib/state.js";
import { withMockApi, jsonResponse } from "./helpers-mock-api.js";

const WS_ID = "ws_grant_test";
const ZOMBIE_ID = "zmb_grant_test";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

async function withAuthedStateDir(fn) {
  const previous = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-grant-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    await saveCredentials({
      token: "header.payload.sig",
      saved_at: Date.now(),
      session_id: "sess_grant",
      api_url: null,
    });
    await saveWorkspaces({
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "test-ws", created_at: Date.now() }],
    });
    return await fn();
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

describe("grant (integration grant) commands", () => {
  test("`grant list --zombie <id>` GETs the grants for the zombie and prints the table", async () => {
    await withAuthedStateDir(async () => {
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/integration-grants`]:
          () => jsonResponse(200, {
            items: [
              { grant_id: "grant_1", service: "github", status: "approved",
                requested_at: 1700000000000, approved_at: 1700000000500 },
              { grant_id: "grant_2", service: "slack", status: "pending",
                requested_at: 1700000001000, approved_at: null },
            ],
          }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["grant", "list", "--zombie", ZOMBIE_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("github");
        expect(text).toContain("slack");
        expect(text).toContain("grant_1");
        expect(text).toContain("grant_2");
        expect(text).toContain("approved");
        expect(text).toContain("pending");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/integration-grants`,
        ]);
      });
    });
  });

  test("`grant delete --zombie <id> <grant_id>` DELETEs the grant and prints the revocation note", async () => {
    await withAuthedStateDir(async () => {
      const routes = {
        [`DELETE /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/integration-grants/grant_1`]:
          () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["grant", "delete", "--zombie", ZOMBIE_ID, "grant_1"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("grant_1");
        // The CLI's success line names UZ-GRANT-003 to set expectations for
        // what the zombie will see on its next execute attempt.
        expect(text).toContain("UZ-GRANT-003");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `DELETE /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/integration-grants/grant_1`,
        ]);
      });
    });
  });
});
