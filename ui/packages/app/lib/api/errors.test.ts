import { describe, expect, it } from "vitest";
import { ApiError } from "./errors";

describe("ApiError", () => {
  it("is instanceof ApiError and Error", () => {
    const e = new ApiError("not found", 404, "UZ-001");
    expect(e).toBeInstanceOf(ApiError);
    expect(e).toBeInstanceOf(Error);
    expect(e.name).toBe("ApiError");
  });

  it("carries status, code, and message", () => {
    const e = new ApiError("conflict", 409, "UZ-ZMB-010");
    expect(e.status).toBe(409);
    expect(e.code).toBe("UZ-ZMB-010");
    expect(e.message).toBe("conflict");
  });

  it("carries optional requestId", () => {
    const e = new ApiError("err", 500, "UZ-500", "req_abc");
    expect(e.requestId).toBe("req_abc");
  });

  it("requestId is undefined when not provided", () => {
    const e = new ApiError("err", 500, "UZ-500");
    expect(e.requestId).toBeUndefined();
  });
});
