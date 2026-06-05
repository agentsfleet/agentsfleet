import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// vi.mock is hoisted above the static `./platform` import, so the mock fn must
// be created via vi.hoisted() to exist when the factory runs (see runners.test.ts).
const { authMock } = vi.hoisted(() => ({ authMock: vi.fn() }));
vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));

import { readPlatformAdminClaim } from "./platform";

beforeEach(() => vi.clearAllMocks());
afterEach(() => vi.resetAllMocks());

describe("readPlatformAdminClaim", () => {
  it("is true only when the session metadata carries platform_admin === true", async () => {
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: { platform_admin: true } } });
    await expect(readPlatformAdminClaim()).resolves.toBe(true);
  });

  it("is false when the claim is present but not exactly the boolean true", async () => {
    // Guard against a truthy-but-wrong value (e.g. the string "true") slipping
    // a non-admin through — the check is strict `=== true`.
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: { platform_admin: "true" } } });
    await expect(readPlatformAdminClaim()).resolves.toBe(false);
  });

  it("is false when the claim is explicitly false", async () => {
    authMock.mockResolvedValueOnce({ sessionClaims: { metadata: { platform_admin: false } } });
    await expect(readPlatformAdminClaim()).resolves.toBe(false);
  });

  it("is false (fail-closed) when the metadata bag is absent", async () => {
    authMock.mockResolvedValueOnce({ sessionClaims: {} });
    await expect(readPlatformAdminClaim()).resolves.toBe(false);
  });

  it("is false (fail-closed) for an anonymous session with no claims", async () => {
    authMock.mockResolvedValueOnce({ sessionClaims: null });
    await expect(readPlatformAdminClaim()).resolves.toBe(false);
  });

  it("is false (fail-closed) when the auth provider throws", async () => {
    authMock.mockRejectedValueOnce(new Error("clerk unavailable"));
    await expect(readPlatformAdminClaim()).resolves.toBe(false);
  });
});
