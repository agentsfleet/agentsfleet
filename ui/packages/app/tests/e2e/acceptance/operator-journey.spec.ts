import * as crypto from "node:crypto";
import * as fs from "node:fs/promises";
import { expect, test, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { clientFor } from "./fixtures/api-client";
import { listWorkspaces } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { gotoWorkspace, workspaceHref, workspaceUrlPattern } from "./fixtures/nav";
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
const MENU_CLICK_TIMEOUT_MS = 5_000;
const MENU_CLICK_ATTEMPTS = 3;
const TEMP_DIR_PREFIX = "agentsfleet-operator-journey-";
// Shared by the fleet-name generator and the afterEach sweep — one literal,
// so the sweep can never drift from what the journey actually names. The
// seeded fleet carries a live cron trigger; a leaked row keeps waking
// runners until something deletes it.
const JOURNEY_FLEET_PREFIX = "journey-fleet";

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

  const dialog = page.getByRole("dialog", { name: "Create workspace" });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("Name").fill(name);
  await dialog.getByTestId("workspace-create-submit").click();
  await expect(dialog).toBeHidden({ timeout: ACTION_TIMEOUT_MS });
  await expect(switcher).toContainText(name, { timeout: ACTION_TIMEOUT_MS });
}

async function switchWorkspace(page: Page, name: string): Promise<void> {
  const switcher = page.getByTestId("workspace-switcher");
  await expect(switcher).toBeVisible();
  for (let attempt = 1; attempt <= MENU_CLICK_ATTEMPTS; attempt += 1) {
    if ((await switcher.getAttribute("aria-expanded")) !== "true") {
      await switcher.click();
    }
    try {
      await page.getByRole("menuitem", { name }).click({ timeout: MENU_CLICK_TIMEOUT_MS });
      break;
    } catch (error) {
      if (attempt === MENU_CLICK_ATTEMPTS) throw error;
    }
  }
  await expect(switcher).toContainText(name, { timeout: ACTION_TIMEOUT_MS });
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

// Resolve a created workspace's id by its (unique-per-run) name. The browser
// flow only ever surfaces names; the API is the id source of record.
async function workspaceIdByName(name: string): Promise<string> {
  const workspace = (await listWorkspaces(FIXTURE_KEY.admin)).find((w) => w.name === name);
  if (!workspace) {
    throw new Error(`operator-journey: workspace '${name}' not found via API`);
  }
  return workspace.id;
}

test.describe("operator journey", () => {
  test.setTimeout(JOURNEY_TIMEOUT_MS);

  let createdApiKeyName: string | null = null;
  let createdTempRoot: string | null = null;
  let createdWorkspaceIds: string[] = [];

  test.afterEach(async () => {
    // The fleet outlives more failure modes than the API key: it exists
    // before the key is minted, and installViaUI can die after the server
    // created it but before the id is readable from the URL. Sweeping by
    // name-prefix with the admin fixture JWT across every workspace this
    // run created depends on neither, so a journey that fails anywhere
    // after fleet creation still tears down its cron-carrying rows.
    for (const wsId of createdWorkspaceIds) {
      await cleanWorkspaceFleets(FIXTURE_KEY.admin, wsId, `${JOURNEY_FLEET_PREFIX}-`).catch(
        (err: unknown) => {
          console.error(`[e2e:journey] fleet sweep failed for workspace ${wsId}:`, err);
        },
      );
    }
    createdWorkspaceIds = [];
    await deleteApiKeyByNameDirect(createdApiKeyName);
    createdApiKeyName = null;
    if (createdTempRoot) {
      await fs.rm(createdTempRoot, { recursive: true, force: true }).catch(() => undefined);
      createdTempRoot = null;
    }
  });

  test("operator switches workspace, installs a Fleet, visits settings, mints an API key, uses it from command line, then halts the Fleet", async ({ page }) => {
    page.setDefaultTimeout(ACTION_TIMEOUT_MS);
    const apiUrl = process.env.NEXT_PUBLIC_API_URL;
    if (!apiUrl) throw new Error("NEXT_PUBLIC_API_URL must be set");

    const primaryWorkspaceName = uniqueName("journey-primary");
    const secondaryWorkspaceName = uniqueName("journey-secondary");
    const fleetName = uniqueName(JOURNEY_FLEET_PREFIX);
    const apiKeyName = uniqueName("journey-key");

    await signInAs(page, FIXTURE_KEY.admin);
    await gotoWorkspace(page, FIXTURE_KEY.admin, "fleets");
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();

    // Resolve each workspace id via the API the moment the browser creates
    // it — before any fleet can exist — so the afterEach sweep knows every
    // workspace this run owns even when the journey dies mid-flight. The
    // workspace is the URL segment (`/w/<id>/…`) — there is no implicit
    // "active workspace" to read from a settings page — so the secondary id
    // is what the nav hrefs, the onboard, and the CLI below all target.
    await createWorkspaceFromSwitcher(page, primaryWorkspaceName);
    createdWorkspaceIds.push(await workspaceIdByName(primaryWorkspaceName));
    await createWorkspaceFromSwitcher(page, secondaryWorkspaceName);
    const wsId = await workspaceIdByName(secondaryWorkspaceName);
    createdWorkspaceIds.push(wsId);
    await switchWorkspace(page, primaryWorkspaceName);
    await switchWorkspace(page, secondaryWorkspaceName);

    await clickSidebarLink(page, workspaceHref(wsId, "fleets"), workspaceUrlPattern("fleets"));
    await page.getByRole("link", { name: /install a fleet/i }).first().click();
    await expect(page).toHaveURL(workspaceUrlPattern("fleets/new"));
    const fleetId = await installViaUI(page, fleetName, {
      handle: FIXTURE_KEY.admin,
      workspaceId: wsId,
    });
    // The detail page opens on Chat — the conversation card is the
    // post-install scaffolding assertion.
    await expect(page.getByLabel("Fleet chat")).toBeVisible({ timeout: 15_000 });

    await clickSidebarLink(page, workspaceHref(wsId, "events"), workspaceUrlPattern("events"));
    await expect(page.getByRole("heading", { name: /^events$/i })).toBeVisible();
    await expect(page.getByLabel("Workspace events")).toBeVisible();

    await clickSidebarLink(page, workspaceHref(wsId, "approvals"), workspaceUrlPattern("approvals"));
    await expect(page.getByRole("heading", { name: /^approvals$/i })).toBeVisible();
    await expect(page.getByLabel("Pending approval gates")).toBeVisible();

    // The standalone workspace-settings page was folded into API Keys post-M118
    // (no `/settings` sidebar link, no workspace-settings index route), so the
    // active-workspace id now comes from the API-resolved id above rather than
    // scraped from a settings page. Billing stays a tenant-scoped root route.
    await clickSidebarLink(page, "/settings/billing", /\/settings\/billing(\?|$)/);
    await expect(page.getByTestId("balance-headline")).toBeVisible();

    await page.goto("/settings/api-keys");
    await expect(page).toHaveURL(/\/settings\/api-keys(\?|$)/);
    await page.getByRole("button", { name: "Create key", exact: true }).click();
    const createKeyDialog = page.getByRole("dialog", { name: "Create API key" });
    await createKeyDialog.getByLabel(/^name$/i).fill(apiKeyName);
    createdApiKeyName = apiKeyName;
    await createKeyDialog.getByRole("button", { name: "Create", exact: true }).click();

    const revealField = page.getByLabel(/api key value/i);
    await expect(revealField).toBeVisible();
    const rawApiKey = await revealField.inputValue();
    expect(rawApiKey.startsWith("agt_t")).toBe(true);

    const { root: tempRoot, stateDir } = await makeCliStateDir(TEMP_DIR_PREFIX);
    createdTempRoot = tempRoot;
    await writeCliState(stateDir, wsId, rawApiKey, apiUrl, secondaryWorkspaceName);
    await closeApiKeyReveal(page);
    const commandEnv = cliEnv({
      AGENTSFLEET_STATE_DIR: stateDir,
      AGENTSFLEET_API_URL: apiUrl,
    });
    const cli = await spawnAgentsfleet(["--json", "list", "--workspace-id", wsId, "--limit", "10"], commandEnv);
    if (cli.code !== 0) {
      throw new Error(`agentsfleet list failed with API key auth (exit ${cli.code}):\n${cli.stderr}`);
    }
    const cliList = JSON.parse(cli.stdout) as CliFleetListResponse;
    expect(cliList.items?.some((fleet) => fleet.id === fleetId && fleet.name === fleetName)).toBe(true);

    await page.goto(workspaceHref(wsId, `fleets/${fleetId}`));
    await stopFleet(page);
    await clickSidebarLink(page, "/settings/billing", /\/settings\/billing(\?|$)/);
    await expect(page.getByTestId("balance-headline")).toBeVisible();

    await page.goto(workspaceHref(wsId, `fleets/${fleetId}`));
    await resumeFleet(page);
    await page.goto(workspaceHref(wsId, "fleets"));
    await expectRowState(page, fleetId, "live");

    await page.goto(workspaceHref(wsId, `fleets/${fleetId}`));
    await killFleet(page);
    await expectDetailKilled(page);
    await page.goto(workspaceHref(wsId, "fleets"));
    await expectRowState(page, fleetId, "failed");
    await deleteFleetWithApiKey(apiUrl, rawApiKey, wsId, fleetId);

    await page.goto("/settings/api-keys");
    await expect(page.getByText(apiKeyName, { exact: true })).toBeVisible();

    const revoke = page.getByRole("button", { name: new RegExp(`revoke api key ${apiKeyName}`, "i") });
    await revoke.click();
    await page.getByRole("alertdialog").getByRole("button", { name: /^revoke$/i }).click();

    const revokedCli = await spawnAgentsfleet(
      ["list", "--workspace-id", wsId, "--limit", "1"],
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
    createdApiKeyName = null;
  });
});
