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
/** A second answer, so a cache that outlives a request is visible. */
const OTHER_TOKEN = "a-different-bearer";
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

  // The invariant the module calls load-bearing: `cache()` is scoped to ONE
  // request and torn down with it, so a module-level cache would survive
  // between requests and hand one visitor another visitor's bearer.
  //
  // Asserting "three calls agree" cannot catch that — a module-scope cache
  // agrees too. This asks the opposite question: within ONE module instance,
  // does a second resolution see the NEW answer? Outside a React request scope
  // `cache()` is a pass-through, so it must. A module-level `let token` would
  // return the first token here and fail.
  it("does not hold a bearer across resolutions in module scope", async () => {
    const getToken = vi
      .fn<() => Promise<string>>()
      .mockResolvedValueOnce(TOKEN)
      .mockResolvedValueOnce(OTHER_TOKEN);
    authMock.mockResolvedValue({ getToken });
    const { credential } = await load();
    expect(await credential()).toBe(TOKEN);
    expect(await credential()).toBe(OTHER_TOKEN);
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
