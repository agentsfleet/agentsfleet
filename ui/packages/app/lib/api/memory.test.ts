import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);
afterEach(() => fetchMock.mockReset());

function headers(map: Record<string, string> = {}): Headers {
  return { get: (k: string) => map[k.toLowerCase()] ?? null } as unknown as Headers;
}

describe("listMemories", () => {
  it("GET …/memories returns entries with content/category/updated_at", async () => {
    const entry = { key: "convention", content: "reviewers use spaces", category: "core", updated_at: 5 };
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      headers: headers(),
      json: async () => ({ items: [entry], total: 1, request_id: "req_1" }),
    });
    const { listMemories } = await import("./memory");
    const res = await listMemories("ws_1", "z_1", "tok", { limit: 50 });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/z_1/memories?limit=50"),
      expect.objectContaining({ method: "GET" }),
    );
    expect(res.items[0]?.content).toBe("reviewers use spaces");
    expect(res.items[0]?.key).toBe("convention");
  });
});

describe("forgetMemory", () => {
  it("DELETE …/memories/{key} resolves on 204 (path-encodes the key)", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204, headers: headers(), json: async () => ({}) });
    const { forgetMemory } = await import("./memory");
    await forgetMemory("ws_1", "z_1", "a/b key", "tok");
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("/v1/workspaces/ws_1/fleets/z_1/memories/a%2Fb%20key");
    expect(fetchMock).toHaveBeenCalledWith(url, expect.objectContaining({ method: "DELETE" }));
  });

  it("a missing key throws ApiError 404 (UZ-MEM-004)", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 404,
      headers: headers(),
      json: async () => ({ error_code: "UZ-MEM-004", detail: "no such memory" }),
    });
    const { forgetMemory } = await import("./memory");
    const err = (await forgetMemory("ws_1", "z_1", "gone", "tok").catch((e) => e)) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(404);
    expect(err.code).toBe("UZ-MEM-004");
  });
});
