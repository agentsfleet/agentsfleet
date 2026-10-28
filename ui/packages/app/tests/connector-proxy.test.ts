import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NextRequest } from "next/server";
import { CONNECTOR_COMPLETION_PATH, CONNECTOR_RETURN_COOKIE } from "@/lib/connectors/completion";
import { SIGN_IN_PATH } from "@/lib/auth/sign-in-redirect";

const { middleware, protect, authMock, fetchMock, cookieDelete } = vi.hoisted(() => ({
  middleware: vi.fn(), protect: vi.fn(), authMock: vi.fn(), fetchMock: vi.fn(), cookieDelete: vi.fn(),
}));
vi.mock("@clerk/nextjs/server", async importOriginal => ({
  ...await importOriginal<typeof import("@clerk/nextjs/server")>(),
  auth: authMock,
  clerkMiddleware: (handler: (...args: unknown[]) => unknown) => {
    middleware.mockImplementation(handler);
    return handler;
  },
}));
vi.mock("next/headers", () => ({ cookies: async () => ({ get: () => undefined, delete: cookieDelete }) }));

const APP_ORIGIN = "https://app-dev.agentsfleet.net";
const CALLBACK_QUERY = "?code=provider-code&state=signed-state";
const PRIVATE_DESTINATION = "/w/ws_1/integrations?from=connect";

async function visit(path: string) {
  await import("../proxy");
  const request = new NextRequest(new URL(path, APP_ORIGIN));
  await middleware(Object.assign(vi.fn(), { protect }), request);
  return request;
}

beforeEach(() => {
  protect.mockResolvedValue(undefined);
  authMock.mockResolvedValue({ getToken: async () => null });
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => { vi.clearAllMocks(); vi.unstubAllGlobals(); });

describe("connector recovery through the dashboard proxy", () => {
  // Pin the provider grammar, including its upper bound and separators.
  it.each(["github", "slack", "jira", "linear", "zoho", "a".repeat(64), "custom_provider-1"])("lets an expired %s session reach recovery without exchanging a code", async provider => {
    const request = await visit(`/api/connectors/${provider}/callback${CALLBACK_QUERY}`);
    expect(protect).not.toHaveBeenCalled();
    const { GET } = await import("../app/api/connectors/[provider]/callback/route");
    const response = await GET(request, { params: Promise.resolve({ provider }) });
    expect(response.status).toBe(302);
    const location = new URL(response.headers.get("location")!);
    expect(location.pathname).toBe(CONNECTOR_COMPLETION_PATH);
    expect(location.searchParams.get("reason")).toBe("session");
    expect(location.href).not.toMatch(/provider-code|signed-state/);
    await visit(location.pathname + location.search);
    expect(protect).not.toHaveBeenCalled();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(cookieDelete).toHaveBeenCalledWith(`${CONNECTOR_RETURN_COOKIE}${provider}`);
  });

  // Pin the route boundaries: neighboring paths must never inherit public access.
  it.each([
    "/", PRIVATE_DESTINATION, "/api/connectors/jira/connect", "/api/connectors/jira/callback/extra",
    "/api/connectors//callback", "/api/connectors/Jira/callback", "/api/connectors/-jira/callback",
    `/api/connectors/${"a".repeat(65)}/callback`, "/api/connectors/jira/other/callback",
    "/connectors/complete/extra", "/connectors/complete-other", "/api/live", "/settings/billing",
  ])("protects private or non-callback path %s", async path => {
    const request = await visit(path);
    expect(protect).toHaveBeenCalledOnce();
    const location = new URL(protect.mock.calls[0]![0].unauthenticatedUrl);
    expect(location.origin).toBe(APP_ORIGIN);
    expect(location.pathname).toBe(SIGN_IN_PATH);
    expect(location.searchParams.get("redirect_url")).toBe(request.nextUrl.pathname + request.nextUrl.search);
    expect(authMock).not.toHaveBeenCalled();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it.each([SIGN_IN_PATH, "/sign-in/factor-one", "/sign-up"])("keeps sign-in entry %s public", async path => {
    await visit(path);
    expect(protect).not.toHaveBeenCalled();
  });
});
