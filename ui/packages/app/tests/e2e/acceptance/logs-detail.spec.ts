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
    await composer.getByPlaceholder(/message your fleet/i).fill(message);
    const persisted = page.waitForResponse((response) => {
      const request = response.request();
      return request.method() === "POST"
        && new URL(response.url()).pathname === `/live/v1/workspaces/${ws}/fleets/${seeded.id}/messages`;
    });
    await composer.getByRole("button", { name: /send/i }).click();
    await expect(page.getByLabel("Fleet chat").getByText(message)).toBeVisible();
    const accepted = await persisted;
    expect(accepted.status()).toBe(202);
    const receipt = await accepted.json();
    expect(receipt).toMatchObject({status: "accepted", replayed: false, event_id: expect.any(String)});
    const settledEventId = receipt.event_id as string;
    const api = clientFor(FIXTURE_KEY.regular);
    await expect
      .poll(
        async () => {
          const events = await api.get<EventPage>(
            `/v1/workspaces/${ws}/fleets/${seeded.id}/events?limit=100`,
          );
          return events.items.some(event => event.event_id === settledEventId);
        },
        { timeout: RENDER_TIMEOUT_MS },
      )
      .toBe(true);
    // Setup receipts share the timeline; inspect the event this send admitted.
    const detail = await api.get<EventDetail>(
      `/v1/workspaces/${ws}/fleets/${seeded.id}/events/${settledEventId}`,
    );
    expect(detail.request_json ?? "").toContain(message);

    // Workspace activity retains inspection after the fleet tabs were removed.
    await page.goto(workspaceHref(ws, "events"));
    const events = page.getByRole("table", { name: "Workspace activity" });
    await expect(events).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await events.getByRole("button", {name: `Inspect event ${settledEventId}`, exact: true}).click();

    // The detail keeps the outcome and request context prominent without
    // technical identifiers or clipboard controls in the footer.
    const dialog = page.getByRole("dialog", { name: "Event details" });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText("ID", { exact: true })).toHaveCount(0);
    await expect(dialog.getByText(/^Source:/)).toBeVisible();
    await expect(dialog.getByText(/^Type:/)).toBeVisible();
    await expect(dialog.getByRole("heading", { name: "Request context" })).toBeVisible();
    await expect(dialog).toContainText(message);
    await expect(dialog.getByRole("button", { name: /copy/i })).toHaveCount(0);
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, "logs-");
  });
});
