import type { Page } from "@playwright/test";
import { expect, test } from "@playwright/test";
import {
  WORKSPACE_LIST_PATH,
  type WorkspaceFetchAuditSnapshot,
} from "@/lib/acceptance/workspace-fetch-audit";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { ensureSecondWorkspace, getDefaultWorkspaceId } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const AUDIT_URL = "/acceptance-audit/workspace-fetches";
const NOT_FOUND_STATUS = 404;
const SECOND_WORKSPACE_NAME = "fixture-secondary";
const WORKSPACE_FETCH_LIMIT = 1;
const SWITCH_TIMEOUT_MS = 10_000;
const AUDIT_HEADERS = {
  "x-acceptance-token": process.env.AGENTSFLEET_E2E_AUDIT_TOKEN ?? "local-acceptance-audit-token",
} as const;

async function resetAudit(page: Page): Promise<boolean> {
  const response = await page.request.post(AUDIT_URL, { headers: AUDIT_HEADERS });
  if (response.status() === NOT_FOUND_STATUS) return false;
  expect(response.ok()).toBe(true);
  return true;
}

async function readAudit(page: Page): Promise<WorkspaceFetchAuditSnapshot> {
  const response = await page.request.get(AUDIT_URL, { headers: AUDIT_HEADERS });
  expect(response.ok()).toBe(true);
  return await response.json() as WorkspaceFetchAuditSnapshot;
}

async function expectWorkspaceFetchesWithinLimit(page: Page, label: string) {
  const snapshot = await readAudit(page);
  const workspaceFetches = snapshot.byPath[WORKSPACE_LIST_PATH] ?? 0;
  expect(workspaceFetches, `${label}: workspace list fetches`).toBeLessThanOrEqual(WORKSPACE_FETCH_LIMIT);
  expect(snapshot.total, `${label}: total audited fetches`).toBeLessThanOrEqual(WORKSPACE_FETCH_LIMIT);
}

test.describe("workspace fetch dedupe", () => {
  test("dashboard navigation and workspace switch do not repeat the workspace list call", async ({ page }) => {
    const primary = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECOND_WORKSPACE_NAME);
    expect(secondary.id).not.toEqual(primary);

    await signInAs(page, FIXTURE_KEY.regular);

    test.skip(!(await resetAudit(page)), "workspace fetch audit route is disabled");
    await page.goto(workspaceHref(primary, "fleets"));
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();
    await expectWorkspaceFetchesWithinLimit(page, "fleets render");

    await resetAudit(page);
    await page.getByRole("link", { name: "Events" }).click();
    await expect(page).toHaveURL(workspaceUrlPattern("events"));
    await expect(page.getByRole("heading", { name: /^events$/i })).toBeVisible();
    await expectWorkspaceFetchesWithinLimit(page, "events navigation");

    await resetAudit(page);
    const switcher = page.getByTestId("workspace-switcher");
    await switcher.click();
    await page.getByRole("menuitem", { name: secondary.name ?? secondary.id }).click();
    await expect(switcher).toContainText(secondary.name ?? secondary.id, {
      timeout: SWITCH_TIMEOUT_MS,
    });
    await expectWorkspaceFetchesWithinLimit(page, "workspace switch");
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws);
  });
});
