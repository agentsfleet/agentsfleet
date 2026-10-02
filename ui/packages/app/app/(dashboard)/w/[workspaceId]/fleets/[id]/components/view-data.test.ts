import { describe, expect, it, vi } from "vitest";

const listFleetEventsMock = vi.hoisted(() => vi.fn());
const listFleetMessagesMock = vi.hoisted(() => vi.fn());
const listAllMemoriesMock = vi.hoisted(() => vi.fn());
const listWorkspaceMembersMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api/events", () => ({
  listFleetEvents: listFleetEventsMock,
  listFleetMessages: listFleetMessagesMock,
}));
vi.mock("@/lib/api/memory", () => ({ listAllMemories: listAllMemoriesMock }));
vi.mock("@/lib/api/tenant-members", () => ({ listWorkspaceMembers: listWorkspaceMembersMock }));

import { CHAT_TURNS, startViewData, type ChatViewData } from "./view-data";
import { FLEET_VIEW } from "./FleetSubnavigation";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";
import type { WorkspaceMember } from "@/lib/api/tenant-members";

const CURSOR = "cur_1";
const ARGS = {
  workspaceId: "ws_1",
  fleetId: "zom_1",
  token: "tok",
  eventsCursor: null,
  eventsPageSize: 25,
};

const EMPTY_PAGE = { items: [], next_cursor: null };
const UPSTREAM_DOWN = "upstream down";

// The chat's data, narrowed by its tag rather than cast.
function startChat(): ChatViewData {
  const data = startViewData(FLEET_VIEW.chat, ARGS);
  if (data.view !== FLEET_VIEW.chat) throw new Error(`chat started the ${data.view} view's data`);
  return data;
}

function resetMocks() {
  for (const mock of [
    listFleetEventsMock,
    listFleetMessagesMock,
    listAllMemoriesMock,
    listWorkspaceMembersMock,
  ]) {
    mock.mockReset();
    mock.mockResolvedValue(EMPTY_PAGE);
  }
}

describe("startViewData", () => {
  it("test_chat_single_thread_fetch: chat starts ONE thread read and no event fan-out", () => {
    resetMocks();
    const data = startViewData(FLEET_VIEW.chat, ARGS);

    // The fetches are issued synchronously from route params — nothing here
    // waited on the fleet detail read.
    expect(listFleetMessagesMock).toHaveBeenCalledTimes(1);
    expect(listFleetMessagesMock).toHaveBeenCalledWith(ARGS.workspaceId, ARGS.fleetId, ARGS.token, {
      limit: CHAT_TURNS,
    });
    // The retired shape: an events-list read followed by per-turn detail reads.
    expect(listFleetEventsMock).not.toHaveBeenCalled();
    // The tag is what lets the chat loader take the thread fields directly.
    expect(data.view).toBe(FLEET_VIEW.chat);
    expect(data).toHaveProperty("thread");
  });

  it("test_detail_view_loaders_concurrent: events view fetch starts from route params alone", () => {
    resetMocks();
    const data = startViewData(FLEET_VIEW.events, {
      ...ARGS,
      eventsCursor: CURSOR,
    });
    expect(listFleetEventsMock).toHaveBeenCalledTimes(1);
    expect(listFleetEventsMock).toHaveBeenCalledWith(ARGS.workspaceId, ARGS.fleetId, ARGS.token, {
      limit: 25,
      cursor: CURSOR,
    });
    expect(data.view).toBe(FLEET_VIEW.events);
    expect(data).toHaveProperty("eventsInitial");
  });

  it("memory view starts its walk from route params alone", () => {
    resetMocks();
    startViewData(FLEET_VIEW.memory, ARGS);
    expect(listAllMemoriesMock).toHaveBeenCalledTimes(1);
    expect(listAllMemoriesMock).toHaveBeenCalledWith(ARGS.workspaceId, ARGS.fleetId, ARGS.token);
  });

  it("skill and trigger views fetch nothing ahead of the fleet", () => {
    resetMocks();
    expect(startViewData(FLEET_VIEW.skill, ARGS)).toEqual({
      view: FLEET_VIEW.skill,
    });
    expect(startViewData(FLEET_VIEW.trigger, ARGS)).toEqual({
      view: FLEET_VIEW.trigger,
    });
    expect(listFleetMessagesMock).not.toHaveBeenCalled();
    expect(listFleetEventsMock).not.toHaveBeenCalled();
    expect(listAllMemoriesMock).not.toHaveBeenCalled();
    expect(listWorkspaceMembersMock).not.toHaveBeenCalled();
  });

  it("a failed thread read degrades to null instead of failing the page", async () => {
    resetMocks();
    listFleetMessagesMock.mockRejectedValue(new Error(UPSTREAM_DOWN));
    const data = startChat();
    await expect(data.thread).resolves.toBeNull();
  });

  it("should start the members read beside the thread read and hand over who it names when the chat opens", async () => {
    resetMocks();
    const bob: WorkspaceMember = { user_id: "user_bob", display_name: "Bob", role: ACCOUNT_ROLE.member, actor: "steer:user_bob" };
    listWorkspaceMembersMock.mockResolvedValue([bob]);
    const data = startChat();
    expect(listWorkspaceMembersMock).toHaveBeenCalledExactlyOnceWith(ARGS.workspaceId, ARGS.token);
    await expect(data.members).resolves.toEqual([bob]);
  });

  it("should degrade the members to null when the members read fails", async () => {
    resetMocks();
    listWorkspaceMembersMock.mockRejectedValue(new Error(UPSTREAM_DOWN));
    const data = startChat();
    await expect(data.members).resolves.toBeNull();
    await expect(data.thread).resolves.toEqual(EMPTY_PAGE);
  });
});
