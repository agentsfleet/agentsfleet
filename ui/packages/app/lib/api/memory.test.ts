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
      json: async () => ({ items: [entry], total: 1, next_cursor: null }),
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

  it("sends starting_after when continuing a walk", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      headers: headers(),
      json: async () => ({ items: [], total: 0, next_cursor: null }),
    });
    const { listMemories } = await import("./memory");
    await listMemories("ws_1", "z_1", "tok", { starting_after: "1700:alpha" });
    const url = fetchMock.mock.calls[0]![0] as string;
    expect(url).toContain("starting_after=1700%3Aalpha");
  });
});

describe("listAllMemories", () => {
  it("test_memory_panel_walks_every_entry", async () => {
    // Two-read fixture: the first page carries a continuation, the second
    // ends the walk — the panel's data path renders both pages' entries,
    // never just the first bounded read.
    const alpha = { key: "alpha", content: "a", category: "core", updated_at: 2 };
    const beta = { key: "beta", content: "b", category: "core", updated_at: 1 };
    fetchMock
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: headers(),
        json: async () => ({ items: [alpha], total: 1, next_cursor: "1700:alpha" }),
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        headers: headers(),
        json: async () => ({ items: [beta], total: 1, next_cursor: null }),
      });
    const { listAllMemories } = await import("./memory");
    const res = await listAllMemories("ws_1", "z_1", "tok");
    expect(res.items.map((e) => e.key)).toEqual(["alpha", "beta"]);
    const secondUrl = fetchMock.mock.calls[1]![0] as string;
    expect(secondUrl).toContain("starting_after=1700%3Aalpha");
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
