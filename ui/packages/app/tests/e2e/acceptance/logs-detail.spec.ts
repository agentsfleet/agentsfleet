/** Fleet detail acceptance: an operator can create and inspect an event. */
import { expect, test } from "@playwright/test";
import { clientFor } from "./fixtures/api-client";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedFleet } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const RENDER_TIMEOUT_MS = 15_000;

// The list is payload-free by design (bodies moved to the single-event
// route), so settlement is detected by an event row existing and the message
// text is proven against the detail read the operator's dialog uses.
interface EventPage {
  items: Array<{ event_id: string }>;
}

interface EventDetail {
  request_json: string | null;
}

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
    const api = clientFor(FIXTURE_KEY.regular);
    let settledEventId = "";
    await expect
      .poll(
        async () => {
          const events = await api.get<EventPage>(
            `/v1/workspaces/${ws}/fleets/${seeded.id}/events?limit=100`,
          );
          settledEventId = events.items[0]?.event_id ?? "";
          return settledEventId !== "";
        },
        { timeout: RENDER_TIMEOUT_MS },
      )
      .toBe(true);
    // The fleet was seeded in this test, so its first event is this steer;
    // the body lives only on the single-event route.
    const detail = await api.get<EventDetail>(
      `/v1/workspaces/${ws}/fleets/${seeded.id}/events/${settledEventId}`,
    );
    expect(detail.request_json ?? "").toContain(message);

    // The chat-first console: the summary strip carries status/outcome/cost
    // figures and the chat card carries the conversation.
    await expect(page.getByLabel("Fleet summary")).toBeVisible();
    await expect(page.getByLabel("Fleet chat")).toBeVisible({ timeout: 15_000 });
    // Scope to the fleet rail: the workspace sidebar carries its own Events
    // link, so the bare role query is ambiguous on the detail page.
    await page
      .getByRole("navigation", { name: "Fleet sections" })
      .getByRole("link", { name: "Events" })
      .click();
    const events = page.getByRole("table", { name: "Fleet events" });
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
