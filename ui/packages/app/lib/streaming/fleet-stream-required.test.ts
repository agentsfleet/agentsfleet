import { describe, expect, it } from "vitest";
import { FRAME_KIND } from "@/lib/api/events-types";
import { applyLiveFrame, parseLiveFrame } from "./fleet-stream-frames";
import { readSavedTrace } from "./fleet-stream-tool-trace";
import { evt } from "@/tests/helpers/fleet-stream-fixtures";

const FIXTURE_CLOCK_MS = 1000;
const event = {event_id: "e1", name: "shell", ms: 12, status: "succeeded"};
const seed = [evt({id: "e1"})];
function apply(payload: Record<string, unknown>) {
  const frame = parseLiveFrame(JSON.stringify(payload));
  return frame === null ? seed : applyLiveFrame(seed, frame, FIXTURE_CLOCK_MS);
}
describe("required stream fields", () => {
  it.each([undefined, null, "", 7])("drops a tool frame with invalid call_id %s", (call_id) => {
    expect(apply({...event, kind: FRAME_KIND.TOOL_CALL_COMPLETED, call_id})).toBe(seed);
  });
  it.each([undefined, null, "ok", 7])("drops a completion with invalid status %s", (status) => {
    expect(apply({...event, kind: FRAME_KIND.TOOL_CALL_COMPLETED, call_id: "1:1", status})).toBe(seed);
  });
  it.each([undefined, null, "unknown", 7])("drops a chunk with invalid text_kind %s", (text_kind) => {
    expect(apply({kind: FRAME_KIND.CHUNK, event_id: "e1", text: "answer", text_kind})).toBe(seed);
  });
  it.each([undefined, null, "ok", 7])("counts a saved call with invalid status %s as omitted", (status) => {
    const call = {call_id: "1:1", name: "shell", arguments: {}, duration_ms: 12, status};
    expect(readSavedTrace({calls: [call], omitted_call_count: 0}, FIXTURE_CLOCK_MS)).toEqual({calls: [], omitted: 1});
  });
});
