import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.fn();
const cookieGet = vi.fn();

vi.stubGlobal("fetch", fetchMock);

vi.mock("next/headers", () => ({
  cookies: vi.fn(async () => ({ get: cookieGet })),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  fetchMock.mockReset();
  cookieGet.mockReset();
});

describe("lib/workspace resolveActiveWorkspace", () => {
  function mockListWorkspaces(ids: string[]) {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        data: ids.map((id) => ({ id })),
        has_more: false,
        next_cursor: null,
        request_id: "req",
      }),
    });
  }

  it("returns null when the tenant has no workspaces", async () => {
    mockListWorkspaces([]);
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    expect(await resolveActiveWorkspace("tok")).toBeNull();
    expect(cookieGet).not.toHaveBeenCalled();
  });

  it("prefers the cookie-named workspace when it matches one the tenant owns", async () => {
    mockListWorkspaces(["ws_1", "ws_2", "ws_3"]);
    cookieGet.mockReturnValue({ value: "ws_2" });
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_2");
    expect(cookieGet).toHaveBeenCalledWith("active_workspace_id");
  });

  it("falls back to the first workspace when the cookie is absent", async () => {
    mockListWorkspaces(["ws_1", "ws_2"]);
    cookieGet.mockReturnValue(undefined);
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_1");
  });

  it("falls back to the first workspace when the cookie names a foreign id", async () => {
    mockListWorkspaces(["ws_1", "ws_2"]);
    cookieGet.mockReturnValue({ value: "ws_not_ours" });
    const { resolveActiveWorkspace } = await import("../lib/workspace");
    const result = await resolveActiveWorkspace("tok");
    expect(result?.id).toBe("ws_1");
  });
});
