import { describe, expect, it, vi, beforeEach } from "vitest";

const { getTokenMock } = vi.hoisted(() => ({
  getTokenMock: vi.fn(),
}));

// Post-Stage-1, withToken calls `auth().getToken()` directly — no
// templated mint, no `getServerToken` indirection. Mock the named export
// from `@clerk/nextjs/server` to feed the resolved token directly.
vi.mock("@clerk/nextjs/server", () => ({
  auth: vi.fn(async () => ({ getToken: getTokenMock })),
}));

import { withToken } from "./with-token";
import { ApiError } from "@/lib/api/errors";

describe("withToken", () => {
  beforeEach(() => {
    getTokenMock.mockReset();
  });

  it("returns 401 when no token resolves", async () => {
    getTokenMock.mockResolvedValueOnce(null);
    const result = await withToken(async () => "should-not-call");
    expect(result).toEqual({
      ok: false,
      error: "Not authenticated",
      status: 401,
      errorCode: "UZ-AUTH-401",
    });
  });

  it("returns ok:true with data on success", async () => {
    getTokenMock.mockResolvedValueOnce("tok_abc");
    const result = await withToken<string>(async (t) => `data:${t}`);
    expect(result).toEqual({ ok: true, data: "data:tok_abc" });
  });

  it("maps ApiError to ok:false with status + errorCode fields", async () => {
    getTokenMock.mockResolvedValueOnce("tok_abc");
    const result = await withToken(async () => {
      throw new ApiError("conflict", 409, "UZ-ZMB-009");
    });
    expect(result).toEqual({
      ok: false,
      error: "conflict",
      status: 409,
      errorCode: "UZ-ZMB-009",
    });
  });

  it("maps a plain Error to ok:false with message and no status", async () => {
    getTokenMock.mockResolvedValueOnce("tok_abc");
    const result = await withToken(async () => {
      throw new Error("unexpected boom");
    });
    expect(result).toEqual({ ok: false, error: "unexpected boom" });
  });

  it("maps a non-Error throw (string) to ok:false with String(e) (covers else branch)", async () => {
    getTokenMock.mockResolvedValueOnce("tok_abc");
    const result = await withToken(async () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      throw "raw-string-failure";
    });
    expect(result).toEqual({ ok: false, error: "raw-string-failure" });
  });
});
