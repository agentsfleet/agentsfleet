/**
 * chat-single-fetch.spec.ts — the chat view's server render costs ONE thread
 * request and ZERO per-turn detail requests.
 *
 * This is the request-count acceptance for the thread read: the chat used to
 * issue an events-list read and then one `GET …/events/{event_id}` per turn
 * (a 20-wide fan-out). The server-side fetch audit counts both templates, so
 * the assertion is a number, not a latency eyeball — re-runnable against any
 * deployment with the audit enabled.
 */
import type { Page } from "@playwright/test";
import { expect, test } from "@playwright/test";
import {
  AUDITED_PATH,
  type WorkspaceFetchAuditSnapshot,
} from "@/lib/acceptance/workspace-fetch-audit";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref } from "./fixtures/nav";

const AUDIT_URL = "/acceptance-audit/workspace-fetches";
const NOT_FOUND_STATUS = 404;
const CHAT_SEED_PREFIX = "chat-fetch-spec-";
const EXPECTED_THREAD_FETCHES = 1;
const EXPECTED_DETAIL_FETCHES = 0;
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

test.describe("chat view request profile", () => {
  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, CHAT_SEED_PREFIX);
  });

  test("test_chat_single_thread_fetch: one thread read, zero per-turn detail reads", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
      name: `${CHAT_SEED_PREFIX}${tag}`,
    });
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);

    test.skip(!(await resetAudit(page)), "workspace fetch audit route is disabled");
    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
    await expect(page.getByRole("navigation", { name: "Fleet sections" })).toBeVisible();

    const snapshot = await readAudit(page);
    const threadFetches = snapshot.byPath[AUDITED_PATH.fleetMessages] ?? 0;
    const detailFetches = snapshot.byPath[AUDITED_PATH.fleetEventDetail] ?? 0;
    expect(threadFetches, "thread reads for one chat render").toBe(EXPECTED_THREAD_FETCHES);
    expect(detailFetches, "per-turn detail reads for one chat render").toBe(EXPECTED_DETAIL_FETCHES);
  });
});
