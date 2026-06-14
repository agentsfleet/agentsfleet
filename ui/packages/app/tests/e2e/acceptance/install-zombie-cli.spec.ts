/**
 * install-zombie-cli.spec.ts — canonical install path.
 *
 * Spawns `agentsfleet install --from <bundle>` against the local agentsfleetd with
 * the fixture user's session JWT, then asserts the dashboard renders the
 * row with `data-state="live"`. This is the real user install flow — the
 * dashboard's <FirstInstallCard> hands users a CLI command, not a button.
 *
 * Wire:
 *   - Per-test temp directory under `os.tmpdir()` holds:
 *       * TRIGGER.md + SKILL.md   (the bundle for `--from <path>`)
 *       * agentsfleet/credentials.json + workspaces.json   (CLI auth state)
 *   - `ZOMBIE_STATE_DIR=<tmpdir>/agentsfleet` points the CLI at that state.
 *   - `ZOMBIE_TOKEN=<fixture.sessionJwt>` populates the Bearer header.
 *     (The env var name is `ZOMBIE_TOKEN`, not `ZOMBIECTL_TOKEN` — see
 *      `agentsfleet/src/cli.js:65`.)
 *   - `ZOMBIE_API_URL=$NEXT_PUBLIC_API_URL` so the CLI and the
 *     workspace-id fetch hit the same agentsfleetd (mismatched URLs land at 404).
 *
 * No `signInAs` cookie-mount, no per-page DOM auth — just agentsfleet + a
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
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

// Worktree root, derived from this file's path. This file lives at
// `ui/packages/app/tests/e2e/acceptance/install-zombie-cli.spec.ts`; the
// worktree root is six levels up from its containing directory.
const THIS_DIR = path.dirname(fileURLToPath(import.meta.url));
const WORKTREE_ROOT = path.resolve(THIS_DIR, "../../../../../..");
const ZOMBIECTL_BIN = path.join(WORKTREE_ROOT, "agentsfleet/dist/bin/agentsfleet.js");

function loadFixtureCache(): Record<string, { sessionJwt: string }> {
  const cachePath = path.join(process.cwd(), ".fixture-jwts.json");
  return JSON.parse(fsSync.readFileSync(cachePath, "utf8"));
}

function triggerMd(name: string): string {
  return [
    "---",
    `name: ${name}`,
    "",
    "x-usezombie:",
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

function skillMd(name: string): string {
  return [
    "---",
    `name: ${name}`,
    "description: Fixture skill body for the install-cli e2e spec.",
    "version: 0.1.0",
    "---",
    "",
    `# ${name}`,
    "",
    "Body for fixture zombie installed via agentsfleet.",
    "",
  ].join("\n");
}

interface SpawnResult {
  stdout: string;
  stderr: string;
  code: number;
}

async function spawnZombiectl(args: string[], env: Record<string, string>): Promise<SpawnResult> {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [ZOMBIECTL_BIN, ...args], {
      env: { ...process.env, ...env },
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

test.describe("install-zombie-cli", () => {
  test("agentsfleet install lands a row on /zombies with live state", async ({ page }) => {
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

    // Bundle + state dir under a per-test tmpdir so concurrent runs do not
    // collide and so cleanup is automatic (Playwright wipes test artifacts
    // between runs).
    const tmpRoot = await fs.mkdtemp(path.join(os.tmpdir(), "m64-005-install-cli-"));
    const bundleDir = path.join(tmpRoot, "bundle");
    const stateDir = path.join(tmpRoot, "agentsfleet");
    await fs.mkdir(bundleDir, { recursive: true });
    await fs.mkdir(stateDir, { recursive: true });
    await fs.writeFile(path.join(bundleDir, "TRIGGER.md"), triggerMd(name));
    await fs.writeFile(path.join(bundleDir, "SKILL.md"), skillMd(name));
    // workspaces.json pins the install target. credentials.json wires
    // the Bearer token (agentsfleet prefers it to ZOMBIE_TOKEN; both work).
    await fs.writeFile(
      path.join(stateDir, "workspaces.json"),
      JSON.stringify(
        { items: [{ workspace_id: ws, name: null }], current_workspace_id: ws },
        null,
        2,
      ),
    );
    await fs.writeFile(
      path.join(stateDir, "credentials.json"),
      JSON.stringify({ token, api_url: apiUrl }, null, 2),
    );

    const result = await spawnZombiectl(["--json", "install", "--from", bundleDir], {
      ZOMBIE_STATE_DIR: stateDir,
      ZOMBIE_API_URL: apiUrl,
      ZOMBIE_TOKEN: token,
      NO_COLOR: "1",
    });
    if (result.code !== 0) {
      throw new Error(
        `agentsfleet install failed (exit ${result.code}):\nstdout: ${result.stdout}\nstderr: ${result.stderr}`,
      );
    }
    const payload = JSON.parse(result.stdout) as { zombie_id: string; status: string };
    expect(payload.status).toBe("installed");
    expect(payload.zombie_id).toBeTruthy();

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/zombies");
    await expect(page).toHaveURL(/\/zombies(\?|$)/);

    const row = page.locator(`a[href="/zombies/${payload.zombie_id}"]`);
    await expect(row).toBeVisible();
    await expect(row).toHaveAttribute("data-state", "live");
    await expect(row.getByText(name)).toBeVisible();

    // The tmpRoot is left for Playwright's artifact retention; no manual cleanup.
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);
  });
});
