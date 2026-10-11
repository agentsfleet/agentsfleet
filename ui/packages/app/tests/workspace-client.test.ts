import { afterEach, describe, expect, it, vi } from "vitest";
import { createTenantWorkspace, listTenantWorkspaces } from "@/lib/api/workspaces";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";

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
      tenant_id: "tenant_x",
    });

    const res = await createTenantWorkspace("tok_1", {
      name: "acme-prod",
    });

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
    // Exact set, deliberately — this assertion exists to catch a header
    // leaking onto the request, so it must list every header the client sends.
    // `traceparent` is one of them: every request mints a fresh W3C root so a
    // slow page can be attributed to the server-side stages that produced it.
    expect(Object.keys(headers).sort()).toEqual([
      "Authorization",
      "Content-Type",
      "traceparent",
    ]);
    // The value has to be a shape the server will actually parse; one it
    // rejects is silently ignored and costs the correlation this header exists
    // for, without failing anything.
    expect(headers.traceparent).toMatch(/^00-[0-9a-f]{32}-[0-9a-f]{16}-01$/);
    expect(reqInit.signal).toBeInstanceOf(AbortSignal);
  });

  it.each([
    null,
    {},
    {
      workspace_id: "ws_x",
      name: "different",
      request_id: "req_1",
      tenant_id: "tenant_x",
    },
    {
      workspace_id: "ws_x",
      name: "acme-prod",
      tenant_id: "tenant_x",
    },
  ])("rejects a malformed create response %#", async (body) => {
    mockFetchOnce(201, body);

    await expect(
      createTenantWorkspace("tok_1", { name: "acme-prod" }),
    ).rejects.toThrow("workspace create response is invalid");
  });

  it("accepts a server-generated name when the user leaves the name blank", async () => {
    mockFetchOnce(201, { workspace_id: "ws_x", name: "silent-raven-k7m2", tenant_id: "tenant_x", request_id: "req_1" });
    await expect(createTenantWorkspace("tok_1", { name: "" })).resolves.toMatchObject({ name: "silent-raven-k7m2" });
  });

  it.each(["", "   ", null, 12])("rejects an invalid generated name %j", async (name) => {
    mockFetchOnce(201, { workspace_id: "ws_x", name, tenant_id: "tenant_x", request_id: "req_1" });
    await expect(createTenantWorkspace("tok_1", { name: "" })).rejects.toThrow("workspace create response is invalid");
  });
});

describe("listTenantWorkspaces", () => {
  // Each item names its account and the caller's role in it: the first page is
  // the caller's own account, the second one they joined as a member.
  const OWN = { account: { tenant_id: "tenant_x", owner_name: "Xan" }, role: ACCOUNT_ROLE.owner };
  const JOINED = { account: { tenant_id: "tenant_y", owner_name: "John" }, role: ACCOUNT_ROLE.member };

  it("walks every cursor page and returns one complete oldest-first list", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            items: [{ id: "ws_1", name: "one", created_at: 1, ...OWN }],
            tenant_id: "tenant_x",
            total: null,
            next_cursor: "1:ws_1",
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            items: [{ id: "ws_2", name: "two", created_at: 2, ...JOINED }],
            tenant_id: "tenant_x",
            total: null,
            next_cursor: null,
          }),
          { status: 200 },
        ),
      );

    const result = await listTenantWorkspaces("tok_1");

    expect(result).toEqual({
      items: [
        { id: "ws_1", name: "one", created_at: 1, ...OWN },
        { id: "ws_2", name: "two", created_at: 2, ...JOINED },
      ],
      tenant_id: "tenant_x",
      total: 2,
      next_cursor: null,
    });
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(fetchSpy.mock.calls[0]?.[0] as string).toContain(
      "/v1/tenants/me/workspaces?limit=100",
    );
    expect(fetchSpy.mock.calls[1]?.[0] as string).toContain(
      "limit=100&starting_after=1%3Aws_1",
    );
  });

  it("rejects a repeated cursor instead of looping forever", async () => {
    vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            items: [],
            tenant_id: "tenant_x",
            total: null,
            next_cursor: "repeat",
          }),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            items: [],
            tenant_id: "tenant_x",
            total: null,
            next_cursor: "repeat",
          }),
          { status: 200 },
        ),
      );

    await expect(listTenantWorkspaces("tok_1")).rejects.toThrow(
      "repeated a cursor",
    );
  });
});

describe("listTenantWorkspaces account and role", () => {
  // A workspace the list cannot place under an account would land in the wrong
  // switcher group, so an item that does not say whose it is fails the read.
  const OWNED = { id: "ws_1", name: "one", created_at: 1 };
  const ACCOUNT = { tenant_id: "tenant_x", owner_name: "Xan" };

  function pageOf(item: unknown) {
    return mockFetchOnce(200, { items: [item], tenant_id: "tenant_x", total: null, next_cursor: null });
  }

  it("should reject an item whose role this dashboard does not know", async () => {
    pageOf({ ...OWNED, account: ACCOUNT, role: "viewer" });
    await expect(listTenantWorkspaces("tok_1")).rejects.toThrow("workspace item is invalid");
  });

  it("should reject an item that names no account", async () => {
    pageOf({ ...OWNED, role: ACCOUNT_ROLE.member });
    await expect(listTenantWorkspaces("tok_1")).rejects.toThrow("workspace account is invalid");
  });

  it("should reject an account with a blank owner name", async () => {
    pageOf({ ...OWNED, account: { ...ACCOUNT, owner_name: "" }, role: ACCOUNT_ROLE.member });
    await expect(listTenantWorkspaces("tok_1")).rejects.toThrow("workspace account is invalid");
  });
});
