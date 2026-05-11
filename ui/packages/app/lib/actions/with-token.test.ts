import { describe, expect, it, vi, beforeEach } from "vitest";

const { getServerTokenMock } = vi.hoisted(() => ({
  getServerTokenMock: vi.fn(),
}));

vi.mock("@/lib/auth/server", () => ({
  getServerToken: getServerTokenMock,
}));

import { withToken } from "./with-token";
import { ApiError } from "@/lib/api/errors";

describe("withToken", () => {
  beforeEach(() => {
    getServerTokenMock.mockReset();
  });

  it("returns 401 when no token resolves", async () => {
    getServerTokenMock.mockResolvedValueOnce(null);
    const result = await withToken(async () => "should-not-call");
    expect(result).toEqual({ ok: false, error: "Not authenticated", status: 401 });
  });

  it("returns ok:true with data on success", async () => {
    getServerTokenMock.mockResolvedValueOnce("tok_abc");
    const result = await withToken<string>(async (t) => `data:${t}`);
    expect(result).toEqual({ ok: true, data: "data:tok_abc" });
  });

  it("maps ApiError to ok:false with status field", async () => {
    getServerTokenMock.mockResolvedValueOnce("tok_abc");
    const result = await withToken(async () => {
      throw new ApiError("conflict", 409, "UZ-ZMB-009");
    });
    expect(result).toEqual({ ok: false, error: "conflict", status: 409 });
  });

  it("maps a plain Error to ok:false with message and no status", async () => {
    getServerTokenMock.mockResolvedValueOnce("tok_abc");
    const result = await withToken(async () => {
      throw new Error("unexpected boom");
    });
    expect(result).toEqual({ ok: false, error: "unexpected boom" });
  });

  it("maps a non-Error throw (string) to ok:false with String(e) (covers else branch)", async () => {
    getServerTokenMock.mockResolvedValueOnce("tok_abc");
    const result = await withToken(async () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      throw "raw-string-failure";
    });
    expect(result).toEqual({ ok: false, error: "raw-string-failure" });
  });
});
