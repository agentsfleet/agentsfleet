import { afterEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { createTenantWorkspace, listTenantWorkspaces } from "@/lib/api/workspaces";

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

  it("pins the create operation the daemon actually serves", () => {
    // The bundle is generated from the daemon's own handlers now, so this
    // pins BEHAVIOUR rather than a hand-written claim about it. The claim it
    // used to pin — `name` required — was never true: `create` reads an empty
    // body as `{}` and a `None` name means "name it for me", so a required
    // field here would have documented a refusal the daemon does not make.
    const bundlePath = resolve(process.cwd(), "../../../public/openapi.json");
    const document = JSON.parse(readFileSync(bundlePath, "utf8")) as {
      paths: Record<string, { post?: Record<string, unknown> }>;
    };
    const operation = document.paths["/v1/workspaces"]?.post as {
      parameters?: Array<{ name?: string }>;
      operationId?: string;
      requestBody?: {
        required?: boolean;
        content?: {
          "application/json"?: {
            schema?: { $ref?: string };
          };
        };
      };
      responses?: Record<string, unknown>;
    };

    expect(operation.operationId).toBe("create_workspace");
    expect(operation.requestBody?.content?.["application/json"]?.schema).toBeDefined();
    // Optional, because the daemon names the workspace when the caller does not.
    expect(operation.requestBody?.required).not.toBe(true);

    const removedReplayHeader = ["Idempotency", "Key"].join("-");
    expect(operation.parameters ?? []).not.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: removedReplayHeader }),
      ]),
    );
    // The refusals the route table guarantees a caller can meet.
    expect(Object.keys(operation.responses ?? {})).toEqual(
      expect.arrayContaining(["401", "403", "409", "500"]),
    );
  });
});

describe("listTenantWorkspaces", () => {
  it("walks every cursor page and returns one complete oldest-first list", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            items: [{ id: "ws_1", name: "one", created_at: 1 }],
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
            items: [{ id: "ws_2", name: "two", created_at: 2 }],
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
        { id: "ws_1", name: "one", created_at: 1 },
        { id: "ws_2", name: "two", created_at: 2 },
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

  it("pins exact-name filtering and cursor pagination in the bundled OpenAPI", () => {
    const bundlePath = resolve(process.cwd(), "../../../public/openapi.json");
    const document = JSON.parse(readFileSync(bundlePath, "utf8")) as {
      paths: Record<string, { get?: Record<string, unknown> }>;
    };
    const operation = document.paths["/v1/tenants/me/workspaces"]?.get as {
      parameters?: Array<{ name: string; schema?: Record<string, unknown> }>;
      responses?: Record<
        string,
        {
          content?: {
            "application/json"?: { schema?: { $ref?: string } };
          };
        }
      >;
    };
    const parameters = operation.parameters ?? [];
    expect(parameters.map(({ name }) => name)).toEqual([
      "name",
      "starting_after",
      "limit",
    ]);
    // The 200 names the response shape the daemon serializes. Per-field bounds
    // — the `limit` ceiling, `items` maxItems, `x-stability` — are not carried
    // yet: the port renamed the schemas, so they could not come across
    // mechanically and are named as follow-up scope in the spec's Discovery.
    const schema =
      operation.responses?.["200"]?.content?.["application/json"]?.schema;
    expect(schema?.$ref).toContain("WorkspacesResponse");
  });
});
