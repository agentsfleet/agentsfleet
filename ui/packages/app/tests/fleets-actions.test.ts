import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FLEET } from "./helpers/draft-fixtures";

// ── Shared mocks ───────────────────────────────────────────────────────────
// These actions are thin forwarders: each wraps withToken((t) => apiFn(args, t)).
// We mock the token wrapper and the API client so the only thing under test is
// the argument order + token position each action threads through (the real
// network + auth boundary is proven elsewhere). vi.mock is hoisted above the
// static actions import, so every mock fn referenced inside a factory is created
// via vi.hoisted() (see runners-actions.test.ts).
const {
  withTokenMock,
  listFleetsMock,
  setFleetStatusMock,
  deleteFleetMock,
  forgetMemoryMock,
  setMemoryAccessMock,
  getFleetEventMock,
} =
  vi.hoisted(() => ({
    getFleetEventMock: vi.fn(),
    withTokenMock: vi.fn(),
    listFleetsMock: vi.fn(),
    setFleetStatusMock: vi.fn(),
    deleteFleetMock: vi.fn(),
    forgetMemoryMock: vi.fn(),
    setMemoryAccessMock: vi.fn(),
  }));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/fleets", () => ({
  listFleets: listFleetsMock,
  setFleetStatus: setFleetStatusMock,
  deleteFleet: deleteFleetMock,
}));
vi.mock("@/lib/api/memory", () => ({
  forgetMemory: forgetMemoryMock,
  setMemoryAccess: setMemoryAccessMock,
}));
vi.mock("@/lib/api/events", () => ({
  getFleetEvent: getFleetEventMock,
}));

import {
  listFleetsAction,
  setFleetStatusAction,
  deleteFleetAction,
  forgetMemoryAction,
  setMemoryAccessAction,
  getFleetEventAction,
} from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";

beforeEach(() => {
  vi.clearAllMocks();
  // withToken forwards a resolved token to its callback for the happy path,
  // wrapping the callback's result in the ok-discriminated success shape.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
});
afterEach(() => vi.resetAllMocks());

describe("fleet server actions — thin token-forwarders", () => {
  it("listFleetsAction threads token in the middle, opts last", async () => {
    const page = { items: [], cursor: null };
    listFleetsMock.mockResolvedValueOnce(page);
    const opts = { cursor: "c1", limit: 25 };
    const r = await listFleetsAction("ws1", opts);
    expect(r).toEqual({ ok: true, data: page });
    expect(listFleetsMock).toHaveBeenCalledWith("ws1", "tok", opts);
  });

  it("listFleetsAction forwards undefined opts when none given", async () => {
    const page = { items: [], cursor: null };
    listFleetsMock.mockResolvedValueOnce(page);
    const r = await listFleetsAction("ws1");
    expect(r).toEqual({ ok: true, data: page });
    expect(listFleetsMock).toHaveBeenCalledWith("ws1", "tok", undefined);
  });

  // The detail read the events list no longer carries. The identifier is the
  // only argument the row supplies, so its position is what a regression here
  // would scramble; the body it returns is the payload the list dropped.
  it("getFleetEventAction forwards ws, fleet and event with token last", async () => {
    const detail = { event_id: "e1", request_json: "{}", response_text: "done" };
    getFleetEventMock.mockResolvedValueOnce(detail);
    const r = await getFleetEventAction("ws1", "z1", "e1");
    expect(r).toEqual({ ok: true, data: detail });
    expect(getFleetEventMock).toHaveBeenCalledWith("ws1", "z1", "e1", "tok");
  });

  it("setFleetStatusAction forwards ws, id, status with token last", async () => {
    const update = { fleet_id: "z1", status: "stopped", config_revision: 7 };
    setFleetStatusMock.mockResolvedValueOnce(update);
    const r = await setFleetStatusAction("ws1", "z1", "stopped", '"observed"');
    expect(r).toEqual({ ok: true, data: update });
    expect(setFleetStatusMock).toHaveBeenCalledWith("ws1", "z1", "stopped", '"observed"', "tok");
  });

  it("deleteFleetAction preserves the observed version with token last", async () => {
    deleteFleetMock.mockResolvedValueOnce(undefined);
    const r = await deleteFleetAction("ws1", "z1", FLEET.etag);
    expect(r).toEqual({ ok: true, data: undefined });
    expect(deleteFleetMock).toHaveBeenCalledWith("ws1", "z1", FLEET.etag, "tok");
  });

  it("forgetMemoryAction forwards the memory key without exposing content", async () => {
    forgetMemoryMock.mockResolvedValueOnce(undefined);
    const r = await forgetMemoryAction("ws1", "z1", "style");
    expect(r).toEqual({ ok: true, data: undefined });
    expect(forgetMemoryMock).toHaveBeenCalledWith("ws1", "z1", "style", "tok");
  });

  it("setMemoryAccessAction forwards the grant change with token last and returns both grants", async () => {
    setMemoryAccessMock.mockResolvedValueOnce({ read: true, publish: false });
    const r = await setMemoryAccessAction("ws1", "z1", { read: true });
    expect(r).toEqual({ ok: true, data: { read: true, publish: false } });
    expect(setMemoryAccessMock).toHaveBeenCalledWith("ws1", "z1", { read: true }, "tok");
  });

});
