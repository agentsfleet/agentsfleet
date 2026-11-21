/** A real turn reaches an untouched conversation in a separate browser session. */
import type {BrowserContext} from "@playwright/test";
import {expect, test} from "@playwright/test";
import {signedInContext} from "./fixtures/auth";
import {clientFor} from "./fixtures/api-client";
import {FIXTURE_KEY} from "./fixtures/constants";
import {executionInstructions} from "./fixtures/definition";
import {assertPassed, classifyTerminalEvent} from "./fixtures/execution";
import {awaitTerminalTurn, messageFleet} from "./fixtures/chat-walk";
import {saveFleetViaUI} from "./fixtures/install-ui";
import {workspaceHref} from "./fixtures/nav";
import {uniqueTag} from "./fixtures/observation";
import {getDefaultWorkspaceId, seedFleet, waitForFleetActive} from "./fixtures/seed";
import {auditStream, documentIsMarked, markDocument, streamCount} from "./fixtures/soft-navigation";
import {assertDestructiveTargetIsSafe, purgeFixtureFleet} from "./fixtures/teardown";
import {streamFleetEventsUrl} from "@/lib/api/events-types";

const JOURNEY_TIMEOUT_MS = 420_000;
const RENDER_TIMEOUT_MS = 15_000;
const EXECUTING_PURPOSE = "conversation-live-update";
const CONTROL_PURPOSE = "conversation-routing-control";
const ONE_OPEN_STREAM = {created: 1, open: 1};

test("a real reply and its run figures reach another session without reloading or leaking into another fleet", async ({browser}) => {
  test.setTimeout(JOURNEY_TIMEOUT_MS);
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const acting = await signedInContext(browser, FIXTURE_KEY.regular);
  const fleetIds: string[] = [];
  let observingContext: BrowserContext | null = null;
  try {
    const fleetId = await saveFleetViaUI(acting.page, EXECUTING_PURPOSE, {
      handle: FIXTURE_KEY.regular, workspaceId: workspace, instructions: executionInstructions(),
    });
    fleetIds.push(fleetId);
    await waitForFleetActive(FIXTURE_KEY.regular, workspace, fleetId);
    const control = await seedFleet(FIXTURE_KEY.regular, workspace, {name: CONTROL_PURPOSE});
    fleetIds.push(control.id);
    await waitForFleetActive(FIXTURE_KEY.regular, workspace, control.id);

    const observing = await signedInContext(browser, FIXTURE_KEY.regular);
    observingContext = observing.context;
    const watching = observing.page;
    await auditStream(watching, streamFleetEventsUrl(workspace, fleetId));
    await watching.goto(workspaceHref(workspace, `fleets/${fleetId}`));
    await expect(watching.getByLabel("Fleet chat", {exact: true})).toBeVisible();
    await expect.poll(() => streamCount(watching)).toEqual(ONE_OPEN_STREAM);
    await markDocument(watching);

    const controlPage = await observing.context.newPage();
    await controlPage.goto(workspaceHref(workspace, `fleets/${control.id}`));
    await expect(controlPage.getByLabel("Fleet chat", {exact: true})).toBeVisible();
    await markDocument(controlPage);

    const message = `cross-session-result-${uniqueTag()}`;
    await messageFleet(acting.page, workspace, fleetId, message);
    const terminal = await awaitTerminalTurn(workspace, fleetId);
    assertPassed(classifyTerminalEvent(terminal));

    const transcript = watching.getByLabel("Fleet chat", {exact: true}).getByRole("log", {name: "Chat", exact: true});
    await expect(transcript.getByText(message, {exact: true})).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    const reply = transcript.locator(`[data-message-id="${terminal.event_id}:reply"]`);
    await expect(reply).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    await expect(reply).toHaveAttribute("data-settled", "true", {timeout: RENDER_TIMEOUT_MS});
    await expect(reply.locator('[data-role="assistant"]')).not.toHaveText(/^\s*$/, {timeout: RENDER_TIMEOUT_MS});
    const figures = reply.getByRole("button", {name: "Run details", exact: true}).last();
    await expect(figures).toBeVisible();
    await figures.click();
    const details = watching.getByRole("dialog", {name: "Run details", exact: true});
    await expect(details).toContainText("Duration");
    await expect(details).toContainText("Tokens");
    await expect(details).toContainText(/\d[\d.,]*(?:[kmb])?/i);

    await expect(controlPage.getByRole("log", {name: "Chat", exact: true}).getByText(message, {exact: true})).toHaveCount(0);
    await expect(controlPage.locator(`[data-message-id="${terminal.event_id}:reply"]`)).toHaveCount(0);
    const controlHistory = await clientFor(FIXTURE_KEY.regular).get<{items: Array<{event_type: string}>}>(
      `/v1/workspaces/${workspace}/fleets/${control.id}/messages`,
    );
    expect(controlHistory.items.every(row => row.event_type === "activity")).toBe(true);
    expect(await documentIsMarked(watching)).toBe(true);
    expect(await documentIsMarked(controlPage)).toBe(true);
    await expect.poll(() => streamCount(watching)).toEqual(ONE_OPEN_STREAM);
  } finally {
    await observingContext?.close();
    await acting.context.close();
    assertDestructiveTargetIsSafe();
    for (const fleetId of fleetIds) await purgeFixtureFleet(FIXTURE_KEY.regular, workspace, fleetId);
  }
});
