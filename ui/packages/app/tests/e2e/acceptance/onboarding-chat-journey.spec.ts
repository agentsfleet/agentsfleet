/**
 * onboarding-chat-journey.spec.ts — "make me a fleet."
 *
 * The path a new user actually takes, typed rather than seeded: open New
 * fleet, see which model is drafting, describe the work, review the proposal
 * the setup chat returns, save it, and find the active fleet in sidebar history. The
 * description is complete and asks for no questions, so a live authoring
 * model has what it needs to propose in one turn.
 */
import { expect, test } from "@playwright/test";
import { CHAT_PLACEHOLDER } from "@/components/domain/setup/chat-defaults";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { workspaceHref } from "./fixtures/nav";
import { CHAT_COPY, openSavedFleet, privateSaveButton, setupSummary } from "./fixtures/private-chat";
import { expectSidebarFleetState } from "./fixtures/lifecycle";
import { purgeFixtureFleet } from "./fixtures/teardown";

const FLEET_NAME = "message-acknowledger";
const JOURNEY_TIMEOUT_MS = 300_000;
const PROPOSAL_TIMEOUT_MS = 180_000;

function description(name: string): string {
  return `Create a fleet named ${name}. When it receives a message it replies with one line: ` +
    "the word ACK, a space, then the message text. It uses no tools, no triggers and no credentials. " +
    "Do not ask me any questions; propose the complete setup now.";
}

test.describe("onboarding chat", () => {
  let workspaceId = "";
  let fleetId: string | null = null;

  test.afterAll(async () => {
    if (workspaceId && fleetId) await purgeFixtureFleet(FIXTURE_KEY.regular, workspaceId, fleetId);
  });

  test("a user describes a fleet, reviews the proposal under a named model, saves it and sees it live", async ({ page }) => {
    test.setTimeout(JOURNEY_TIMEOUT_MS);
    workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspaceId, "fleets/new"));

    // The drafting model is named before the first word is typed.
    const model = page.getByRole("combobox", { name: "Authoring model" });
    await expect(model).toBeVisible();
    await expect(model).not.toHaveText(/^\s*(Choose model)?\s*$/);

    await page.getByLabel(CHAT_PLACEHOLDER).fill(description(FLEET_NAME));
    await page.getByRole("button", { name: CHAT_COPY.start, exact: true }).click();

    // The proposal arrives as a reviewable setup, with its runtime model named.
    // The proposed name heads the page (DraftChatHeading); the setup region holds its checks.
    await expect(page.getByRole("heading", { level: 1, name: FLEET_NAME, exact: true })).toBeVisible({ timeout: PROPOSAL_TIMEOUT_MS });
    const summary = setupSummary(page);
    await expect(summary).toContainText("Model");
    await expect(summary).not.toContainText("Not reviewed");
    await expect(summary).not.toContainText("Not resolved");

    await (await privateSaveButton(page)).click();
    const saved = await openSavedFleet(page, { handle: FIXTURE_KEY.regular, workspaceId });
    fleetId = saved;
    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expectSidebarFleetState(page, saved, "active");
  });
});
