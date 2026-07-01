import * as crypto from "node:crypto";
import * as fs from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { clientFor } from "./fixtures/api-client";
import { listWorkspaces } from "./fixtures/seed";
import { installViaUI } from "./fixtures/install-ui";
import {
  expectDetailKilled,
  expectRowState,
  killFleet,
  resumeFleet,
  stopFleet,
} from "./fixtures/lifecycle";
import {
  cliEnv,
  makeCliStateDir,
  spawnAgentsfleet,
  writeCliState,
} from "./fixtures/cli-runner";

const JOURNEY_TIMEOUT_MS = 300_000;
const ACTION_TIMEOUT_MS = 60_000;
const WORKSPACE_ID_PATTERN = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|ws_[A-Za-z0-9_-]+/i;
const TEMP_DIR_PREFIX = "agentsfleet-operator-journey-";

interface CliFleetListResponse {
  items?: Array<{ id?: string; name?: string; status?: string }>;
}

interface ApiKeyListResponse {
  items: Array<{ id: string; key_name: string }>;
}

function uniqueName(prefix: string): string {
  return `${prefix}-${crypto.randomBytes(4).toString("hex")}`;
}

async function clickSidebarLink(page: Page, href: string, destination: RegExp): Promise<void> {
  const link = page.locator(`aside a[href="${href}"]`);
  await expect(link).toBeVisible();
  await link.click();
  await expect(page).toHaveURL(destination, { timeout: ACTION_TIMEOUT_MS });
}

async function closeApiKeyReveal(page: Page): Promise<void> {
  await page.getByRole("button", { name: /done/i }).click();
  await expect(page.getByLabel(/api key value/i)).toHaveCount(0);
}

async function createWorkspaceFromSwitcher(page: Page, name: string): Promise<void> {
  const switcher = page.getByTestId("workspace-switcher");
  await expect(switcher).toBeVisible();
  await switcher.click();
  await page.getByTestId("workspace-new").click();

  const dialog = page.getByRole("dialog", { name: "New workspace" });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("Name (optional)").fill(name);
  await dialog.getByRole("button", { name: "Create workspace" }).click();
  await expect(dialog).toBeHidden({ timeout: ACTION_TIMEOUT_MS });
  await expect(switcher).toContainText(name, { timeout: ACTION_TIMEOUT_MS });
}

async function switchWorkspace(page: Page, name: string): Promise<void> {
  const switcher = page.getByTestId("workspace-switcher");
  await expect(switcher).toBeVisible();
  await switcher.click();
  await page.getByRole("menuitem", { name }).click();
  await expect(switcher).toContainText(name, { timeout: ACTION_TIMEOUT_MS });
}

async function activeWorkspaceIdFromSettings(page: Page): Promise<string> {
  const workspaceSection = page.getByLabel("Workspace", { exact: true });
  await expect(workspaceSection).toBeVisible();
  const text = await workspaceSection.textContent();
  const match = text?.match(WORKSPACE_ID_PATTERN);
  if (!match) {
    throw new Error(`Workspace section did not expose an id: ${text ?? "<empty>"}`);
  }
  return match[0];
}

function combinedOutput(result: { stdout: string; stderr: string }): string {
  return `${result.stdout}\n${result.stderr}`;
}

function expectNoRuntimeDump(output: string): void {
  expect(output).not.toMatch(/\n\s+at\s+\S+/);
  expect(output).not.toMatch(/UnhandledPromiseRejection|TypeError|SyntaxError/);
}

async function deleteApiKeyByNameDirect(keyName: string | null): Promise<void> {
  if (!keyName) return;
  const client = clientFor(FIXTURE_KEY.admin);
  const qs = new URLSearchParams({ page: "1", page_size: "100", sort: "-created_at" });
  const list = await client.get<ApiKeyListResponse>(`/v1/api-keys?${qs.toString()}`).catch(() => ({ items: [] }));
  for (const item of list.items.filter((k) => k.key_name === keyName)) {
    await client.patch(`/v1/api-keys/${encodeURIComponent(item.id)}`, { active: false }).catch(() => undefined);
    await client.delete(`/v1/api-keys/${encodeURIComponent(item.id)}`).catch(() => undefined);
  }
}

