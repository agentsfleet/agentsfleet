import { describe, expect, it } from "vitest";
import type { LiveFrame, TurnRow } from "@/lib/api/turns";
import { FRAME_KIND } from "@/lib/api/turns-types";
import { ACTOR } from "@/lib/turns/turn-summary";
import { convertTurn } from "@/components/domain/conversation/stream/useFleetTurnStream";
import { toReplyMessage } from "@/components/domain/activity/replies/fleetReplyMessage";
import { applyLiveFrame, mergeBackfill, parseLiveFrame } from "./fleet-stream-frames";
import { foldRow, optimisticRow } from "./fleet-stream-optimistic";
import { AGENTSFLEET_TURN_STATUS, FOLDED_OUTCOME, type FleetTurn } from "./fleet-stream-row";
import { row } from "@/tests/helpers/fleet-stream-registry-fixtures";

const TURN_ID = "1725000000000-3";
const TEAMMATE = `${ACTOR.STEER_PREFIX}user_bob`;
const TYPED = "check the tests";
const ADMITTED_AT = 1_725_000_000_000;
const RECEIVED_AT = ADMITTED_AT + 5_000;

function admitted(message: string | null = TYPED): LiveFrame {
  return {
    kind: FRAME_KIND.TURN_ADMITTED,
    turn_id: TURN_ID,
    actor: TEAMMATE,
    event_type: "chat",
    ...(message === null ? {} : { message }),
    created_at: new Date(ADMITTED_AT).toISOString(),
  };
}

function received(message?: string): LiveFrame {
  return {
    kind: FRAME_KIND.TURN_RECEIVED,
    turn_id: TURN_ID,
    actor: TEAMMATE,
    event_type: "chat",
    created_at: new Date(RECEIVED_AT).toISOString(),
    ...(message === undefined ? {} : { message }),
  };
}

function fold(frames: LiveFrame[]): FleetTurn[] {
  return frames.reduce<FleetTurn[]>((rows, frame) => applyLiveFrame(rows, frame), []);
}

describe("a message waiting for a runner", () => {
  it("test_admitted_frame_renders_queued_row", () => {
    const [row, ...rest] = fold([admitted()]);
    expect(rest).toEqual([]);
    expect(row).toMatchObject({
      id: TURN_ID,
      role: "user",
      actor: TEAMMATE,
      text: TYPED,
      status: AGENTSFLEET_TURN_STATUS.QUEUED,
    });
    expect(row?.createdAt.getTime()).toBe(ADMITTED_AT);
    // It renders as a turn whose reply is queued: running, marked queued.
    const message = convertTurn(row as FleetTurn);
    expect(message.metadata?.custom?.queued).toBe(true);
    expect(toReplyMessage(message, row as FleetTurn).status).toEqual({ type: "running" });
  });

  it("test_received_moves_queued_row", () => {
    const rows = fold([admitted(), received(TYPED)]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ text: TYPED, status: AGENTSFLEET_TURN_STATUS.RECEIVED });
    expect(rows[0]?.createdAt.getTime()).toBe(RECEIVED_AT);
    expect(convertTurn(rows[0] as FleetTurn).metadata?.custom?.queued).toBe(false);
  });

  it("test_late_admitted_frame_keeps_status", () => {
    // A runner held the send and announced it before the route published.
    const rows = fold([received(), admitted()]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ text: TYPED, status: AGENTSFLEET_TURN_STATUS.RECEIVED });
  });

  it("keeps a row's own text over a late admitted frame's", () => {
    const rows = fold([received("what the row already says"), admitted()]);
    expect(rows[0]?.text).toBe("what the row already says");
  });

  it("names the typed words on a received frame that opens the row", () => {
    const rows = fold([received(TYPED)]);
    expect(rows[0]).toMatchObject({ text: TYPED, status: AGENTSFLEET_TURN_STATUS.RECEIVED });
  });

  it("opens a row with no text when the frame carried none", () => {
    expect(fold([admitted(null)])[0]?.text).toBe("");
  });

  it("fills an empty text from a received frame on a row already running", () => {
    // The first received frame carried no message; a redelivered one does.
    const rows = fold([received(), received(TYPED)]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ text: TYPED, status: AGENTSFLEET_TURN_STATUS.RECEIVED });
  });

  it("stamps a waiting row with the browser clock when the frame has no instant", () => {
    const before = Date.now();
    const { created_at: _dropped, ...undated } = admitted() as Extract<LiveFrame, { created_at?: string }>;
    const [row] = fold([undated as LiveFrame]);
    expect(row?.createdAt.getTime()).toBeGreaterThanOrEqual(before);
  });
});

