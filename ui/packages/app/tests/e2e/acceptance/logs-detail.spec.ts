/** Fleet detail acceptance: an operator can create and inspect an event. */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedFleet } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const RENDER_TIMEOUT_MS = 15_000;

test.describe("fleet detail logs", () => {
  test("an operator can open actionable details for a fleet event", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const seeded = await seedFleet(FIXTURE_KEY.regular, ws, { name: `logs-${tag}` });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(ws, `fleets/${seeded.id}`));
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${seeded.id}`));

    const composer = page.getByLabel("Chat composer");
    await expect(composer).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    const message = `inspect-${tag}`;
    await composer.getByPlaceholder(/message this fleet/i).fill(message);
    const persisted = page.waitForResponse((response) => {
      const request = response.request();
      return request.method() === "POST" && Boolean(request.headers()["next-action"]);
    });
    await composer.getByRole("button", { name: /send/i }).click();
    await expect(page.getByLabel("Fleet chat").getByText(message)).toBeVisible();
    await persisted;

    // The chat-first console: the summary strip carries status/outcome/cost
    // figures and the chat card carries the conversation.
    await expect(page.getByLabel("Fleet summary")).toBeVisible();
    await expect(page.getByLabel("Fleet chat")).toBeVisible({ timeout: 15_000 });
    await page.getByRole("link", { name: "Events" }).click();
    const events = page.getByLabel("Fleet events");
    await expect(events).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await events.getByRole("button", { name: /inspect event/i }).first().click();

    const dialog = page.getByRole("dialog", { name: "Event details" });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText("ID", { exact: true })).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Copy event ID" })).toBeVisible();
    await expect(dialog.getByRole("heading", { name: "Request context" })).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Copy diagnostic" })).toBeVisible();
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, "logs-");
  });
});
