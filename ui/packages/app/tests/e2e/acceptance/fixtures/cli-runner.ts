import * as fs from "node:fs/promises";
import { existsSync } from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const THIS_DIR = path.dirname(fileURLToPath(import.meta.url));
const WORKTREE_ROOT = path.resolve(THIS_DIR, "../../../../../../..");
// The one CLI the whole suite runs: the workflow-built dist bundle. Preflight
// (fixtures/preflight.ts) checks the same path so a missing build stops the
// job before any browser journey, and this module double-checks at spawn time
// so a directly-invoked spec fails with the recovery step instead of node's
// bare "Cannot find module".
export const AGENTSFLEET_CLI_ENTRY = path.join(WORKTREE_ROOT, "cli/dist/bin/agentsfleet.js");
export const CLI_ARTIFACT_MISSING_DIAGNOSIS =
  `CLI artifact missing at ${AGENTSFLEET_CLI_ENTRY}. ` +
  "Build it first: cd cli && bun install && bun run build.";
const DEFAULT_TIMEOUT_MS = 60_000;
const STATE_DIR_NAME = "agentsfleet";

export interface SpawnResult {
  stdout: string;
  stderr: string;
  code: number;
}

export function cliEnv(env: Record<string, string>): Record<string, string> {
  return {
    AGENTSFLEET_TELEMETRY_DISABLED: "1",
    NO_COLOR: "1",
    ...env,
  };
}

export async function makeCliStateDir(prefix: string): Promise<{ root: string; stateDir: string }> {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), prefix));
  return { root, stateDir: path.join(root, STATE_DIR_NAME) };
}

export async function spawnAgentsfleet(
  args: string[],
  env: Record<string, string>,
  timeoutMs = DEFAULT_TIMEOUT_MS,
): Promise<SpawnResult> {
  if (!existsSync(AGENTSFLEET_CLI_ENTRY)) {
    return Promise.reject(new Error(CLI_ARTIFACT_MISSING_DIAGNOSIS));
  }
  return new Promise((resolve, reject) => {
    const childEnv: NodeJS.ProcessEnv = { ...process.env, ...env };
    delete childEnv.FORCE_COLOR;
    const child = spawn(process.execPath, [AGENTSFLEET_CLI_ENTRY, ...args], {
      env: childEnv,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGTERM");
    }, timeoutMs);
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
    child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({
        stdout: Buffer.concat(stdout).toString("utf8"),
        stderr: Buffer.concat(stderr).toString("utf8"),
        code: timedOut ? 124 : code ?? 1,
      });
    });
  });
}

export async function writeCliState(
  stateDir: string,
  workspaceId: string,
  token: string,
  apiUrl: string,
  workspaceName: string,
): Promise<void> {
  await fs.mkdir(stateDir, { recursive: true });
  await fs.writeFile(
    path.join(stateDir, "workspaces.json"),
    JSON.stringify(
      {
        current_workspace_id: workspaceId,
        items: [{ workspace_id: workspaceId, name: workspaceName, created_at: null }],
      },
      null,
      2,
    ),
  );
  await fs.writeFile(
    path.join(stateDir, "credentials.json"),
    JSON.stringify(
      { token, api_url: apiUrl, saved_at: Date.now(), session_id: null },
      null,
      2,
    ),
  );
}
