import { describe, expect, it } from "vitest";
import type { TurnRow } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { applyLiveFrame, mergeBackfill } from "./fleet-stream-frames";
import { AGENTSFLEET_TURN_STATUS, rowToTurn } from "./fleet-stream-row";
import { getSnapshot, reconcileServerRows, subscribe } from "@/tests/helpers/fleet-streams";
import { row } from "@/tests/helpers/fleet-stream-fixtures";
import { setupRegistryTests, WS, Z_A } from "@/tests/helpers/fleet-stream-registry-fixtures";

// A fleet page opens while its first message is still running. The rows the
// thread holds by then can carry no body — the header subscribed with none, a
// list page rebuilt the row, a frame opened it — and the message the person
// sent must still show once any read carries it.

const TURN = "01a210a6-946b-7838-8471-7de5355eed3e";
const ACTOR = "steer:user_viewer";
const MESSAGE = "watch checkout errors";
const BODY = JSON.stringify({ message: MESSAGE });

const served = row({ turn_id: TURN, actor: ACTOR, event_type: "chat", status: AGENTSFLEET_TURN_STATUS.RECEIVED, request_json: BODY, response_text: null });

/** The same turn as the turns list serves it: no bodies. */
function listed(): TurnRow {
  const { request_json: _request, response_text: _response, ...rest } = served;
  return rest;
}

setupRegistryTests();

describe("a running row's trigger body", () => {
  it("a running row rebuilt from a list page shows the message once a read carries it", () => {
    const release = subscribe(WS, Z_A, [], () => {});
    reconcileServerRows(Z_A, [listed()]);
    expect(getSnapshot(Z_A).turns[0]?.text).toBe("");
    reconcileServerRows(Z_A, [served]);
    expect(getSnapshot(Z_A).turns[0]).toMatchObject({ text: MESSAGE, status: AGENTSFLEET_TURN_STATUS.RECEIVED, custom: { requestJson: BODY } });
    release();
  });

  it("a frame's head of the message gives way to the whole the read carries", () => {
    const opened = applyLiveFrame([], {
      kind: FRAME_KIND.TURN_RECEIVED, turn_id: TURN, actor: ACTOR, event_type: "chat",
      created_at: served.created_at, message: "watch", message_trimmed: true,
    });
    expect(opened[0]).toMatchObject({ text: "watch", textTrimmed: true });
    const merged = mergeBackfill(opened, [served]);
    expect(merged[0]).toMatchObject({ text: MESSAGE, status: AGENTSFLEET_TURN_STATUS.RECEIVED });
    expect(merged[0]?.textTrimmed).toBeUndefined();
  });

  it("a row that holds its body, or a read that carries none, leaves the row as it is", () => {
    const held = [rowToTurn(served)];
    const restated = row({ ...served, request_json: JSON.stringify({ message: "another text" }) });
    expect(mergeBackfill(held, [restated])).toBe(held);
    const opened = applyLiveFrame([], { kind: FRAME_KIND.TURN_RECEIVED, turn_id: TURN, actor: ACTOR, created_at: served.created_at });
    expect(mergeBackfill(opened, [listed()])).toBe(opened);
    expect(mergeBackfill(opened, [row({ ...served, request_json: "{}" })])).toBe(opened);
    expect(mergeBackfill(opened, [{ ...served, turn_id: "another-turn" }])[1]).toBe(opened[0]);
  });
});
