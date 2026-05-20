import { afterEach, describe, expect, it, vi } from "vitest";
import { createTenantWorkspace } from "@/lib/api/workspaces";

afterEach(() => {
  vi.restoreAllMocks();
});

function mockFetchOnce(status: number, body: unknown) {
  return vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(JSON.stringify(body), {
      status,
      headers: { "Content-Type": "application/json" },
    }),
  );
}

describe("createTenantWorkspace", () => {
  it("POSTs the name body to /v1/workspaces with a bearer token", async () => {
    const fetchSpy = mockFetchOnce(201, {
      workspace_id: "ws_x",
      name: "acme-prod",
      request_id: "req_1",
    });

    const res = await createTenantWorkspace("tok_1", { name: "acme-prod" });

    expect(res.workspace_id).toBe("ws_x");
    const [url, init] = fetchSpy.mock.calls[0]!;
    expect(init).toBeDefined();
    const reqInit = init as RequestInit;
    const headers = reqInit.headers as Record<string, string>;
    // The client builds a string URL + JSON string body, so these casts are
    // exact, not lossy.
    expect(url as string).toContain("/v1/workspaces");
    expect(reqInit.method).toBe("POST");
    expect(reqInit.body as string).toContain("acme-prod");
    expect(headers.Authorization).toBe("Bearer tok_1");
  });

  it("sends an empty body object when no name is given", async () => {
    const fetchSpy = mockFetchOnce(201, { workspace_id: "ws_y", name: "auto-name" });

    await createTenantWorkspace("tok_1");

    const init = fetchSpy.mock.calls[0]![1];
    expect(init?.body).toBe("{}");
  });
});
