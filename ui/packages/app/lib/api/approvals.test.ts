import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  approveApproval,
  denyApproval,
  getApproval,
  listApprovals,
  type ApprovalGate,
} from "./approvals";

// Constants — RULE UFS. URL fragments + tokens reused across multiple tests.
const WORKSPACE_ID = "ws_test_001";
const TOKEN = "token_abc";
const GATE_ID = "01999999-0000-7000-8000-000000000001";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701";
const PATH_PREFIX = `/v1/workspaces/${WORKSPACE_ID}/approvals`;
const BACKEND_BASE = "/backend";

const fetchMock = vi.fn();

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  fetchMock.mockReset();
  vi.unstubAllGlobals();
});

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function gateFixture(over: Partial<ApprovalGate> = {}): ApprovalGate {
  return {
    gate_id: GATE_ID,
    fleet_id: FLEET_ID,
    fleet_name: "approvals-a",
    workspace_id: WORKSPACE_ID,
    action_id: "act_001",
    tool_name: "write_repo",
    action_name: "create_pr",
    gate_kind: "destructive_action",
    proposed_action: "Open PR titled wire approval inbox",
    evidence: { files: ["a", "b"], loc: 42 },
    blast_radius: "single repo branch",
    status: "pending",
    detail: "",
    created_at: 1_700_000_000_000,
    timeout_at: 1_700_086_400_000,
    updated_at: null,
    resolved_by: "",
    resolved_by_name: "",
    ...over,
  };
}

// ── listApprovals ──────────────────────────────────────────────────────

describe("listApprovals", () => {
  it("calls /v1/workspaces/{ws}/approvals with no querystring by default", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ items: [gateFixture()], next_cursor: null }),
    );
    const result = await listApprovals(WORKSPACE_ID, TOKEN);
    expect(fetchMock).toHaveBeenCalledWith(
      `${BACKEND_BASE}${PATH_PREFIX}`,
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: `Bearer ${TOKEN}` }),
      }),
    );
    expect(result.items).toHaveLength(1);
    expect(result.next_cursor).toBeNull();
  });

  it("threads fleetId, gateKind, status, cursor, limit into the query string", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ items: [], next_cursor: null }));
    await listApprovals(WORKSPACE_ID, TOKEN, {
      status: "pending",
      fleetId: FLEET_ID,
      gateKind: "cost_overrun",
      cursor: "cur_abc",
      limit: 25,
    });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain(`fleet_id=${encodeURIComponent(FLEET_ID)}`);
    expect(url).toContain("gate_kind=cost_overrun");
    expect(url).toContain("status=pending");
    expect(url).toContain("cursor=cur_abc");
    expect(url).toContain("limit=25");
  });

  it("propagates JSON evidence without re-stringifying", async () => {
    const evidence = { files: ["a", "b"], loc: 42 };
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ items: [gateFixture({ evidence })], next_cursor: null }),
    );
    const result = await listApprovals(WORKSPACE_ID, TOKEN);
    expect(result.items[0]!.evidence).toEqual(evidence);
  });

  it("returns next_cursor for paginated responses", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({
        items: Array.from({ length: 50 }, (_, i) =>
          gateFixture({ gate_id: `01999999-0000-7000-8000-${String(i).padStart(12, "0")}` }),
        ),
        next_cursor: "cur_next",
      }),
    );
    const result = await listApprovals(WORKSPACE_ID, TOKEN, { limit: 50 });
    expect(result.items).toHaveLength(50);
    expect(result.next_cursor).toBe("cur_next");
  });

  it("throws ApiError on non-2xx (404 unknown gate / cross-workspace)", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ error: "not_found" }, 404));
    await expect(listApprovals(WORKSPACE_ID, TOKEN)).rejects.toBeTruthy();
  });
});

// ── getApproval ────────────────────────────────────────────────────────

describe("getApproval", () => {
  it("hits the single-resource path with bearer", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(gateFixture()));
    const gate = await getApproval(WORKSPACE_ID, GATE_ID, TOKEN);
    expect(fetchMock).toHaveBeenCalledWith(
      `${BACKEND_BASE}${PATH_PREFIX}/${GATE_ID}`,
      expect.objectContaining({ method: "GET" }),
    );
    expect(gate.gate_id).toBe(GATE_ID);
  });

  it("throws on 404 unknown gate id", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ error_code: "UZ-APPROVAL-002", detail: "not found" }, 404),
    );
    await expect(getApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toBeTruthy();
  });
});

// On the server (no `window`) the resolve fetch targets the absolute API
// base instead of the `/backend` proxy. afterEach's unstubAllGlobals
// restores `window`; the env var is restored explicitly.
describe("resolve base URL — server-side (window undefined)", () => {
  function resolved() {
    return jsonResponse({
      gate_id: GATE_ID,
      action_id: "a",
      outcome: "approved",
      resolved_at: 1,
      resolved_by: "user:x",
    });
  }

  it("uses NEXT_PUBLIC_API_URL when set", async () => {
    vi.stubGlobal("window", undefined);
    const prev = process.env.NEXT_PUBLIC_API_URL;
    process.env.NEXT_PUBLIC_API_URL = "https://api-test.agentsfleet.net";
    try {
      fetchMock.mockResolvedValueOnce(resolved());
      await approveApproval(WORKSPACE_ID, GATE_ID, TOKEN);
      const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
      expect(url).toBe(`https://api-test.agentsfleet.net${PATH_PREFIX}/${GATE_ID}/approve`);
    } finally {
      if (prev === undefined) delete process.env.NEXT_PUBLIC_API_URL;
      else process.env.NEXT_PUBLIC_API_URL = prev;
    }
  });

  it("throws when NEXT_PUBLIC_API_URL is unset instead of guessing a backend", async () => {
    vi.stubGlobal("window", undefined);
    const prev = process.env.NEXT_PUBLIC_API_URL;
    delete process.env.NEXT_PUBLIC_API_URL;
    try {
      await expect(denyApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toThrow(
        /NEXT_PUBLIC_API_URL is unset/,
      );
      expect(fetchMock).not.toHaveBeenCalled();
    } finally {
      if (prev !== undefined) process.env.NEXT_PUBLIC_API_URL = prev;
    }
  });
});
