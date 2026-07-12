/**
 * install-fleet-cli.spec.ts — current library install path.
 *
 * Uploads a library entry, then runs `agentsfleet install --library <id>`.
 * The fixture uses the command-line interface (CLI) against local `agentsfleetd`.
 * The test then checks that the dashboard renders a live fleet row.
 *
 * Wire:
 *   - Per-test temp directory under `os.tmpdir()` holds:
 *       * agentsfleet/credentials.json + workspaces.json   (CLI auth state)
 *   - `AGENTSFLEET_STATE_DIR=<tmpdir>/agentsfleet` points the CLI at that state.
 *   - `AGENTSFLEET_TOKEN=<fixture.sessionJwt>` populates the Bearer header with a JSON Web Token (JWT).
 *   - `AGENTSFLEET_API_URL=$NEXT_PUBLIC_API_URL` so the CLI and the
 *     workspace-id fetch hit the same agentsfleetd (mismatched URLs land at 404).
 *
 * No `signInAs` cookie-mount, no per-page Document Object Model (DOM) auth — just agentsfleet + a
 * post-install dashboard reload to confirm the row landed.
 */
import * as fs from "node:fs/promises";
import * as fsSync from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

// Worktree root, derived from this file's path. This file lives at
// `ui/packages/app/tests/e2e/acceptance/install-fleet-cli.spec.ts`; the
// worktree root is six levels up from its containing directory.
const THIS_DIR = path.dirname(fileURLToPath(import.meta.url));
const WORKTREE_ROOT = path.resolve(THIS_DIR, "../../../../../..");
const CLI_BIN = path.join(WORKTREE_ROOT, "cli/dist/bin/agentsfleet.js");
const INSTALL_LIBRARY_NAME = "install-cli-fixture";
const INSTALL_STATUS = "installed";
const LIVE_STATE = "live";
const SOURCE_KIND_UPLOAD = "upload";

function loadFixtureCache(): Record<string, { sessionJwt: string }> {
  const cachePath = path.join(process.cwd(), ".fixture-jwts.json");
  return JSON.parse(fsSync.readFileSync(cachePath, "utf8"));
}

function triggerMd(): string {
  return [
    "---",
    `name: ${INSTALL_LIBRARY_NAME}`,
    "",
    "x-agentsfleet:",
    "  triggers:",
    "    - type: cron",
    '      schedule: "0 0 * * *"',
    "  tools:",
    "    - agentmail",
    "  budget:",
    "    daily_dollars: 1.0",
    "---",
    "",
  ].join("\n");
}

function skillMd(): string {
  return [
    "---",
    `name: ${INSTALL_LIBRARY_NAME}`,
    "description: Fixture skill body for the install-cli e2e spec.",
    "version: 0.1.0",
    "---",
    "",
    `# ${INSTALL_LIBRARY_NAME}`,
    "",
    "Body for fixture fleet installed via agentsfleet.",
    "",
  ].join("\n");
}

interface SpawnResult {
  stdout: string;
  stderr: string;
  code: number;
}

async function spawnFleetctl(args: string[], env: Record<string, string>): Promise<SpawnResult> {
  return new Promise((resolve, reject) => {
    const childEnv: NodeJS.ProcessEnv = { ...process.env, ...env };
    delete childEnv.FORCE_COLOR;
    const child = spawn(process.execPath, [CLI_BIN, ...args], {
      env: childEnv,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
    child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
    child.on("error", reject);
    child.on("close", (code) =>
      resolve({
        stdout: Buffer.concat(stdout).toString("utf8"),
        stderr: Buffer.concat(stderr).toString("utf8"),
        code: code ?? 1,
      }),
    );
  });
}

async function writeClientState(stateDir: string, workspaceId: string, token: string, apiUrl: string) {
  await fs.mkdir(stateDir, { recursive: true });
  await fs.writeFile(
    path.join(stateDir, "workspaces.json"),
    JSON.stringify(
      { items: [{ workspace_id: workspaceId, name: null }], current_workspace_id: workspaceId },
      null,
      2,
    ),
  );
  await fs.writeFile(
    path.join(stateDir, "credentials.json"),
    JSON.stringify({ token, api_url: apiUrl }, null, 2),
  );
}

async function onboardLibrary(apiUrl: string, workspaceId: string, token: string): Promise<string> {
  const response = await fetch(
    `${apiUrl}/v1/workspaces/${encodeURIComponent(workspaceId)}/fleet-libraries`,
    {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify({
        source_kind: SOURCE_KIND_UPLOAD,
        skill_markdown: skillMd(),
        trigger_markdown: triggerMd(),
      }),
    },
  );
  if (!response.ok) {
    throw new Error(`library upload failed (${response.status}): ${await response.text()}`);
  }
  const library: unknown = await response.json();
  if (library === null || typeof library !== "object" || !("id" in library)) {
    throw new Error("library upload returned no id");
  }
  const id = library.id;
  if (typeof id !== "string" || id.length === 0) throw new Error("library upload returned no id");
  return id;
}

async function installFixture(
  apiUrl: string,
  workspaceId: string,
  token: string,
  fleetName: string,
): Promise<{ fleet_id: string; status: string }> {
  const tmpRoot = await fs.mkdtemp(path.join(os.tmpdir(), "install-cli-"));
  try {
    const stateDir = path.join(tmpRoot, "agentsfleet");
    await writeClientState(stateDir, workspaceId, token, apiUrl);
    const libraryId = await onboardLibrary(apiUrl, workspaceId, token);
    const result = await spawnFleetctl(
      ["--json", "install", "--library", libraryId, "--name", fleetName],
      {
        AGENTSFLEET_STATE_DIR: stateDir,
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_TOKEN: token,
        AGENTSFLEET_TELEMETRY_DISABLED: "1",
        NO_COLOR: "1",
      },
    );
    if (result.code !== 0) {
      throw new Error(
        `agentsfleet install failed (exit ${result.code}):\nstdout: ${result.stdout}\nstderr: ${result.stderr}`,
      );
    }
    return JSON.parse(result.stdout) as { fleet_id: string; status: string };
  } finally {
    await fs.rm(tmpRoot, { recursive: true, force: true });
  }
}

test.describe("install-fleet-cli", () => {
  test("agentsfleet install lands a row on /w/[workspaceId]/fleets with live state", async ({ page }) => {
    // Drive the CLI and the workspace-id fetch against the SAME agentsfleetd —
    // splitting them lands the install at a 404 (workspace from server A,
    // install to server B) with no clear hint about the URL mismatch.
    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (!apiUrl) throw new Error("NEXT_PUBLIC_API_URL must be set");
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const cache = loadFixtureCache();
    const token = cache[FIXTURE_KEY.regular]?.sessionJwt;
    if (!token) throw new Error("fixture cache missing sessionJwt for 'regular'");

    const tag = Math.random().toString(36).slice(2, 8);
    const name = `install-cli-${tag}`;

    const payload = await installFixture(apiUrl, ws, token, name);
    expect(payload.status).toBe(INSTALL_STATUS);
    expect(payload.fleet_id).toBeTruthy();

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(ws, "fleets"));
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));

    const row = page.locator(`a[href="${workspaceHref(ws, `fleets/${payload.fleet_id}`)}"]`);
    await expect(row).toBeVisible();
    await expect(row).toHaveAttribute("data-state", LIVE_STATE);
    await expect(row.getByText(name)).toBeVisible();
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws);
  });
});
