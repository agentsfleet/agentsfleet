import { afterEach, describe, expect, it, vi } from "vitest";

const { authMock, redirectMock } = vi.hoisted(() => ({
  authMock: vi.fn(),
  redirectMock: vi.fn(() => {
    // Next's redirect never returns: it throws a control-flow signal the
    // framework catches. A stub that returned would let `requireCredential`
    // fall through and hand a caller `null` typed as `string`.
    throw new Error("NEXT_REDIRECT");
  }),
}));
vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));
vi.mock("next/navigation", () => ({ redirect: redirectMock }));

// Deliberately NOT JWT-shaped. Nothing here parses the token — it is an
// opaque string the boundary hands back — and a realistic `eyJ…` fixture
// trips the secret scanner on entropy, which is the scanner working.
const TOKEN = "a-bearer-for-this-test";
const SIGN_IN = "/sign-in";

afterEach(() => {
  authMock.mockReset();
  redirectMock.mockClear();
  // `cache()` memoizes per module instance, so each test needs its own.
  vi.resetModules();
});

/** The module, freshly imported so `cache()` starts empty. */
async function load() {
  return import("@/lib/auth/credential");
}

describe("credential — the dashboard's only bearer", () => {
  it("hands back the token the provider resolved", async () => {
    authMock.mockResolvedValue({ getToken: async () => TOKEN });
    const { credential } = await load();
    expect(await credential()).toBe(TOKEN);
  });

  // Anonymous is not an error here. A route handler composing its own 401 body
  // and a layout rendering a signed-out shell both need to see the null.
  it("answers null for an anonymous visitor", async () => {
    authMock.mockResolvedValue({ getToken: async () => null });
    const { credential } = await load();
    expect(await credential()).toBeNull();
  });

  // Rendering one approvals page resolves a bearer four times: the dashboard
  // layout, the workspace layout, and the page twice. `cache()` collapses those
  // to one — but only inside a React request scope, which a unit test does not
  // have, so the CALL COUNT is deliberately not asserted here. What is asserted
  // is the contract every one of those callers depends on: same request, same
  // answer, no caller racing another into a different token.
  it("answers every caller in a request with the same token", async () => {
    authMock.mockResolvedValue({ getToken: async () => TOKEN });
    const { credential } = await load();
    const answers = await Promise.all([credential(), credential(), credential()]);
    expect(answers).toEqual([TOKEN, TOKEN, TOKEN]);
  });
});

describe("requireCredential — the guard twenty-four pages wrote by hand", () => {
  it("returns the token when there is one", async () => {
    authMock.mockResolvedValue({ getToken: async () => TOKEN });
    const { requireCredential } = await load();
    expect(await requireCredential()).toBe(TOKEN);
    expect(redirectMock).not.toHaveBeenCalled();
  });

  it("redirects to sign-in when there is not", async () => {
    authMock.mockResolvedValue({ getToken: async () => null });
    const { requireCredential } = await load();
    await expect(requireCredential()).rejects.toThrow("NEXT_REDIRECT");
    expect(redirectMock).toHaveBeenCalledWith(SIGN_IN);
  });
});

describe("claims — the one thing read beyond a bearer", () => {
  it("returns the claim set the session carries", async () => {
    authMock.mockResolvedValue({ sessionClaims: { scopes: "fleet:read" } });
    const { claims } = await load();
    expect(await claims()).toEqual({ scopes: "fleet:read" });
  });

  // Fail closed: every caller treats an absent claim as "not permitted", so an
  // anonymous session must read as no claims rather than as an exception.
  it("answers null when the session carries none", async () => {
    authMock.mockResolvedValue({ sessionClaims: null });
    const { claims } = await load();
    expect(await claims()).toBeNull();
  });

  // A provider outage must not take the page with it. The scope reader above
  // turns null into an empty set, which hides operator surfaces rather than
  // showing them.
  it("answers null rather than throwing when the provider is unavailable", async () => {
    authMock.mockRejectedValue(new Error("clerk is down"));
    const { claims } = await load();
    expect(await claims()).toBeNull();
  });
});
