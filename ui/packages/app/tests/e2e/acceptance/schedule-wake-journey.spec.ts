/**
 * schedule-wake-journey.spec.ts — "wake up every minute, then let me talk to it."
 *
 * A fleet made from its first message is given an every-minute schedule; the
 * scheduler's callback wakes it; the fleet chat shows the wake-up and the
 * run's reply; and the user keeps chatting in the same thread. The wake is the
 * real scheduler calling the daemon. The fleet is purged by its identifier
 * afterwards, so it stops waking runners.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { createFleetViaUI } from "./fixtures/new-fleet";
import { probeMessage, probeReply } from "./fixtures/probe";
import { getDefaultWorkspaceId, expectFleetActive } from "./fixtures/seed";
import { purgeFixtureFleet } from "./fixtures/teardown";
import { anyRunnerLive } from "./fixtures/execution";
import { messageFleet } from "./fixtures/chat-walk";
import { workspaceHref } from "./fixtures/nav";

const EVERY_MINUTE = "* * * * *";
// One scheduler minute, the run and one model round trip, with headroom that is not a retry.
const WAKE_TIMEOUT_MS = 240_000;
const REPLY_TIMEOUT_MS = 180_000;
const JOURNEY_TIMEOUT_MS = 600_000;

test.describe("scheduled wake-up", () => {
  let workspaceId = "";
  let fleetId: string | null = null;

  test.afterAll(async () => {
    if (workspaceId && fleetId) await purgeFixtureFleet(FIXTURE_KEY.regular, workspaceId, fleetId);
  });

  test("a schedule wakes the fleet into its chat, and the user keeps chatting", async ({ page }) => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);
    expect(await anyRunnerLive(), "no runner is online, so a wake-up could not run").toBe(true);
    workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = crypto.randomBytes(3).toString("hex");
    const wake = `wake-${tag}`;
    await signInAs(page, FIXTURE_KEY.regular);

    // A first message makes the fleet; the schedule is its own row, each firing asking for one known line.
    const saved = await createFleetViaUI(page, workspaceId, probeMessage(`first-${tag}`));
    fleetId = saved;
    await expectFleetActive(FIXTURE_KEY.regular, workspaceId, saved);
    await clientFor(FIXTURE_KEY.regular).post(`/v1/workspaces/${workspaceId}/fleets/${saved}/schedules`,
      { cron: EVERY_MINUTE, timezone: "UTC", message: probeMessage(wake) });

    // The scheduler's callback lands in the fleet chat as a wake-up, and the run answers it.
    await page.goto(workspaceHref(workspaceId, `fleets/${saved}`));
    const chat = page.getByLabel("Fleet chat");
    await expect(chat.getByText("Woke up on schedule").first()).toBeVisible({ timeout: WAKE_TIMEOUT_MS });
    await expect(chat.getByText(probeReply(wake)).first()).toBeVisible({ timeout: REPLY_TIMEOUT_MS });

    // The same thread keeps taking ordinary messages after the wake-up.
    const followUp = `follow-up-${tag}`;
    await messageFleet(page, workspaceId, saved, probeMessage(followUp));
    await expect(chat.getByText(probeReply(followUp)).first()).toBeVisible({ timeout: REPLY_TIMEOUT_MS });
  });
});
