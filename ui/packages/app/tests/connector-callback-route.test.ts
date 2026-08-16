import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { authMock, fetchMock, requireApiOriginMock } = vi.hoisted(() => ({
  authMock: vi.fn(),
  fetchMock: vi.fn(),
  requireApiOriginMock: vi.fn(),
}));

vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));
vi.mock("@/lib/api/client", () => ({ requireApiOrigin: requireApiOriginMock }));

const APP_ORIGIN = "https://app-dev.agentsfleet.net";
const API_ORIGIN = "https://api-dev.agentsfleet.net";
const TOKEN = "callback-test-token";
const CALLBACK_PATH = "/api/connectors/github/callback?code=provider-code&state=signed-state&callback_source=legacy_api";
const COMPLETE_PATH = "/v1/connectors/github/callback?code=provider-code&state=signed-state&callback_source=legacy_api";
const REDIRECT_PATH = "/w/ws_1/integrations";

function callbackRequest(): Request {
  return new Request(`${APP_ORIGIN}${CALLBACK_PATH}`);
}

function githubParams(): { params: Promise<{ provider: string }> } {
  return { params: Promise.resolve({ provider: "github" }) };
}

describe("connector callback route", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(TOKEN) });
    requireApiOriginMock.mockReturnValue(API_ORIGIN);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("relays the signed-in browser token and provider query to backend callbacks", async () => {
    fetchMock.mockResolvedValueOnce(new Response(null, {
      headers: { location: `${APP_ORIGIN}${REDIRECT_PATH}` },
      status: 302,
    }));
    const { GET } = await import("../app/api/connectors/[provider]/callback/route");

    const response = await GET(callbackRequest(), githubParams());

    expect(response.status).toBe(302);
    expect(response.headers.get("location")).toBe(`${APP_ORIGIN}${REDIRECT_PATH}`);
    expect(fetchMock).toHaveBeenCalledWith(`${API_ORIGIN}${COMPLETE_PATH}`, {
      headers: { Authorization: `Bearer ${TOKEN}` },
      method: "POST",
      redirect: "manual",
    });
  });

  it("refuses an unauthenticated browser before forwarding provider data", async () => {
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const { GET } = await import("../app/api/connectors/[provider]/callback/route");

    const response = await GET(callbackRequest(), githubParams());

    expect(response.status).toBe(401);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("rejects a path-tampered provider before reading the session", async () => {
    const { GET } = await import("../app/api/connectors/[provider]/callback/route");

    const response = await GET(callbackRequest(), { params: Promise.resolve({ provider: "github/../evil" }) });

    expect(response.status).toBe(400);
    expect(authMock).not.toHaveBeenCalled();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("passes a backend completion error through to the browser", async () => {
    fetchMock.mockResolvedValueOnce(Response.json(
      { error: { code: "UZ-CONN-002", message: "Invalid or expired connect state" } },
      { status: 400 },
    ));
    const { GET } = await import("../app/api/connectors/[provider]/callback/route");

    const response = await GET(callbackRequest(), githubParams());

    expect(response.status).toBe(400);
    expect(response.headers.get("content-type")).toContain("application/json");
    expect(await response.json()).toEqual({
      error: { code: "UZ-CONN-002", message: "Invalid or expired connect state" },
    });
  });

  it("uses a JSON content type when a backend error omits one", async () => {
    fetchMock.mockResolvedValueOnce(new Response(new TextEncoder().encode("upstream unavailable"), { status: 503 }));
    const { GET } = await import("../app/api/connectors/[provider]/callback/route");

    const response = await GET(callbackRequest(), githubParams());

    expect(response.status).toBe(503);
    expect(response.headers.get("content-type")).toBe("application/json");
    expect(await response.text()).toBe("upstream unavailable");
  });

  it("refuses a backend redirect off the dashboard origin", async () => {
    fetchMock.mockResolvedValueOnce(new Response(null, {
      headers: { location: "https://evil.example/steal" },
      status: 302,
    }));
    const { GET } = await import("../app/api/connectors/[provider]/callback/route");

    const response = await GET(callbackRequest(), githubParams());

    expect(response.status).toBe(502);
  });
});
