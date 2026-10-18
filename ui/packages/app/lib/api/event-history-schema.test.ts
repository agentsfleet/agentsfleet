import { afterEach, expect, it, vi } from "vitest";
import { row } from "@/tests/helpers/event-summary-fixtures";
import { listFleetEvents, listWorkspaceEvents } from "./events";

const fetchMock = vi.fn<typeof fetch>();
vi.stubGlobal("fetch", fetchMock);
afterEach(() => fetchMock.mockReset());
const readers = [() => listFleetEvents("workspace", "fleet", "session"), () => listWorkspaceEvents("workspace", "session")];

it.each(readers)("accepts recorded facts and future event names without losing precision", async (read) => {
  const event = row({event_type: "future_trigger", status: "future_status", cost_nanos: 1, created_at: "2026-10-19T00:00:00.125Z"});
  fetchMock.mockResolvedValue(Response.json({items: [event], next_cursor: "next"}));
  const page = await read();
  expect(page.items[0]).toMatchObject({event_type: "future_trigger", status: "future_status", cost_nanos: 1, created_at: event.created_at});
  expect(page.next_cursor).toBe("next");
});

it.each([null, {}, {items: [], next_cursor: 1}, {items: [row({created_at: "bad"})], next_cursor: null},
  {items: [{...row(), updated_at: Date.now()}], next_cursor: null},
  {items: [{...row(), tokens: "12"}], next_cursor: null}])("refuses malformed activity without turning it into an empty result", async (body) => {
  fetchMock.mockImplementation(async () => Response.json(body));
  for (const read of readers) await expect(read()).rejects.toThrow();
});

it("keeps omitted optional measurements unknown", async () => {
  fetchMock.mockResolvedValue(Response.json({items: [{event_id: "e", fleet_id: "f", workspace_id: "w", actor: "fleet", event_type: "chat",
    status: "received", created_at: "2026-10-19T00:00:00Z", updated_at: "2026-10-19T00:00:00Z"}], next_cursor: null}));
  expect((await listWorkspaceEvents("w", "session")).items[0]).toMatchObject({tokens: null, wall_ms: null, cost_nanos: null, failure_detail: null});
});