// A message past the frame bound rides as its head; the turn holds it whole.
describe("a message its frame carried only the head of", () => {
  const HEAD = "check the tests in";
  const WHOLE = `${HEAD} every package, then report`;

  // Off the wire as the EventSource listener reads it, so a malformed flag
  // needs no cast.
  function wire(over: Record<string, unknown>, base: LiveFrame = admitted(HEAD)): LiveFrame {
    const frame = parseLiveFrame(JSON.stringify({ ...base, ...over }));
    if (frame === null) throw new Error("not a frame");
    return frame;
  }

  // The saved row; the turns list carries neither body.
  function saved(requestJson: string | null): TurnRow {
    const detail = row({ turn_id: TURN_ID, actor: TEAMMATE, created_at: new Date(RECEIVED_AT).toISOString(), request_json: requestJson ?? "{}", response_text: "Done." });
    if (requestJson !== null) return detail;
    const { request_json: _request, response_text: _response, ...listRow } = detail;
    return listRow;
  }

  it("marks a waiting row whose admitted frame says it was trimmed", () => {
    expect(fold([wire({ message_trimmed: true })])[0]).toMatchObject({ text: HEAD, textTrimmed: true });
  });

  it("marks an empty text an admitted frame fills with a head", () => {
    const rows = fold([received(), wire({ message_trimmed: true })]);
    expect(rows[0]).toMatchObject({ text: HEAD, textTrimmed: true, status: AGENTSFLEET_TURN_STATUS.RECEIVED });
  });

  it("marks a row a trimmed received frame opens or fills", () => {
    const head = wire({ message_trimmed: true }, received(HEAD));
    expect(fold([head])[0]).toMatchObject({ text: HEAD, textTrimmed: true });
    expect(fold([admitted(null), head])[0]).toMatchObject({ text: HEAD, textTrimmed: true });
  });

  it.each([undefined, false, "true", 1, null])("leaves the row unmarked when the flag is %j", (flag) => {
    const [shown] = fold([wire({ message_trimmed: flag })]);
    expect(shown?.text).toBe(HEAD);
    expect(shown).not.toHaveProperty("textTrimmed");
  });

  it("marks neither an empty message nor a row that already holds its own text", () => {
    expect(fold([wire({ message: "", message_trimmed: true })])[0]).not.toHaveProperty("textTrimmed");
    const kept = fold([received("what the row already says"), wire({ message_trimmed: true })])[0];
    expect(kept).toMatchObject({ text: "what the row already says" });
    expect(kept).not.toHaveProperty("textTrimmed");
  });

  it("keeps the head marked over a list row, which carries no message", () => {
    const [merged] = mergeBackfill(fold([wire({ message_trimmed: true })]), [saved(null)]);
    expect(merged).toMatchObject({ text: HEAD, textTrimmed: true, status: "processed" });
  });

  it("takes the saved row's whole message and drops the mark", () => {
    const [merged] = mergeBackfill(fold([wire({ message_trimmed: true })]), [saved(JSON.stringify({ message: WHOLE }))]);
    expect(merged).toMatchObject({ text: WHOLE, reply: "Done." });
    expect(merged?.textTrimmed).toBeUndefined();
  });

  it("keeps the head marked when the saved body names no message", () => {
    const [merged] = mergeBackfill(fold([wire({ message_trimmed: true })]), [saved("not json")]);
    expect(merged).toMatchObject({ text: HEAD, textTrimmed: true });
  });
});

// The 202 named a live run, which ended before it was assigned the message; the
// fleet's next run starts the message under its own turn.
describe("a send labelled folded whose run never took it", () => {
  function labelled(): FleetTurn[] {
    return foldRow([optimisticRow("temp", TYPED, TEAMMATE)], "temp", TURN_ID);
  }

  it("starts on its received frame and drops the label", () => {
    const rows = labelled();
    expect(rows[0]).toMatchObject({ status: AGENTSFLEET_TURN_STATUS.FOLDED, outcome: FOLDED_OUTCOME });

    const [started, ...rest] = applyLiveFrame(rows, received());

    expect(rest).toEqual([]);
    expect(started).toMatchObject({ id: TURN_ID, text: TYPED, status: AGENTSFLEET_TURN_STATUS.RECEIVED });
    expect(started?.outcome).not.toBe(FOLDED_OUTCOME);
  });

  it("reads as a running reply once its own run starts", () => {
    const [started] = applyLiveFrame(labelled(), received());
    const row = started as FleetTurn;
    expect(toReplyMessage(convertTurn(row), row).status).toEqual({ type: "running" });
  });
});
