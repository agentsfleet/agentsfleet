/**
 * prime-walk.ts — what the prime journeys share once their fleet exists: who
 * the person is, how their next message is typed and waited out, and how a
 * turn is read back through the API.
 *
 * A journey asserts what a tool did, never the model's prose: the thread row
 * carries who sent a turn, which turn it resumes, the calls its run made and
 * the reply, and those are what the readers here return.
 */
import type { Page } from "@playwright/test";
import { z } from "zod";
import type { TurnDetail, ThreadPage } from "@/lib/api/turns";
import { clientFor } from "./api-client";
import { awaitTerminalTurn, messageFleet } from "./chat-walk";
import { FIXTURE_KEY } from "./constants";
import { JOURNEY_LEG, turnHasSettled } from "./execution";
import { observed, pollFor } from "./observation";

/** The signed-in person every prime journey types as. */
export const PRIME_PERSON = FIXTURE_KEY.regular;

// A recorded activity row (a schedule registered, a model changed) is the fleet's own note, not a turn.
const TURN_KIND_ACTIVITY = "activity";
const MessageBody = z.object({message: z.string()});

/** Every turn on the fleet's thread, newest first, without the fleet's recorded activity. */
export async function threadOf(workspaceId: string, fleetId: string): Promise<TurnDetail[]> {
  const page = await observed(JOURNEY_LEG.observe, () =>
    clientFor(PRIME_PERSON).get<ThreadPage>(`/v1/workspaces/${workspaceId}/fleets/${fleetId}/messages`));
  return page.items.filter((item) => item.event_type !== TURN_KIND_ACTIVITY);
}

/** Types `message` in the fleet's composer and answers with that message's turn once it has settled. */
export async function say(page: Page, workspaceId: string, fleetId: string, message: string): Promise<TurnDetail> {
  const seen = await messageFleet(page, workspaceId, fleetId, message);
  return awaitTerminalTurn(workspaceId, fleetId, seen);
}

/**
 * The first settled turn `pick` accepts, or `null` when the budget ran out
 * with none: how a journey waits for a turn something other than its own
 * message wrote, such as a wake-up or another fleet's request.
 */
export async function settledTurn(
  workspaceId: string, fleetId: string, pick: (turn: TurnDetail) => boolean, timeoutMs: number,
): Promise<TurnDetail | null> {
  return pollFor(async () =>
    (await threadOf(workspaceId, fleetId)).find((turn) => pick(turn) && turnHasSettled(turn.status)) ?? null, timeoutMs);
}

/** The names of the tools the turn's run called, in order. */
export function toolsCalled(turn: TurnDetail): string[] {
  return (turn.tool_calls?.calls ?? []).map((call) => call.name);
}

/** The arguments of the turn's first call to `tool`, or a sentence saying the run never made it. */
export function callTo(turn: TurnDetail, tool: string): Record<string, unknown> {
  const call = turn.tool_calls?.calls.find((made) => made.name === tool);
  if (call === undefined) throw new Error(`turn ${turn.turn_id} called ${toolsCalled(turn).join(", ") || "no tool"}, never ${tool}`);
  return call.arguments;
}

/** The words a turn was asked, as its stored request carries them. */
export function messageOf(turn: TurnDetail): string {
  return MessageBody.parse(JSON.parse(turn.request_json)).message;
}
