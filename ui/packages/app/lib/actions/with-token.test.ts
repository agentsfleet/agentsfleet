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
import { z } from "zod";
import { UNCONFIRMED_REQUEST_MESSAGE } from "@/lib/errors";
import { ApiError } from "@/lib/api/errors";

describe("withToken", () => {
  it("never serializes schema issues or private payloads into an action failure", async () => {
    getTokenMock.mockResolvedValueOnce("session");
    const result = await withToken(async () => z.object({private_response: z.string()}).parse({private_response: {secret: "private-marker"}}));
    expect(result).toEqual({ok: false, error: UNCONFIRMED_REQUEST_MESSAGE});
    expect(JSON.stringify(result)).not.toMatch(/private_response|private-marker|invalid_type/);
  });
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
      throw new ApiError("conflict", 409, "UZ-AGT-009");
    });
    expect(result).toEqual({
      ok: false,
      error: "conflict",
      status: 409,
      errorCode: "UZ-AGT-009",
    });
  });

  it("hides internal exceptions without inventing a refusal status", async () => {
    getTokenMock.mockResolvedValueOnce("tok_abc");
    const result = await withToken(async () => {
      throw new Error("unexpected boom");
    });
    expect(result).toEqual({ ok: false, error: UNCONFIRMED_REQUEST_MESSAGE });
  });

  it("hides non-Error thrown payloads", async () => {
    getTokenMock.mockResolvedValueOnce("tok_abc");
    const result = await withToken(async () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      throw "raw-string-failure";
    });
    expect(result).toEqual({ ok: false, error: UNCONFIRMED_REQUEST_MESSAGE });
  });
});
