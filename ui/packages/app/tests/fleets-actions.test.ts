import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Shared mocks ───────────────────────────────────────────────────────────
// These actions are thin forwarders: each wraps withToken((t) => apiFn(args, t)).
// We mock the token wrapper and the API client so the only thing under test is
// the argument order + token position each action threads through (the real
// network + auth boundary is proven elsewhere). vi.mock is hoisted above the
// static actions import, so every mock fn referenced inside a factory is created
// via vi.hoisted() (see runners-actions.test.ts).
const { withTokenMock, listFleetsMock, setFleetStatusMock, deleteFleetMock, installFleetMock, steerFleetMock } =
  vi.hoisted(() => ({
    withTokenMock: vi.fn(),
    listFleetsMock: vi.fn(),
    setFleetStatusMock: vi.fn(),
    deleteFleetMock: vi.fn(),
    installFleetMock: vi.fn(),
    steerFleetMock: vi.fn(),
  }));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/fleets", () => ({
  listFleets: listFleetsMock,
  setFleetStatus: setFleetStatusMock,
  deleteFleet: deleteFleetMock,
  installFleet: installFleetMock,
  steerFleet: steerFleetMock,
}));

import {
  listFleetsAction,
  setFleetStatusAction,
  deleteFleetAction,
  installFleetAction,
  steerFleetAction,
} from "@/app/(dashboard)/fleets/actions";

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

  it("setFleetStatusAction forwards ws, id, status with token last", async () => {
    const update = { fleet_id: "z1", status: "stopped", config_revision: 7 };
    setFleetStatusMock.mockResolvedValueOnce(update);
    const r = await setFleetStatusAction("ws1", "z1", "stopped");
    expect(r).toEqual({ ok: true, data: update });
    expect(setFleetStatusMock).toHaveBeenCalledWith("ws1", "z1", "stopped", "tok");
  });

  it("deleteFleetAction forwards ws, id with token last", async () => {
    deleteFleetMock.mockResolvedValueOnce(undefined);
    const r = await deleteFleetAction("ws1", "z1");
    expect(r).toEqual({ ok: true, data: undefined });
    expect(deleteFleetMock).toHaveBeenCalledWith("ws1", "z1", "tok");
  });

  it("installFleetAction forwards ws + platform-template body with token last", async () => {
    const resp = { fleet_id: "z1", status: "installing" };
    installFleetMock.mockResolvedValueOnce(resp);
    const body = { platform_template_id: "github-pr-reviewer", name: "deploybot" };
    const r = await installFleetAction("ws1", body);
    expect(r).toEqual({ ok: true, data: resp });
    expect(installFleetMock).toHaveBeenCalledWith("ws1", body, "tok");
  });

  it("installFleetAction forwards a tenant-template body unchanged", async () => {
    const resp = { fleet_id: "z2", status: "installing" };
    installFleetMock.mockResolvedValueOnce(resp);
    const body = { tenant_template_id: "01932d4e-7c10-7a3a-9f00-000000000001" };
    const r = await installFleetAction("ws1", body);
    expect(r).toEqual({ ok: true, data: resp });
    expect(installFleetMock).toHaveBeenCalledWith("ws1", body, "tok");
  });

  it("steerFleetAction forwards ws, id, message with token last", async () => {
    steerFleetMock.mockResolvedValueOnce({ event_id: "evt-1" });
    const r = await steerFleetAction("ws1", "z1", "ship it");
    expect(r).toEqual({ ok: true, data: { event_id: "evt-1" } });
    expect(steerFleetMock).toHaveBeenCalledWith("ws1", "z1", "ship it", "tok");
  });
});
