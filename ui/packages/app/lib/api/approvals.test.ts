import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  approveApproval,
  denyApproval,
  getApproval,
  listApprovals,
  type ApprovalGate,
  type AlreadyResolvedResponse,
  type ResolveResponse,
} from "./approvals";

// Constants — RULE UFS. URL fragments + tokens reused across multiple tests.
const WORKSPACE_ID = "ws_test_001";
const TOKEN = "token_abc";
const GATE_ID = "01999999-0000-7000-8000-000000000001";
const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701";
const PATH_PREFIX = `/v1/workspaces/${WORKSPACE_ID}/approvals`;
const BACKEND_BASE = "/backend";

const ERR_ALREADY_RESOLVED = "UZ-APPROVAL-006" as const;

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
    zombie_id: ZOMBIE_ID,
    zombie_name: "approvals-a",
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
    requested_at: 1_700_000_000_000,
    timeout_at: 1_700_086_400_000,
    updated_at: null,
    resolved_by: "",
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

  it("threads zombieId, gateKind, status, cursor, limit into the query string", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ items: [], next_cursor: null }));
    await listApprovals(WORKSPACE_ID, TOKEN, {
      status: "pending",
      zombieId: ZOMBIE_ID,
      gateKind: "cost_overrun",
      cursor: "cur_abc",
      limit: 25,
    });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain(`zombie_id=${encodeURIComponent(ZOMBIE_ID)}`);
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

// ── approveApproval / denyApproval — tagged union over 200 vs 409 ─────

describe("approveApproval", () => {
  it("returns kind=resolved on 200", async () => {
    const body: ResolveResponse = {
      gate_id: GATE_ID,
      action_id: "act_001",
      outcome: "approved",
      resolved_at: 1_700_000_001_000,
      resolved_by: "user:user_abc",
    };
    fetchMock.mockResolvedValueOnce(jsonResponse(body));
    const result = await approveApproval(WORKSPACE_ID, GATE_ID, TOKEN);
    expect(result.kind).toBe("resolved");
    if (result.kind === "resolved") {
      expect(result.data.outcome).toBe("approved");
      expect(result.data.resolved_by).toBe("user:user_abc");
    }
  });

  it("posts to :approve with bearer + JSON body and reason when provided", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({
        gate_id: GATE_ID,
        action_id: "a",
        outcome: "approved",
        resolved_at: 1,
        resolved_by: "user:x",
      }),
    );
    await approveApproval(WORKSPACE_ID, GATE_ID, TOKEN, "looks good");
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${BACKEND_BASE}${PATH_PREFIX}/${GATE_ID}:approve`);
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toEqual({ reason: "looks good" });
  });

  it("posts an empty object when reason is omitted", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({
        gate_id: GATE_ID,
        action_id: "a",
        outcome: "approved",
        resolved_at: 1,
        resolved_by: "user:x",
      }),
    );
    await approveApproval(WORKSPACE_ID, GATE_ID, TOKEN);
    const init = fetchMock.mock.calls[0]![1] as RequestInit;
    expect(JSON.parse(init.body as string)).toEqual({});
  });

  it("returns kind=already_resolved on 409 carrying the original outcome", async () => {
    const body: AlreadyResolvedResponse = {
      gate_id: GATE_ID,
      action_id: "act_001",
      outcome: "approved",
      resolved_at: 1_700_000_001_000,
      resolved_by: "slack:webhook",
      error_code: ERR_ALREADY_RESOLVED,
      detail: "already resolved by slack",
    };
    fetchMock.mockResolvedValueOnce(jsonResponse(body, 409));
    const result = await approveApproval(WORKSPACE_ID, GATE_ID, TOKEN);
    expect(result.kind).toBe("already_resolved");
    if (result.kind === "already_resolved") {
      expect(result.data.error_code).toBe(ERR_ALREADY_RESOLVED);
      expect(result.data.resolved_by).toBe("slack:webhook");
      expect(result.data.outcome).toBe("approved");
    }
  });

  it("throws on neither 200 nor 409 (network error / 500)", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ detail: "internal db error" }, 500),
    );
    await expect(approveApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toBeTruthy();
  });

  it("throws when fetch itself rejects (network failure)", async () => {
    fetchMock.mockRejectedValueOnce(new Error("ECONNRESET"));
    await expect(approveApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toThrow(/ECONNRESET/);
  });
});

describe("denyApproval", () => {
  it("posts to :deny and returns resolved on 200", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({
        gate_id: GATE_ID,
        action_id: "a",
        outcome: "denied",
        resolved_at: 1,
        resolved_by: "user:x",
      }),
    );
    const result = await denyApproval(WORKSPACE_ID, GATE_ID, TOKEN, "blocking");
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toBe(`${BACKEND_BASE}${PATH_PREFIX}/${GATE_ID}:deny`);
    expect(result.kind).toBe("resolved");
    if (result.kind === "resolved") {
      expect(result.data.outcome).toBe("denied");
    }
  });

  it("returns already_resolved when prior outcome is denied", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse(
        {
          gate_id: GATE_ID,
          action_id: "a",
          outcome: "denied",
          resolved_at: 1,
          resolved_by: "slack:interaction",
          error_code: ERR_ALREADY_RESOLVED,
          detail: "x",
        },
        409,
      ),
    );
    const result = await denyApproval(WORKSPACE_ID, GATE_ID, TOKEN);
    expect(result.kind).toBe("already_resolved");
    if (result.kind === "already_resolved") {
      expect(result.data.outcome).toBe("denied");
    }
  });
});
