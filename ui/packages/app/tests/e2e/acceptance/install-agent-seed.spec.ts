/**
 * install-agent-seed.spec.ts — sanity check for the API seed helper.
 *
 * Sister to install-agent-cli.spec.ts (canonical install path drives
 * `agentsfleet install`); this spec exercises the API-seed shortcut every
 * later spec (lifecycle, kill, multi-agent, multi-workspace, events,
 * logs-detail) relies on as setup. If the seed helper drifts, every
 * downstream spec fails the same way — keeping a dedicated sanity test
 * isolates the failure mode.
 *
 * Wire: fixture-user Bearer → POST /v1/workspaces/{ws}/agents → dashboard
 * /agents reload → assert the row renders with `data-state="live"`.
 * No `agentsfleet` here.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedAgent } from "./fixtures/seed";
import { cleanWorkspaceAgents } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("install-agent-seed", () => {
  test("API-seeded agent renders on /agents with live state", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    // Random suffix avoids (workspace_id, name) uniqueness collision with
    // any killed-but-not-deleted row from a previous interrupted run.
    const tag = Math.random().toString(36).slice(2, 8);
    const name = `install-seed-${tag}`;

    const seeded = await seedAgent(FIXTURE_KEY.regular, ws, { name });
    expect(seeded.id).toBeTruthy();

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/agents");
    await expect(page).toHaveURL(/\/agents(\?|$)/);

    // The row is an anchor `<Link href="/agents/{id}" data-state="live">`
    // wrapping a `<div class="font-medium truncate">{name}</div>`. Match by
    // visible name (accessible to a Playwright user) and assert data-state
    // is "live" (the dashboard's translation of agentsfleetd's "active" status —
    // canonical mapping at app/(dashboard)/agents/components/AgentsList.tsx).
    const row = page.locator(`a[href="/agents/${seeded.id}"]`);
    await expect(row).toBeVisible();
    await expect(row).toHaveAttribute("data-state", "live");
    await expect(row.getByText(name)).toBeVisible();
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceAgents(FIXTURE_KEY.regular, ws);
  });
});
