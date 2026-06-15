import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Shared mocks ───────────────────────────────────────────────────────────
// These actions are thin forwarders: each wraps withToken((t) => apiFn(args, t)).
// We mock the token wrapper and the API client so the only thing under test is
// the argument order + token position each action threads through (the real
// network + auth boundary is proven elsewhere). vi.mock is hoisted above the
// static actions import, so every mock fn referenced inside a factory is created
// via vi.hoisted() (see runners-actions.test.ts).
const { withTokenMock, listAgentsMock, setAgentStatusMock, deleteAgentMock, installAgentMock, steerAgentMock } =
  vi.hoisted(() => ({
    withTokenMock: vi.fn(),
    listAgentsMock: vi.fn(),
    setAgentStatusMock: vi.fn(),
    deleteAgentMock: vi.fn(),
    installAgentMock: vi.fn(),
    steerAgentMock: vi.fn(),
  }));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/agents", () => ({
  listAgents: listAgentsMock,
  setAgentStatus: setAgentStatusMock,
  deleteAgent: deleteAgentMock,
  installAgent: installAgentMock,
  steerAgent: steerAgentMock,
}));

import {
  listAgentsAction,
  setAgentStatusAction,
  deleteAgentAction,
  installAgentAction,
  steerAgentAction,
} from "@/app/(dashboard)/agents/actions";

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

describe("agent server actions — thin token-forwarders", () => {
  it("listAgentsAction threads token in the middle, opts last", async () => {
    const page = { items: [], cursor: null };
    listAgentsMock.mockResolvedValueOnce(page);
    const opts = { cursor: "c1", limit: 25 };
    const r = await listAgentsAction("ws1", opts);
    expect(r).toEqual({ ok: true, data: page });
    expect(listAgentsMock).toHaveBeenCalledWith("ws1", "tok", opts);
  });

  it("listAgentsAction forwards undefined opts when none given", async () => {
    const page = { items: [], cursor: null };
    listAgentsMock.mockResolvedValueOnce(page);
    const r = await listAgentsAction("ws1");
    expect(r).toEqual({ ok: true, data: page });
    expect(listAgentsMock).toHaveBeenCalledWith("ws1", "tok", undefined);
  });

  it("setAgentStatusAction forwards ws, id, status with token last", async () => {
    const update = { agent_id: "z1", status: "stopped", config_revision: 7 };
    setAgentStatusMock.mockResolvedValueOnce(update);
    const r = await setAgentStatusAction("ws1", "z1", "stopped");
    expect(r).toEqual({ ok: true, data: update });
    expect(setAgentStatusMock).toHaveBeenCalledWith("ws1", "z1", "stopped", "tok");
  });

  it("deleteAgentAction forwards ws, id with token last", async () => {
    deleteAgentMock.mockResolvedValueOnce(undefined);
    const r = await deleteAgentAction("ws1", "z1");
    expect(r).toEqual({ ok: true, data: undefined });
    expect(deleteAgentMock).toHaveBeenCalledWith("ws1", "z1", "tok");
  });

  it("installAgentAction forwards ws, body with token last", async () => {
    const resp = { agent_id: "z1", webhook_urls: {} };
    installAgentMock.mockResolvedValueOnce(resp);
    const body = { template_id: "tmpl-1", name: "deploybot" } as never;
    const r = await installAgentAction("ws1", body);
    expect(r).toEqual({ ok: true, data: resp });
    expect(installAgentMock).toHaveBeenCalledWith("ws1", body, "tok");
  });

  it("steerAgentAction forwards ws, id, message with token last", async () => {
    steerAgentMock.mockResolvedValueOnce({ event_id: "evt-1" });
    const r = await steerAgentAction("ws1", "z1", "ship it");
    expect(r).toEqual({ ok: true, data: { event_id: "evt-1" } });
    expect(steerAgentMock).toHaveBeenCalledWith("ws1", "z1", "ship it", "tok");
  });
});
