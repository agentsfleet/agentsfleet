import { describe, expect, it, vi } from "vitest";

const listFleetEventsMock = vi.hoisted(() => vi.fn());
const listFleetMessagesMock = vi.hoisted(() => vi.fn());
const listApprovalsMock = vi.hoisted(() => vi.fn());
const listAllMemoriesMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api/events", () => ({
  listFleetEvents: listFleetEventsMock,
  listFleetMessages: listFleetMessagesMock,
}));
vi.mock("@/lib/api/approvals", () => ({ listApprovals: listApprovalsMock }));
vi.mock("@/lib/api/memory", () => ({ listAllMemories: listAllMemoriesMock }));

import { CHAT_TURNS, startViewData } from "./view-data";
import { FLEET_VIEW } from "./FleetSubnavigation";

const ARGS = {
  workspaceId: "ws_1",
  fleetId: "zom_1",
  token: "tok",
  eventsCursor: null,
  eventsPageSize: 25,
};

function resetMocks() {
  for (const mock of [
    listFleetEventsMock,
    listFleetMessagesMock,
    listApprovalsMock,
    listAllMemoriesMock,
  ]) {
    mock.mockReset();
    mock.mockResolvedValue({ items: [], next_cursor: null });
  }
}

describe("startViewData", () => {
  it("test_chat_single_thread_fetch: chat starts ONE thread read and no event fan-out", () => {
    resetMocks();
    const data = startViewData(FLEET_VIEW.chat, ARGS);

    // The fetches are issued synchronously from route params — nothing here
    // waited on the fleet detail read.
    expect(listFleetMessagesMock).toHaveBeenCalledTimes(1);
    expect(listFleetMessagesMock).toHaveBeenCalledWith("ws_1", "zom_1", "tok", {
      limit: CHAT_TURNS,
    });
    expect(listApprovalsMock).toHaveBeenCalledTimes(1);
    // The retired shape: an events-list read followed by per-turn detail reads.
    expect(listFleetEventsMock).not.toHaveBeenCalled();
    expect(data.thread).toBeDefined();
    expect(data.approvals).toBeDefined();
  });

  it("test_detail_view_loaders_concurrent: events view fetch starts from route params alone", () => {
    resetMocks();
    const data = startViewData(FLEET_VIEW.events, {
      ...ARGS,
      eventsCursor: "cur_1",
    });
    expect(listFleetEventsMock).toHaveBeenCalledTimes(1);
    expect(listFleetEventsMock).toHaveBeenCalledWith("ws_1", "zom_1", "tok", {
      limit: 25,
      cursor: "cur_1",
    });
    expect(data.eventsInitial).toBeDefined();
  });

  it("memory view starts its walk from route params alone", () => {
    resetMocks();
    startViewData(FLEET_VIEW.memory, ARGS);
    expect(listAllMemoriesMock).toHaveBeenCalledTimes(1);
    expect(listAllMemoriesMock).toHaveBeenCalledWith("ws_1", "zom_1", "tok");
  });

  it("skill and trigger views fetch nothing ahead of the fleet", () => {
    resetMocks();
    expect(startViewData(FLEET_VIEW.skill, ARGS)).toEqual({});
    expect(startViewData(FLEET_VIEW.trigger, ARGS)).toEqual({});
    expect(listFleetMessagesMock).not.toHaveBeenCalled();
    expect(listFleetEventsMock).not.toHaveBeenCalled();
    expect(listAllMemoriesMock).not.toHaveBeenCalled();
  });

  it("a failed thread read degrades to null instead of failing the page", async () => {
    resetMocks();
    listFleetMessagesMock.mockRejectedValue(new Error("upstream down"));
    listApprovalsMock.mockRejectedValue(new Error("upstream down"));
    const data = startViewData(FLEET_VIEW.chat, ARGS);
    await expect(data.thread).resolves.toBeNull();
    await expect(data.approvals).resolves.toBeNull();
  });
});
