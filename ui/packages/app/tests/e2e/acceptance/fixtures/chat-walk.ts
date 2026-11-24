/**
 * chat-walk.ts — messaging a fleet from its own composer, and waiting out the
 * answer.
 *
 * One turn is four things a walk must not conflate: the operator's send, the
 * daemon's acknowledgement of it, the runner running and finishing the work,
 * and the reply appearing on screen. Each is a different
 * failure with a different owner, so each is its own bounded step here and the
 * verdicts come from `execution.ts`'s classifiers rather than from a timeout.
 *
 * The acknowledgement step is the one that looks redundant and is not: a send
 * rides a Server Action, and the optimistic row renders whether or not the
 * request survives. A walk that clicks Send and navigates away in the same beat
 * can abort the delivery it is about to wait for.
 */
import { expect, type Page } from "@playwright/test";
import type { TurnDetail } from "@/lib/api/turns";
import { FIXTURE_KEY } from "./constants";
import {
  anyRunnerLive,
  classifyReportMissing,
  classifyStillRunning,
  classifyIdle,
  countChatTurns,
  failWith,
  findRunFor,
  JOURNEY_LEG,
  runIsSettled,
  readTerminalChatTurn,
  type RunLocation,
} from "./execution";
import { workspaceHref } from "./nav";
import { observed, pollFor } from "./observation";

const RENDER_TIMEOUT_MS = 15_000;
// Delivery → run is the queue plus the runner heartbeat cadence.
const RUN_TIMEOUT_MS = 120_000;
// Run → terminal row includes one provider round trip. Sized for a slow
// model minute, not a stalled one.
const EXECUTION_TIMEOUT_MS = 150_000;
// How long the walk waits for the daemon to acknowledge a send before it trusts
// the page enough to navigate away from it.
const SEND_ACK_TIMEOUT_MS = 30_000;

const CHAT_LABEL = "Fleet chat";
const COMPOSER_LABEL = "Chat composer";
const ASSISTANT_TURN = '[data-role="assistant"]';

/** Send one message from the fleet's own composer, and return once the daemon
 * holds it. */
export async function messageFleet(
  page: Page,
  workspaceId: string,
  fleetId: string,
  body: string,
): Promise<void> {
  const before = await observed(JOURNEY_LEG.execute, () =>
    countChatTurns(FIXTURE_KEY.regular, workspaceId, fleetId),
  );
  await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
  const composer = page.getByLabel(COMPOSER_LABEL);
  await expect(composer).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  await composer.getByPlaceholder(/message your fleet/i).fill(body);
  await composer.getByRole("button", { name: /send/i }).click();
  await expect
    .poll(
      () =>
        observed(JOURNEY_LEG.execute, () =>
          countChatTurns(FIXTURE_KEY.regular, workspaceId, fleetId),
        ),
      { timeout: SEND_ACK_TIMEOUT_MS },
    )
    .toBeGreaterThan(before);
}

/**
 * Wait out the delivery and answer with its terminal row.
 *
 * Never returns for a delivery that did not finish: each way of not finishing
 * has its own verdict, because "no run at all", "a run that settled with no
 * report" and "a run still running" have three different owners.
 */
export async function awaitTerminalTurn(
  workspaceId: string,
  fleetId: string,
): Promise<TurnDetail> {
  const run = await pollFor<RunLocation>(
    () => observed(JOURNEY_LEG.run, () => findRunFor(fleetId)),
    RUN_TIMEOUT_MS,
  );
  if (run === null) failWith(classifyIdle(await anyRunnerLive()));
  const terminal = await pollFor<TurnDetail>(
    () =>
      observed(JOURNEY_LEG.execute, () =>
        readTerminalChatTurn(FIXTURE_KEY.regular, workspaceId, fleetId),
      ),
    EXECUTION_TIMEOUT_MS,
  );
  if (terminal !== null) return terminal;
  const settled = await observed(JOURNEY_LEG.execute, () => findRunFor(fleetId));
  if (settled !== null && runIsSettled(settled.outcome)) {
    failWith(classifyReportMissing(settled.outcome));
  }
  if (settled !== null) failWith(classifyStillRunning(settled.outcome));
  failWith(classifyIdle(await anyRunnerLive()));
}

/** A persisted terminal row can arrive before the browser paints its reply. */
export async function expectAnsweredOnScreen(
  page: Page,
  workspaceId: string,
  fleetId: string,
): Promise<void> {
  await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
  const reply = page.getByLabel(CHAT_LABEL).locator(ASSISTANT_TURN).last();
  await expect(reply).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  await expect(reply).not.toHaveText(/^\s*$/);
  await expect(reply.getByRole("button", {name: "Copy reply"})).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  await expect(reply.getByRole("button", {name: "Run details"})).toHaveCount(0);
}
