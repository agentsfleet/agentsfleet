import type { TurnDetail } from "@/lib/api/turns";
import { WORKSPACE } from "@/tests/helpers/fleet-fixtures";

/** The fleet a first message makes on the probe's New fleet page, and the answer its first turn records. */
export const NEW_FLEET = {
  id: "0199a801-1000-7000-8000-0000000000f1",
  turnId: "0199a801-1000-7000-8000-0000000000f2",
  name: "Support triage",
  answer: "I read the three newest support tickets. Two are the same checkout timeout; I grouped them and drafted the reply.",
} as const;

/** The first turn as `GET F/messages` returns it once the run answered: the person's words, then the fleet's reply. */
export function newFleetTurn(base: TurnDetail, message: string): TurnDetail {
  const at = new Date().toISOString();
  return {...base, workspace_id: WORKSPACE, fleet_id: NEW_FLEET.id, turn_id: NEW_FLEET.turnId, actor: "steer:fixture-owner",
    event_type: "chat", status: "processed", request_json: JSON.stringify({message}), response_text: NEW_FLEET.answer,
    tool_calls: null, created_at: at, updated_at: at};
}