async function deleteFleetWithApiKey(
  apiUrl: string,
  rawApiKey: string,
  workspaceId: string,
  fleetId: string,
): Promise<void> {
  const fleetUrl = `${apiUrl}/v1/workspaces/${encodeURIComponent(workspaceId)}/fleets/${encodeURIComponent(fleetId)}`;
  const headers = { Authorization: `Bearer ${rawApiKey}`, "Content-Type": "application/json" };
  await fetch(fleetUrl, {
    method: "PATCH",
    headers,
    body: JSON.stringify({ status: "killed" }),
  }).catch(() => undefined);
  await fetch(fleetUrl, { method: "DELETE", headers }).catch(() => undefined);
}

test.describe("operator journey", () => {
  test.setTimeout(JOURNEY_TIMEOUT_MS);

  let createdApiKeyName: string | null = null;
  let createdApiKeyRaw: string | null = null;
  let createdTempRoot: string | null = null;
  let activeWorkspaceId: string | null = null;
  let createdFleetId: string | null = null;

  test.afterEach(async () => {
    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (apiUrl && createdApiKeyRaw && activeWorkspaceId && createdFleetId) {
      await deleteFleetWithApiKey(apiUrl, createdApiKeyRaw, activeWorkspaceId, createdFleetId);
    }
    createdApiKeyRaw = null;
    createdFleetId = null;
    await deleteApiKeyByNameDirect(createdApiKeyName);
    createdApiKeyName = null;
    if (createdTempRoot) {
      await fs.rm(createdTempRoot, { recursive: true, force: true }).catch(() => undefined);
      createdTempRoot = null;
    }
    activeWorkspaceId = null;
  });

  test("operator switches workspace, installs a Fleet, visits settings, mints an API key, uses it from command line, then halts the Fleet", async ({ page }) => {
    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (!apiUrl) throw new Error("NEXT_PUBLIC_API_URL must be set");

    const primaryWorkspaceName = uniqueName("journey-primary");
    const secondaryWorkspaceName = uniqueName("journey-secondary");
    const fleetName = uniqueName("journey-fleet");
    const apiKeyName = uniqueName("journey-key");

    await signInAs(page, FIXTURE_KEY.admin);
    await page.goto("/fleets");
    await expect(page.getByRole("heading", { name: /^fleets$/i }).first()).toBeVisible();

    await createWorkspaceFromSwitcher(page, primaryWorkspaceName);
    await createWorkspaceFromSwitcher(page, secondaryWorkspaceName);
    await switchWorkspace(page, primaryWorkspaceName);
    await switchWorkspace(page, secondaryWorkspaceName);

    await clickSidebarLink(page, "/fleets", /\/fleets(\?|$)/);
    await page.getByRole("link", { name: /install teammate/i }).first().click();
    await expect(page).toHaveURL(/\/fleets\/new(\?|$)/);
    // Resolve the active (secondary) workspace id so the onboard targets the
    // same workspace the browser switched to — its card must render here.
    const installWorkspace = (await listWorkspaces(FIXTURE_KEY.admin)).find(
      (workspace) => workspace.name === secondaryWorkspaceName,
    );
    if (!installWorkspace) {
      throw new Error(`operator-journey: workspace '${secondaryWorkspaceName}' not found via API`);
    }
    const fleetId = await installViaUI(page, fleetName, {
      handle: FIXTURE_KEY.admin,
      workspaceId: installWorkspace.id,
    });
    createdFleetId = fleetId;
    await expect(page.getByRole("region", { name: "Recent Activity" })).toBeVisible();

    await clickSidebarLink(page, "/events", /\/events(\?|$)/);
    await expect(page.getByRole("heading", { name: /^events$/i })).toBeVisible();
    await expect(page.getByLabel("Workspace events")).toBeVisible();

    await clickSidebarLink(page, "/approvals", /\/approvals(\?|$)/);
    await expect(page.getByRole("heading", { name: /^approvals$/i })).toBeVisible();
    await expect(page.getByLabel("Pending approval gates")).toBeVisible();

    await clickSidebarLink(page, "/settings", /\/settings(\?|$)/);
    await expect(page.getByRole("heading", { name: /^workspace$/i })).toBeVisible();
    await expect(page.getByLabel("Workspace", { exact: true })).toContainText(secondaryWorkspaceName);
    activeWorkspaceId = await activeWorkspaceIdFromSettings(page);

    await clickSidebarLink(page, "/settings/billing", /\/settings\/billing(\?|$)/);
    await expect(page.getByTestId("balance-headline")).toBeVisible();

    await page.goto("/settings/api-keys");
    await expect(page).toHaveURL(/\/settings\/api-keys(\?|$)/);
    await page.getByRole("button", { name: /new api key/i }).click();
    await page.getByLabel(/^name$/i).fill(apiKeyName);
    createdApiKeyName = apiKeyName;
    await page.getByRole("button", { name: /create key/i }).click();

    const revealField = page.getByLabel(/api key value/i);
    await expect(revealField).toBeVisible();
    const rawApiKey = await revealField.inputValue();
    expect(rawApiKey.startsWith("agt_t")).toBe(true);
    createdApiKeyRaw = rawApiKey;

    const { root: tempRoot, stateDir } = await makeCliStateDir(TEMP_DIR_PREFIX);
    createdTempRoot = tempRoot;
    await writeCliState(stateDir, activeWorkspaceId, rawApiKey, apiUrl, secondaryWorkspaceName);
    await closeApiKeyReveal(page);
    const commandEnv = cliEnv({
      AGENTSFLEET_STATE_DIR: stateDir,
      AGENTSFLEET_API_URL: apiUrl,
    });
    const cli = await spawnAgentsfleet(["--json", "list", "--workspace-id", activeWorkspaceId, "--limit", "10"], commandEnv);
    if (cli.code !== 0) {
      throw new Error(`agentsfleet list failed with API key auth (exit ${cli.code}):\n${cli.stderr}`);
    }
    const cliList = JSON.parse(cli.stdout) as CliFleetListResponse;
    expect(cliList.items?.some((fleet) => fleet.id === fleetId && fleet.name === fleetName)).toBe(true);

    await page.goto(`/fleets/${fleetId}`);
    await stopFleet(page);
    await clickSidebarLink(page, "/settings/billing", /\/settings\/billing(\?|$)/);
    await expect(page.getByTestId("balance-headline")).toBeVisible();

    await page.goto(`/fleets/${fleetId}`);
    await resumeFleet(page);
    await page.goto("/fleets");
    await expectRowState(page, fleetId, "live");

    await page.goto(`/fleets/${fleetId}`);
    await killFleet(page);
    await expectDetailKilled(page);
    await page.goto("/fleets");
    await expectRowState(page, fleetId, "failed");
    await deleteFleetWithApiKey(apiUrl, rawApiKey, activeWorkspaceId, fleetId);
    createdFleetId = null;

    await page.goto("/settings/api-keys");
    await expect(page.getByText(apiKeyName, { exact: true })).toBeVisible();

    const revoke = page.getByRole("button", { name: new RegExp(`revoke api key ${apiKeyName}`, "i") });
    await revoke.click();
    await page.getByRole("alertdialog").getByRole("button", { name: /^revoke$/i }).click();

    const revokedCli = await spawnAgentsfleet(
      ["list", "--workspace-id", activeWorkspaceId, "--limit", "1"],
      cliEnv({
        ...commandEnv,
        AGENTSFLEET_NO_RETRY: "1",
      }),
    );
    const revokedOutput = combinedOutput(revokedCli);
    expect(revokedCli.code).not.toBe(0);
    expect(revokedOutput).toMatch(/agentsfleet login|re-authenticate|unauthorized/i);
    expect(revokedOutput).not.toContain(rawApiKey);
    expectNoRuntimeDump(revokedOutput);

    const del = page.getByRole("button", { name: new RegExp(`delete api key ${apiKeyName}`, "i") });
    await expect(del).toBeVisible();
    await del.click();
    await page.getByRole("alertdialog").getByRole("button", { name: /^delete$/i }).click();
    await expect(page.getByText(apiKeyName, { exact: true })).toHaveCount(0);
    createdApiKeyRaw = null;
    createdApiKeyName = null;
  });
});
