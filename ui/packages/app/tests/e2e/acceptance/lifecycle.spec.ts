/**
 * lifecycle.spec.ts — operator stops a running agent via the dashboard.
 *
 * Wire: API-seed → /agents/[id] → KillSwitch "Stop" → ConfirmDialog
 * confirm → return to /agents and assert the row's `data-state` is
 * `parked` (the dashboard's translation of agentsfleetd's `stopped` status,
 * per `liveStateOf` in
 * `app/(dashboard)/agents/components/AgentsList.tsx:19`).
 *
 * Sister to kill.spec.ts; both exercise the same KillSwitch + ConfirmDialog
 * wiring but with different target statuses (`stopped` vs `killed`). The
 * shared interaction lives in fixtures/lifecycle.ts.
 *
 * Why no `waitForResponse(... PATCH)`: post-WS-A, KillSwitch fires
 * `setAgentStatusAction` (a Next.js Server Action) which POSTs to the app
 * origin, not directly to agentsfleetd. The PATCH happens server-side inside the
 * action. Asserting on the dashboard listing's `data-state` is the only
 * stable signal from the browser.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { expectRowState, stopAgent } from "./fixtures/lifecycle";
import { getDefaultWorkspaceId, seedAgent } from "./fixtures/seed";
import { cleanWorkspaceAgents } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("lifecycle", () => {
  test("Stop transitions the row's data-state from live to parked", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const name = `lifecycle-${tag}`;
    const seeded = await seedAgent(FIXTURE_KEY.regular, ws, { name });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(`/agents/${seeded.id}`);
    await expect(page).toHaveURL(new RegExp(`/agents/${seeded.id}(\\?|$)`));

    await stopAgent(page);

    await page.goto("/agents");
    await expectRowState(page, seeded.id, "parked");
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceAgents(FIXTURE_KEY.regular, ws);
  });
});
