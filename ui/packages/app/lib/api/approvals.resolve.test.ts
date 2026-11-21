import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RequestCancelledError } from "./errors";
import {
  answerApproval,
  approveApproval,
  denyApproval,
  type AlreadyResolvedResponse,
  type ResolveResponse,
} from "./approvals";

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
      resolved_at: "2023-11-14T22:13:21Z",
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
        resolved_at: "1970-01-01T00:00:00.001Z",
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
        resolved_at: "1970-01-01T00:00:00.001Z",
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
      resolved_at: "2023-11-14T22:13:21Z",
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

  it("refuses a non-JSON success response instead of confirming a decision", async () => {
    fetchMock.mockResolvedValueOnce(new Response("<<not json", {status: 200}));
    await expect(approveApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toMatchObject({status: 502, message: expect.stringContaining("could not be confirmed")});
  });
});

describe("denyApproval", () => {
  it("posts to :deny and returns resolved on 200", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({
        gate_id: GATE_ID,
        action_id: "a",
        outcome: "denied",
        resolved_at: "1970-01-01T00:00:00.001Z",
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
          resolved_at: "1970-01-01T00:00:00.001Z",
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

// ── answerApproval — a question gate takes the answer as its body ─────

describe("answerApproval", () => {
  const ANSWER = "Yes — ship the 東京 build";
  const answered = (status: number, extra: Record<string, unknown> = {}) => jsonResponse({
    gate_id: GATE_ID, action_id: "ask_user", outcome: "answered",
    resolved_at: "2026-11-21T10:00:00Z", resolved_by: "user:x", ...extra,
  }, status);

  it("posts the answer to /answer and returns the answered receipt", async () => {
    fetchMock.mockResolvedValueOnce(answered(200));
    const result = await answerApproval(WORKSPACE_ID, GATE_ID, TOKEN, ANSWER);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${BACKEND_BASE}${PATH_PREFIX}/${GATE_ID}/answer`);
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toEqual({ answer: ANSWER });
    expect(result).toEqual({ kind: "resolved", data: {
      gate_id: GATE_ID, action_id: "ask_user", outcome: "answered", resolved_at: "2026-11-21T10:00:00Z", resolved_by: "user:x",
    } });
  });

  it("refuses a 200 whose receipt names another decision", async () => {
    fetchMock.mockResolvedValueOnce(answered(200, { outcome: "denied" }));
    await expect(answerApproval(WORKSPACE_ID, GATE_ID, TOKEN, ANSWER)).rejects.toMatchObject({ status: 502, code: "UZ-UNKNOWN" });
  });

  it("returns already_resolved when another answer decided the gate first", async () => {
    fetchMock.mockResolvedValueOnce(answered(409, { error_code: ERR_ALREADY_RESOLVED, detail: "already answered" }));
    const result = await answerApproval(WORKSPACE_ID, GATE_ID, TOKEN, "No");
    expect(result.kind).toBe("already_resolved");
    expect(result.data.outcome).toBe("answered");
  });

  it("throws the archived fleet's refusal as an ApiError carrying UZ-AGT-020", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ error_code: "UZ-AGT-020", detail: "fleet is archived" }, 409));
    const { ApiError } = await import("./errors");
    await expect(answerApproval(WORKSPACE_ID, GATE_ID, TOKEN, ANSWER)).rejects.toSatisfy((err: unknown) =>
      err instanceof ApiError && err.code === "UZ-AGT-020" && err.status === 409 && err.message === "fleet is archived");
  });

  it("throws the refusal of an empty answer instead of confirming it", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ error_code: "UZ-REQ-001", detail: "answer is empty" }, 400));
    await expect(answerApproval(WORKSPACE_ID, GATE_ID, TOKEN, "")).rejects.toMatchObject({ status: 400, code: "UZ-REQ-001" });
  });

  it("reports a body read cut by navigation as a cancelled request, not an empty success", async () => {
    const cut = new Response(null, { status: 200 });
    vi.spyOn(cut, "json").mockRejectedValueOnce(new DOMException("The user aborted a request.", "AbortError"));
    fetchMock.mockResolvedValueOnce(cut);
    await expect(approveApproval(WORKSPACE_ID, GATE_ID, TOKEN)).rejects.toBeInstanceOf(RequestCancelledError);
  });
});
