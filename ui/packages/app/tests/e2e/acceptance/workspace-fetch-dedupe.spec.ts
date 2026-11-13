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
import { expectSelectedWorkspace, openWorkspaceMenu, workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

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
    await page.getByRole("navigation", {name: "Primary", exact: true}).getByRole("link", {name: "Secrets", exact: true}).click();
    await expect(page).toHaveURL(workspaceUrlPattern("secrets"));
    await expect(page.getByRole("heading", { name: "Secrets", exact: true })).toBeVisible();
    await expectWorkspaceFetchesWithinLimit(page, "secrets navigation");

    await resetAudit(page);
    await openWorkspaceMenu(page);
    await page.getByRole("menuitem", { name: secondary.name ?? secondary.id }).click();
    await expect(page).toHaveURL(new RegExp(`/w/${secondary.id}/secrets$`), {timeout: SWITCH_TIMEOUT_MS});
    await expectSelectedWorkspace(page, secondary.name ?? "Unnamed workspace");
    await expectWorkspaceFetchesWithinLimit(page, "workspace switch");
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws);
  });
});
