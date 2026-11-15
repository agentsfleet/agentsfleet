/**
 * schedule-wake-journey.spec.ts — "wake up every minute, then let me talk to it."
 *
 * A fleet saved with a cron trigger gets a hosted schedule; the scheduler's
 * callback wakes it; the fleet chat shows the wake-up and the run's reply; and
 * the user keeps chatting in the same thread. Nothing is seeded past the save:
 * the wake is the real scheduler calling the daemon. The fleet is swept
 * afterwards, so it stops waking runners.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { EXECUTION_REPLY_PREFIX } from "./fixtures/definition";
import { saveFleetViaUI } from "./fixtures/install-ui";
import { getDefaultWorkspaceId, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { anyRunnerLive } from "./fixtures/execution";
import { messageFleet } from "./fixtures/chat-walk";
import { workspaceHref } from "./fixtures/nav";
import { NANOS_PER_USD } from "@/lib/types";

const NAME_PREFIX = "wake-walk";
const EVERY_MINUTE = "* * * * *";
// One scheduler minute, the lease and one model round trip, with headroom that is not a retry.
const WAKE_TIMEOUT_MS = 240_000;
const REPLY_TIMEOUT_MS = 180_000;
const JOURNEY_TIMEOUT_MS = 600_000;

function wakeConfig(name: string, message: string) {
  return { name, "x-agentsfleet": { triggers: [{ type: "cron", schedule: EVERY_MINUTE, timezone: "UTC", message }],
    tools: [], budget: { daily_nanos: String(NANOS_PER_USD) } } };
}

test.describe("scheduled wake-up", () => {
  let workspaceId = "";

  test.afterAll(async () => {
    if (workspaceId) await cleanWorkspaceFleets(FIXTURE_KEY.regular, workspaceId, NAME_PREFIX);
  });

  test("a saved schedule wakes the fleet into its chat, and the user keeps chatting", async ({ page }) => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);
    expect(await anyRunnerLive(), "no runner is online, so a wake-up could not run").toBe(true);
    workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = crypto.randomBytes(3).toString("hex");
    const name = `${NAME_PREFIX}-${tag}`;
    const wake = `morning check ${tag}`;
    await signInAs(page, FIXTURE_KEY.regular);

    // The schedule is part of the setup the user reviews and saves.
    const fleetId = await saveFleetViaUI(page, name, { handle: FIXTURE_KEY.regular, workspaceId, config: wakeConfig(name, wake) });
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleetId);

    // The scheduler's callback lands in the fleet chat as a wake-up, and the run answers it.
    await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
    const chat = page.getByLabel("Fleet chat");
    await expect(chat.getByText("Woke up on schedule").first()).toBeVisible({ timeout: WAKE_TIMEOUT_MS });
    await expect(chat.getByText(`${EXECUTION_REPLY_PREFIX} ${wake}`).first()).toBeVisible({ timeout: REPLY_TIMEOUT_MS });

    // The same thread keeps taking ordinary messages after the wake-up.
    const followUp = `follow-up ${tag}`;
    await messageFleet(page, workspaceId, fleetId, followUp);
    await expect(chat.getByText(`${EXECUTION_REPLY_PREFIX} ${followUp}`).first()).toBeVisible({ timeout: REPLY_TIMEOUT_MS });
  });
});
