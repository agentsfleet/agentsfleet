/**
 * chat-walk.ts — messaging a fleet from its own composer, and waiting out the
 * answer.
 *
 * One turn is four things a walk must not conflate: the operator's send, the
 * daemon's acknowledgement of it, the runner leasing and finishing the work,
 * and the reply appearing on screen with its figures. Each is a different
 * failure with a different owner, so each is its own bounded step here and the
 * verdicts come from `execution.ts`'s classifiers rather than from a timeout.
 *
 * The acknowledgement step is the one that looks redundant and is not: a send
 * rides a Server Action, and the optimistic row renders whether or not the
 * request survives. A walk that clicks Send and navigates away in the same beat
 * can abort the delivery it is about to wait for.
 */
import { expect, type Page } from "@playwright/test";
import type { EventDetail } from "@/lib/api/events";
import {
  METRICS_STRIP_LABEL,
  METRICS_TIME_LABEL,
  METRICS_TOKENS_LABEL,
  METRICS_VALUE_UNKNOWN,
} from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/console-copy";
import { FIXTURE_KEY } from "./constants";
import {
  anyRunnerLive,
  classifyReportMissing,
  classifyStillRunning,
  classifyUnleased,
  countChatTurns,
  failWith,
  findLeaseFor,
  JOURNEY_LEG,
  leaseIsSettled,
  readTerminalChatTurn,
  type LeaseLocation,
} from "./execution";
import { workspaceHref } from "./nav";
import { observed, pollFor } from "./observation";

const RENDER_TIMEOUT_MS = 15_000;
// Delivery → lease is the queue plus the runner heartbeat cadence.
const LEASE_TIMEOUT_MS = 120_000;
// Lease → terminal row includes one provider round trip. Sized for a slow
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
  await composer.getByPlaceholder(/message this fleet/i).fill(body);
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
 * has its own verdict, because "no lease at all", "a lease that settled with no
 * report" and "a lease still running" have three different owners.
 */
export async function awaitTerminalTurn(
  workspaceId: string,
  fleetId: string,
): Promise<EventDetail> {
  const lease = await pollFor<LeaseLocation>(
    () => observed(JOURNEY_LEG.lease, () => findLeaseFor(fleetId)),
    LEASE_TIMEOUT_MS,
  );
  if (lease === null) failWith(classifyUnleased(await anyRunnerLive()));
  const terminal = await pollFor<EventDetail>(
    () =>
      observed(JOURNEY_LEG.execute, () =>
        readTerminalChatTurn(FIXTURE_KEY.regular, workspaceId, fleetId),
      ),
    EXECUTION_TIMEOUT_MS,
  );
  if (terminal !== null) return terminal;
  const settled = await observed(JOURNEY_LEG.execute, () => findLeaseFor(fleetId));
  if (settled !== null && leaseIsSettled(settled.outcome)) {
    failWith(classifyReportMissing(settled.outcome));
  }
  if (settled !== null) failWith(classifyStillRunning(settled.outcome));
  failWith(classifyUnleased(await anyRunnerLive()));
}

/**
 * The reply as the operator reads it, and the figures beside it.
 *
 * The row is what the daemon persisted; this is the surface a person sees, and
 * the two are asserted apart on purpose. The figures are graded against the
 * strip's own unknown-dash rather than against a number, because what the model
 * spent is not this walk's claim — only that the strip has it.
 */
export async function expectAnsweredOnScreen(
  page: Page,
  workspaceId: string,
  fleetId: string,
): Promise<void> {
  await page.goto(workspaceHref(workspaceId, `fleets/${fleetId}`));
  const reply = page.getByLabel(CHAT_LABEL).locator(ASSISTANT_TURN).last();
  await expect(reply).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  await expect(reply).not.toHaveText(/^\s*$/);
  const strip = page.getByLabel(METRICS_STRIP_LABEL);
  await expect(strip).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
  for (const label of [METRICS_TOKENS_LABEL, METRICS_TIME_LABEL]) {
    const figure = new RegExp(`${label}\\s*(\\S+)`, "i");
    await expect
      .poll(async () => figure.exec(await strip.innerText())?.[1] ?? METRICS_VALUE_UNKNOWN, {
        timeout: RENDER_TIMEOUT_MS,
      })
      .not.toBe(METRICS_VALUE_UNKNOWN);
  }
}
