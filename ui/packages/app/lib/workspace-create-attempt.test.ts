import { afterEach, describe, expect, it, vi } from "vitest";

import {
  acquireWorkspaceCreateAttempt,
  clearWorkspaceCreateAttempt,
} from "./workspace-create-attempt";

afterEach(() => {
  window.sessionStorage.clear();
  vi.restoreAllMocks();
});

describe("workspace create attempts", () => {
  it("reuses the same UUIDv7 for the same request after module state is lost", () => {
    vi.spyOn(crypto, "randomUUID").mockReturnValue(
      "12345678-1234-4234-9234-123456789abc",
    );
    const first = acquireWorkspaceCreateAttempt(" acme ", null);
    const recovered = acquireWorkspaceCreateAttempt("acme", null);

    expect(first.idempotencyKey).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
    expect(recovered).toEqual(first);
  });

  it("mints a new key when the request body changes", () => {
    vi.spyOn(crypto, "randomUUID")
      .mockReturnValueOnce("12345678-1234-4234-9234-123456789abc")
      .mockReturnValueOnce("abcdefab-cdef-4def-8def-abcdefabcdef");

    const first = acquireWorkspaceCreateAttempt("acme", null);
    const changed = acquireWorkspaceCreateAttempt("other", first);

    expect(changed.idempotencyKey).not.toBe(first.idempotencyKey);
    expect(changed.name).toBe("other");
  });

  it("clears only the attempt that completed", () => {
    const attempt = acquireWorkspaceCreateAttempt(undefined, null);
    const differentKey = attempt.idempotencyKey.replace(/.$/, (last) =>
      last === "0" ? "1" : "0",
    );
    clearWorkspaceCreateAttempt({
      idempotencyKey: differentKey,
      name: undefined,
    });
    expect(acquireWorkspaceCreateAttempt(undefined, null)).toEqual(attempt);

    clearWorkspaceCreateAttempt(attempt);
    expect(
      acquireWorkspaceCreateAttempt(undefined, null).idempotencyKey,
    ).not.toBe(attempt.idempotencyKey);
  });

  it("falls back to memory when browser storage is unavailable", () => {
    const getItem = vi
      .spyOn(window.sessionStorage, "getItem")
      .mockImplementation(() => {
        throw new Error("storage denied");
      });
    const first = acquireWorkspaceCreateAttempt("offline", null);
    getItem.mockRestore();
    expect(acquireWorkspaceCreateAttempt(" offline ", first)).toEqual(first);

    const setItem = vi
      .spyOn(window.sessionStorage, "setItem")
      .mockImplementation(() => {
        throw new Error("storage denied");
      });
    const memoryOnly = acquireWorkspaceCreateAttempt(" memory-only ", null);
    setItem.mockRestore();

    expect(first.name).toBe("offline");
    expect(memoryOnly.name).toBe("memory-only");
  });

  it("tolerates storage removal failure after a successful create", () => {
    const attempt = acquireWorkspaceCreateAttempt("kept", null);
    vi.spyOn(window.sessionStorage, "removeItem").mockImplementation(() => {
      throw new Error("storage denied");
    });

    expect(() => clearWorkspaceCreateAttempt(attempt)).not.toThrow();
  });

  it("replaces malformed stored keys and normalizes blank names", () => {
    window.sessionStorage.setItem(
      "agentsfleet.workspace-create.idempotency-key",
      "malformed",
    );
    const attempt = acquireWorkspaceCreateAttempt("   ", null);

    expect(attempt.name).toBeUndefined();
    expect(attempt.idempotencyKey).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  });
});
