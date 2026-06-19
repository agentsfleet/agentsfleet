/**
 * agent-key mutation round-trip (live, AGENTSFLEET_TOKEN-injected).
 *
 * Closes the biggest mutation gap the coverage critic flagged: every other
 * write verb got a dedicated live slice, but `agent-key add` / `delete` were
 * only ever exercised in the negative matrices. This walks the happy path:
 *   install agent -> agent-key add --agent <id> --name <prefixed> --json
 *   -> agent-key list --json includes the agent_key_id
 *   -> agent-key delete <id> --json -> deleted
 *   -> agent-key list --json excludes it
 * plus the server-side negative: delete of a well-formed-but-unknown id.
 *
 * Prefix-scoped: the bound agent is ACCEPTANCE_RUN_PREFIX-named and cleaned
 * in afterAll; any minted key is best-effort deleted so a crash can't strand
 * a key in the shared DEV tenant. No assertion claims global emptiness.
 * Live-only; skips cleanly when AGENTSFLEET_ACCEPTANCE_TARGET isn't https.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv, runAgentctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { installPlatformOpsAgent } from "./fixtures/seed.ts";
import { cleanWorkspaceAgents } from "./fixtures/teardown.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

// --- command/flag/key constants (RULE UFS) ---------------------------------
const CMD_AGENT_KEY = "agent-key" as const;
const SUB_ADD = "add" as const;
const SUB_LIST = "list" as const;
const SUB_DELETE = "delete" as const;
const FLAG_AGENT = "--agent" as const;
const FLAG_NAME = "--name" as const;
const FLAG_JSON = "--json" as const;
const KEY_DELETED = "deleted" as const;

const ENV_TOKEN = "AGENTSFLEET_TOKEN" as const;
const ENV_API_URL = "AGENTSFLEET_API_URL" as const;
const ENV_STATE_DIR = "AGENTSFLEET_STATE_DIR" as const;
const ENV_NO_COLOR = "NO_COLOR" as const;
const NO_COLOR_ON = "1" as const;
const STATE_DIR_PREFIX = "agentsfleet-agentkey-" as const;

// Well-formed (uuidv7-shaped) but never-issued id: it must clear the client
// validator and fail server-side, proving the negative reaches the API.
const UNKNOWN_KEY_ID = "01900000-0000-7000-8000-000000000000" as const;
// Verified live against api-dev (2026-06-19): deleting a well-formed but
// never-issued key id is refused with HTTP_404 Not Found (no UZ-AGENT-*
// code in the body — a minor error-registry gap noted in the PR).
const NOT_FOUND_RE = /HTTP_404|Not Found/i;

const keyName = (label: string): string => `${ACCEPTANCE_RUN_PREFIX}-${label}`;

interface AgentKeyRow {
  readonly agent_key_id?: string;
}
interface AgentKeyListEnvelope {
  readonly items?: ReadonlyArray<AgentKeyRow>;
}

function listIncludesId(envelope: AgentKeyListEnvelope, id: string): boolean {
  const rows = Array.isArray(envelope.items) ? envelope.items : [];
  return rows.some((row) => row.agent_key_id === id);
}

if (!isLive) {
  describe("agent-key-mutation.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("agent-key — mint → list → delete round-trip (token injection)", () => {
    let apiUrl = "";
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let agentId = "";
    let mintedKeyId = "";

    async function run(args: ReadonlyArray<string>): Promise<RunResult> {
      const result = await runAgentctl(args, { env, stdin: "" });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
      env = composeEnv({
        [ENV_TOKEN]: sessionJwt,
        [ENV_API_URL]: apiUrl,
        [ENV_STATE_DIR]: stateDir,
        [ENV_NO_COLOR]: NO_COLOR_ON,
      });
      await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });

      const installed = await installPlatformOpsAgent({ env, runPrefix: ACCEPTANCE_RUN_PREFIX });
      const id = installed.id ?? installed.agent_id;
      assert.ok(id, `install missing id: ${JSON.stringify(installed)}`);
      agentId = id as string;
    });

    afterAll(async () => {
      if (mintedKeyId) {
        try { await runAgentctl([CMD_AGENT_KEY, SUB_DELETE, mintedKeyId, FLAG_JSON], { env, stdin: "" }); }
        catch { /* best-effort key cleanup */ }
      }
      try { await cleanWorkspaceAgents(env, { runPrefix: ACCEPTANCE_RUN_PREFIX }); }
      catch { /* best-effort agent cleanup */ }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    it("add mints a key bound to the agent and returns an agent_key_id", async () => {
      const result = await run([CMD_AGENT_KEY, SUB_ADD, FLAG_AGENT, agentId, FLAG_NAME, keyName("roundtrip"), FLAG_JSON]);
      assert.equal(result.code, 0, `add exited ${result.code}: ${result.stderr}`);
      const parsed = JSON.parse(result.stdout.trim()) as AgentKeyRow;
      assert.equal(typeof parsed.agent_key_id, "string", `add missing agent_key_id: ${result.stdout}`);
      mintedKeyId = parsed.agent_key_id as string;
    });

    it("list includes the minted key", async () => {
      const result = await run([CMD_AGENT_KEY, SUB_LIST, FLAG_JSON]);
      assert.equal(result.code, 0, `list exited ${result.code}: ${result.stderr}`);
      const parsed = JSON.parse(result.stdout.trim()) as AgentKeyListEnvelope;
      assert.ok(listIncludesId(parsed, mintedKeyId), `list missing ${mintedKeyId}: ${result.stdout}`);
    });

    it("delete removes the key and list no longer includes it", async () => {
      const del = await run([CMD_AGENT_KEY, SUB_DELETE, mintedKeyId, FLAG_JSON]);
      assert.equal(del.code, 0, `delete exited ${del.code}: ${del.stderr}`);
      const delParsed = JSON.parse(del.stdout.trim()) as Record<string, unknown>;
      assert.equal(delParsed[KEY_DELETED], true, `unexpected delete envelope: ${del.stdout}`);

      const after = await run([CMD_AGENT_KEY, SUB_LIST, FLAG_JSON]);
      const parsed = JSON.parse(after.stdout.trim()) as AgentKeyListEnvelope;
      assert.ok(!listIncludesId(parsed, mintedKeyId), `key still present after delete: ${after.stdout}`);
      mintedKeyId = ""; // already deleted — skip the afterAll re-delete
    });

    it("delete of a well-formed but unknown id is refused server-side", async () => {
      const result = await run([CMD_AGENT_KEY, SUB_DELETE, UNKNOWN_KEY_ID, FLAG_JSON]);
      assert.notEqual(result.code, 0, `expected non-zero for unknown id: ${result.stdout}`);
      assert.match(`${result.stdout}\n${result.stderr}`, NOT_FOUND_RE,
        `expected a not-found refusal; stdout=${result.stdout} stderr=${result.stderr}`);
    });
  });
}
