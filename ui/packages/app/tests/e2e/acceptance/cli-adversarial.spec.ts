import * as fs from "node:fs/promises";
import * as path from "node:path";
import { expect, test } from "@playwright/test";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import {
  cliEnv,
  makeCliStateDir,
  spawnAgentsfleet,
  writeCliState,
} from "./fixtures/cli-runner";

const TEMP_DIR_PREFIX = "agentsfleet-cli-adversarial-";
const WORKSPACE_NAME = "cli-adversarial";
const CORRUPT_CREDENTIALS = "{";
const UNREACHABLE_API_URL = "http://127.0.0.1:1";
const NO_RETRY = "1";

function encodeJwtPart(value: unknown): string {
  return Buffer.from(JSON.stringify(value)).toString("base64url");
}

function expiredToken(): string {
  return [
    encodeJwtPart({ alg: "none" }),
    encodeJwtPart({ sub: "e2e-cli-acceptance", exp: 1 }),
    "sig",
  ].join(".");
}

function combinedOutput(result: { stdout: string; stderr: string }): string {
  return `${result.stdout}\n${result.stderr}`;
}

function expectNoRuntimeDump(output: string): void {
  expect(output).not.toMatch(/\n\s+at\s+\S+/);
  expect(output).not.toMatch(/UnhandledPromiseRejection|TypeError|SyntaxError/);
}

test.describe("command line adversarial reads", () => {
  test("expired token, malformed credentials, and API outage fail cleanly", async () => {
    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (!apiUrl) throw new Error("NEXT_PUBLIC_API_URL must be set");

    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const { root, stateDir } = await makeCliStateDir(TEMP_DIR_PREFIX);
    const credentialsPath = path.join(stateDir, "credentials.json");
    const expired = expiredToken();

    try {
      await writeCliState(stateDir, workspaceId, expired, apiUrl, WORKSPACE_NAME);
      const expiredResult = await spawnAgentsfleet(
        ["list", "--workspace-id", workspaceId, "--limit", "1"],
        cliEnv({
          AGENTSFLEET_STATE_DIR: stateDir,
          AGENTSFLEET_API_URL: apiUrl,
          AGENTSFLEET_NO_RETRY: NO_RETRY,
        }),
      );
      const expiredOutput = combinedOutput(expiredResult);
      expect(expiredResult.code).not.toBe(0);
      expect(expiredOutput).toMatch(/agentsfleet login|re-authenticate|unauthorized/i);
      expect(expiredOutput).not.toContain(expired);
      expectNoRuntimeDump(expiredOutput);

      await fs.writeFile(credentialsPath, CORRUPT_CREDENTIALS);
      const malformed = await spawnAgentsfleet(
        ["list", "--workspace-id", workspaceId, "--limit", "1"],
        cliEnv({
          AGENTSFLEET_STATE_DIR: stateDir,
          AGENTSFLEET_API_URL: apiUrl,
        }),
      );
      const malformedOutput = combinedOutput(malformed);
      expect(malformed.code).not.toBe(0);
      expect(malformedOutput).toMatch(/not authenticated|agentsfleet login/i);
      expect(await fs.readFile(credentialsPath, "utf8")).toBe(CORRUPT_CREDENTIALS);
      expectNoRuntimeDump(malformedOutput);

      await writeCliState(stateDir, workspaceId, expired, apiUrl, WORKSPACE_NAME);
      const unreachable = await spawnAgentsfleet(
        ["list", "--workspace-id", workspaceId, "--limit", "1"],
        cliEnv({
          AGENTSFLEET_STATE_DIR: stateDir,
          AGENTSFLEET_API_URL: UNREACHABLE_API_URL,
          AGENTSFLEET_NO_RETRY: NO_RETRY,
        }),
      );
      const unreachableOutput = combinedOutput(unreachable);
      expect(unreachable.code).not.toBe(0);
      expect(unreachableOutput).toContain("cannot reach agentsfleet API");
      expect(unreachableOutput).toContain("AGENTSFLEET_API_URL");
      expect(unreachableOutput).not.toMatch(/not authenticated/i);
      expectNoRuntimeDump(unreachableOutput);
    } finally {
      await fs.rm(root, { recursive: true, force: true }).catch(() => undefined);
    }
  });
});
