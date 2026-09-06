import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  approveApproval,
  denyApproval,
  type AlreadyResolvedResponse,
  type ResolveResponse,
} from "./approvals";
import { DEFAULT_REQUEST_TIMEOUT_MS } from "./client";
import { HTTP_STATUS_REQUEST_TIMEOUT, RETRY_CODE_TIMEOUT } from "./errors";

// The resolve half of the approvals client (approve / deny: the tagged union
// over 200 vs 409, and the transport timeout). Split from approvals.test.ts by
// concern so each file stays under the length cap.

// Constants — RULE UFS. URL fragments + tokens reused across multiple tests.
const WORKSPACE_ID = "ws_test_001";
const TOKEN = "token_abc";
const GATE_ID = "01999999-0000-7000-8000-000000000001";
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
    expect(url).toBe(`${BACKEND_BASE}${PATH_PREFIX}/${GATE_ID}/approve`);
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

  it("falls back to a generic message when 500 body has no detail field", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({}, 503));
    await expect(approveApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toThrow(
      /Resolve failed: HTTP 503/,
    );
  });

  it("throws an ApiError carrying the real error_code, not a bare Error", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ error_code: "UZ-APPROVAL-004", detail: "gate service unavailable" }, 503),
    );
    const { ApiError } = await import("./errors");
    await expect(approveApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toSatisfy((err: unknown) => {
      return err instanceof ApiError && err.code === "UZ-APPROVAL-004" && err.status === 503;
    });
  });

  it("throws when fetch itself rejects (network failure)", async () => {
    fetchMock.mockRejectedValueOnce(new Error("ECONNRESET"));
    await expect(approveApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toThrow(/ECONNRESET/);
  });

  it("tolerates a non-JSON 200 body — the .json() catch yields an empty object", async () => {
    // A 200 with a body that isn't valid JSON (truncated stream, proxy
    // injecting HTML). `res.json()` rejects; resolveAction's `.catch(() => ({}))`
    // swallows it so a successful status still resolves rather than throwing.
    fetchMock.mockResolvedValueOnce(new Response("<<not json", { status: 200 }));
    const result = await approveApproval(WORKSPACE_ID, GATE_ID, TOKEN);
    expect(result.kind).toBe("resolved");
    if (result.kind === "resolved") {
      expect(result.data).toEqual({});
    }
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
    expect(url).toBe(`${BACKEND_BASE}${PATH_PREFIX}/${GATE_ID}/deny`);
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

// ── the transport's timeout ───────────────────────────────────────────

/** What `fetch` rejects with when an `AbortSignal.timeout` fires. */
function timeoutAbort(): DOMException {
  return new DOMException("signal timed out", "TimeoutError");
}

describe("resolve — bounded by the default timeout", () => {
  it("a hung resolve aborts after the default timeout instead of pending forever", async () => {
    const timeoutSpy = vi.spyOn(AbortSignal, "timeout");
    fetchMock.mockRejectedValueOnce(timeoutAbort());
    await expect(approveApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toMatchObject({
      code: RETRY_CODE_TIMEOUT,
      status: HTTP_STATUS_REQUEST_TIMEOUT,
    });
    // The bypassed transport still carries the budget every request() carries,
    // so a hung POST surfaces as ok:false and the optimistic row comes back.
    expect(timeoutSpy).toHaveBeenCalledWith(DEFAULT_REQUEST_TIMEOUT_MS);
    const init = fetchMock.mock.calls[0]![1] as RequestInit;
    expect(init.signal).toBe(timeoutSpy.mock.results[0]?.value);
    timeoutSpy.mockRestore();
  });

  it("a timeout during the body read is a timeout, never an empty resolve", async () => {
    fetchMock.mockResolvedValueOnce({ status: 200, json: () => Promise.reject(timeoutAbort()) });
    await expect(denyApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toMatchObject({
      code: RETRY_CODE_TIMEOUT,
    });
  });
});
