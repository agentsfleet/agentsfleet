import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CONNECTOR_CALLBACK_TIMEOUT_MS } from "@/lib/connectors/completion";

const { authMock, fetchMock, requireApiOriginMock, cookieGet, cookieDelete } = vi.hoisted(() => ({
  authMock: vi.fn(), fetchMock: vi.fn(), requireApiOriginMock: vi.fn(),
  cookieGet: vi.fn(), cookieDelete: vi.fn(),
}));
vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));
vi.mock("next/headers", () => ({ cookies: async () => ({ get: cookieGet, delete: cookieDelete }) }));
vi.mock("@/lib/api/client", () => ({ requireApiOrigin: requireApiOriginMock }));

const APP_ORIGIN = "https://app-dev.agentsfleet.net";
const API_ORIGIN = "https://api-dev.agentsfleet.net";
const TOKEN = "callback-test-token";
const QUERY = "?code=provider-code&state=signed-state";
const REDIRECT_PATH = "/w/ws_1/integrations";

async function complete(provider = "jira", query = QUERY): Promise<Response> {
  const { GET } = await import("../app/api/connectors/[provider]/callback/route");
  return GET(new Request(`${APP_ORIGIN}/api/connectors/${provider}/callback${query}`), {
    params: Promise.resolve({ provider }),
  });
}
function landing(response: Response): URL {
  expect(response.status).toBe(302);
  const url = new URL(response.headers.get("location")!);
  expect(url.origin).toBe(APP_ORIGIN);
  return url;
}
function expectFailure(response: Response, reason: string): void {
  const url = landing(response);
  expect(url.pathname).toBe("/connectors/complete");
  expect(url.searchParams.get("reason")).toBe(reason);
  expect(url.searchParams.get("return_to")).toBe(REDIRECT_PATH);
  expect(url.href).not.toMatch(/provider-code|signed-state|callback-test-token/);
}

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
  authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(TOKEN) });
  requireApiOriginMock.mockReturnValue(API_ORIGIN);
  cookieGet.mockReturnValue({ value: REDIRECT_PATH });
});
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.resetAllMocks(); });

describe("connector callback recovery", () => {
  it.each(["github", "slack", "jira", "linear", "zoho"])("relays %s once and returns to the authorized workspace", async (provider) => {
    fetchMock.mockResolvedValueOnce(new Response(null, {
      headers: { location: `${APP_ORIGIN}${REDIRECT_PATH}?code=untrusted#fragment` }, status: 302,
    }));
    const response = await complete(provider);
    expect(landing(response).href).toBe(`${APP_ORIGIN}${REDIRECT_PATH}`);
    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, options] = fetchMock.mock.calls[0]!;
    expect(String(url)).toBe(`${API_ORIGIN}/v1/connectors/${provider}/callback${QUERY}`);
    expect(options).toMatchObject({ headers: { Authorization: `Bearer ${TOKEN}` }, method: "POST", redirect: "manual" });
    expect(options.signal).toBeInstanceOf(AbortSignal);
    expect(cookieDelete).toHaveBeenCalledWith(`connector_return_${provider}`);
  });

  it("recovers a missing browser session before forwarding provider data", async () => {
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    expectFailure(await complete(), "session");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("recovers when the sign-in service rejects the session lookup", async () => {
    authMock.mockRejectedValueOnce(new Error("private-auth-detail"));
    expectFailure(await complete(), "unavailable");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("bounds a stalled session lookup and never exchanges after a late answer", async () => {
    vi.useFakeTimers();
    const auth = Promise.withResolvers<{ getToken: () => Promise<string> }>();
    authMock.mockReturnValueOnce(auth.promise);
    const response = complete();
    await vi.advanceTimersByTimeAsync(CONNECTOR_CALLBACK_TIMEOUT_MS);
    expectFailure(await response, "timeout");
    auth.resolve({ getToken: async () => TOKEN });
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("shows a cancellation without exchanging or exposing the provider error", async () => {
    expectFailure(await complete("jira", "?error=access_denied&error_description=private-vendor-detail"), "denied");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it.each([400, 401, 500, 503])("recovers backend status %s without returning its body", async (status) => {
    fetchMock.mockResolvedValueOnce(Response.json({ access_token: "private-vendor-detail" }, { status }));
    const response = await complete();
    expectFailure(response, status === 400 ? "expired" : "unavailable");
    expect(await response.text()).not.toContain("private-vendor-detail");
    expect(cookieDelete).toHaveBeenCalledWith("connector_return_jira");
  });

  it.each(["https://evil.example/w/ws_1/integrations", "http://[", "/settings/billing", "/w/ws_1/integrations/evil", null])("recovers an unsafe or missing success location: %s", async (location) => {
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 302, headers: location ? { location } : {} }));
    expectFailure(await complete(), "unavailable");
  });

  it("bounds a stalled exchange, aborts it and starts no retry", async () => {
    vi.useFakeTimers();
    fetchMock.mockImplementationOnce((_url: URL, options: RequestInit) => new Promise((_resolve, reject) => {
      options.signal!.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")));
    }));
    const response = complete();
    await vi.advanceTimersByTimeAsync(CONNECTOR_CALLBACK_TIMEOUT_MS);
    expectFailure(await response, "timeout");
    expect(fetchMock).toHaveBeenCalledOnce();
  });

  it("recovers a dropped backend connection", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("connection dropped"));
    expectFailure(await complete(), "unavailable");
  });

  it("discards an untrusted return cookie", async () => {
    cookieGet.mockReturnValue({ value: "//evil.example" });
    fetchMock.mockRejectedValueOnce(new Error("unavailable"));
    expect(landing(await complete()).searchParams.has("return_to")).toBe(false);
  });

  it.each(["github/../evil", "Datadog", "a".repeat(65)])("refuses provider path %s before session work", async provider => {
    const { GET } = await import("../app/api/connectors/[provider]/callback/route");
    const response = await GET(new Request(APP_ORIGIN), { params: Promise.resolve({ provider }) });
    expect(response.status).toBe(400);
    expect(authMock).not.toHaveBeenCalled();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
