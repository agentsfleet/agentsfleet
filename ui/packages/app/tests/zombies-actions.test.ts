import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Shared mocks ───────────────────────────────────────────────────────────
// These actions are thin forwarders: each wraps withToken((t) => apiFn(args, t)).
// We mock the token wrapper and the API client so the only thing under test is
// the argument order + token position each action threads through (the real
// network + auth boundary is proven elsewhere). vi.mock is hoisted above the
// static actions import, so every mock fn referenced inside a factory is created
// via vi.hoisted() (see runners-actions.test.ts).
const { withTokenMock, listZombiesMock, setZombieStatusMock, deleteZombieMock, installZombieMock, steerZombieMock } =
  vi.hoisted(() => ({
    withTokenMock: vi.fn(),
    listZombiesMock: vi.fn(),
    setZombieStatusMock: vi.fn(),
    deleteZombieMock: vi.fn(),
    installZombieMock: vi.fn(),
    steerZombieMock: vi.fn(),
  }));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/zombies", () => ({
  listZombies: listZombiesMock,
  setZombieStatus: setZombieStatusMock,
  deleteZombie: deleteZombieMock,
  installZombie: installZombieMock,
  steerZombie: steerZombieMock,
}));

import {
  listZombiesAction,
  setZombieStatusAction,
  deleteZombieAction,
  installZombieAction,
  steerZombieAction,
} from "@/app/(dashboard)/zombies/actions";

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

describe("zombie server actions — thin token-forwarders", () => {
  it("listZombiesAction threads token in the middle, opts last", async () => {
    const page = { items: [], cursor: null };
    listZombiesMock.mockResolvedValueOnce(page);
    const opts = { cursor: "c1", limit: 25 };
    const r = await listZombiesAction("ws1", opts);
    expect(r).toEqual({ ok: true, data: page });
    expect(listZombiesMock).toHaveBeenCalledWith("ws1", "tok", opts);
  });

  it("listZombiesAction forwards undefined opts when none given", async () => {
    const page = { items: [], cursor: null };
    listZombiesMock.mockResolvedValueOnce(page);
    const r = await listZombiesAction("ws1");
    expect(r).toEqual({ ok: true, data: page });
    expect(listZombiesMock).toHaveBeenCalledWith("ws1", "tok", undefined);
  });

  it("setZombieStatusAction forwards ws, id, status with token last", async () => {
    const update = { zombie_id: "z1", status: "stopped", config_revision: 7 };
    setZombieStatusMock.mockResolvedValueOnce(update);
    const r = await setZombieStatusAction("ws1", "z1", "stopped");
    expect(r).toEqual({ ok: true, data: update });
    expect(setZombieStatusMock).toHaveBeenCalledWith("ws1", "z1", "stopped", "tok");
  });

  it("deleteZombieAction forwards ws, id with token last", async () => {
    deleteZombieMock.mockResolvedValueOnce(undefined);
    const r = await deleteZombieAction("ws1", "z1");
    expect(r).toEqual({ ok: true, data: undefined });
    expect(deleteZombieMock).toHaveBeenCalledWith("ws1", "z1", "tok");
  });

  it("installZombieAction forwards ws, body with token last", async () => {
    const resp = { zombie_id: "z1", webhook_urls: {} };
    installZombieMock.mockResolvedValueOnce(resp);
    const body = { template_id: "tmpl-1", name: "deploybot" } as never;
    const r = await installZombieAction("ws1", body);
    expect(r).toEqual({ ok: true, data: resp });
    expect(installZombieMock).toHaveBeenCalledWith("ws1", body, "tok");
  });

  it("steerZombieAction forwards ws, id, message with token last", async () => {
    steerZombieMock.mockResolvedValueOnce({ event_id: "evt-1" });
    const r = await steerZombieAction("ws1", "z1", "ship it");
    expect(r).toEqual({ ok: true, data: { event_id: "evt-1" } });
    expect(steerZombieMock).toHaveBeenCalledWith("ws1", "z1", "ship it", "tok");
  });
});
