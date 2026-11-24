/** A real turn reaches an untouched conversation in a separate browser session. */
import type {BrowserContext} from "@playwright/test";
import {expect, test} from "@playwright/test";
import {signedInContext} from "./fixtures/auth";
import {clientFor} from "./fixtures/api-client";
import {FIXTURE_KEY} from "./fixtures/constants";
import {assertPassed, classifyTerminalTurn} from "./fixtures/execution";
import {awaitTerminalTurn, messageFleet} from "./fixtures/chat-walk";
import { createFixtureFleet } from "./fixtures/fleet-create";
import { createFleetViaUI } from "./fixtures/new-fleet";
import { probeMessage } from "./fixtures/probe";
import {workspaceHref} from "./fixtures/nav";
import {uniqueTag} from "./fixtures/observation";
import {getDefaultWorkspaceId, expectFleetActive} from "./fixtures/seed";
import {auditStream, documentIsMarked, markDocument, streamCount} from "./fixtures/soft-navigation";
import {assertDestructiveTargetIsSafe, purgeFixtureFleet} from "./fixtures/teardown";
import {streamFleetTurnsUrl} from "@/lib/api/turns-types";

const JOURNEY_TIMEOUT_MS = 420_000;
const RENDER_TIMEOUT_MS = 15_000;
const CONTROL_PURPOSE = "conversation-routing-control";
const ONE_OPEN_STREAM = {created: 1, open: 1};

test("a real reply and its run figures reach another session without reloading or leaking into another fleet", async ({browser}) => {
  test.setTimeout(JOURNEY_TIMEOUT_MS);
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const acting = await signedInContext(browser, FIXTURE_KEY.regular);
  const fleetIds: string[] = [];
  let observingContext: BrowserContext | null = null;
  try {
    const fleetId = await createFleetViaUI(acting.page, workspace, probeMessage(uniqueTag()));
    fleetIds.push(fleetId);
    await expectFleetActive(FIXTURE_KEY.regular, workspace, fleetId);
    const created = await createFixtureFleet(FIXTURE_KEY.regular, workspace, {name: CONTROL_PURPOSE, message: probeMessage(uniqueTag())});
    const control = created.fleet;
    fleetIds.push(control.id);
    await expectFleetActive(FIXTURE_KEY.regular, workspace, control.id);

    const observing = await signedInContext(browser, FIXTURE_KEY.regular);
    observingContext = observing.context;
    const watching = observing.page;
    await auditStream(watching, streamFleetTurnsUrl(workspace, fleetId));
    await watching.goto(workspaceHref(workspace, `fleets/${fleetId}`));
    await expect(watching.getByLabel("Fleet chat", {exact: true})).toBeVisible();
    await expect.poll(() => streamCount(watching)).toEqual(ONE_OPEN_STREAM);
    await markDocument(watching);

    const controlPage = await observing.context.newPage();
    await controlPage.goto(workspaceHref(workspace, `fleets/${control.id}`));
    await expect(controlPage.getByLabel("Fleet chat", {exact: true})).toBeVisible();
    await markDocument(controlPage);

    const message = `cross-session-result-${uniqueTag()}`;
    const seen = await messageFleet(acting.page, workspace, fleetId, message);
    const terminal = await awaitTerminalTurn(workspace, fleetId, seen);
    assertPassed(classifyTerminalTurn(terminal));

    const transcript = watching.getByLabel("Fleet chat", {exact: true}).getByRole("log", {name: "Chat", exact: true});
    await expect(transcript.getByText(message, {exact: true})).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    const reply = transcript.locator(`[data-message-id="${terminal.turn_id}:reply"]`);
    await expect(reply).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    await expect(reply).toHaveAttribute("data-settled", "true", {timeout: RENDER_TIMEOUT_MS});
    await expect(reply.locator('[data-role="assistant"]')).not.toHaveText(/^\s*$/, {timeout: RENDER_TIMEOUT_MS});
    await expect(reply.getByRole("button", {name: "Copy reply", exact: true})).toBeVisible();
    await expect(reply.getByRole("button", {name: "Run details", exact: true})).toHaveCount(0);

    await expect(controlPage.getByRole("log", {name: "Chat", exact: true}).getByText(message, {exact: true})).toHaveCount(0);
    await expect(controlPage.locator(`[data-message-id="${terminal.turn_id}:reply"]`)).toHaveCount(0);
    // The control fleet holds its own first turn and nothing of this walk's.
    const controlHistory = await clientFor(FIXTURE_KEY.regular).get<{items: Array<{event_type: string; turn_id: string}>}>(
      `/v1/workspaces/${workspace}/fleets/${control.id}/messages`,
    );
    expect(controlHistory.items.filter(row => row.event_type !== "activity").map(row => row.turn_id)).toEqual([created.turnId]);
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
