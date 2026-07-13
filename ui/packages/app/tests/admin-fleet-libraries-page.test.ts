import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ApiError } from "@/lib/api/errors";
import { SCOPE, expandScopes } from "@/lib/auth/scopes";

// The platform fleet-library surface is operator-only. These tests cover the two
// halves the backend cannot vouch for on the client's behalf: the page guard
// (nobody without `platform-library:write` sees the surface) and the server
// action's error mapping. The view is stubbed so this stays a page test.

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const hasScopeMock = vi.fn();
const withTokenMock = vi.fn();
const onboardPlatformFleetLibraryMock = vi.fn();

vi.mock("next/navigation", () => ({ redirect }));
vi.mock("@/lib/auth/platform", () => ({ hasScope: hasScopeMock }));
vi.mock("@/lib/api/fleet-library", () => ({
  onboardPlatformFleetLibrary: onboardPlatformFleetLibraryMock,
}));

vi.mock("@/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView", () => ({
  default: () => React.createElement("div", { "data-fleet-libraries-view": "1" }, "Fleet libraries"),
}));

const NOT_PLATFORM_ADMIN = "/settings?notice=fleet-libraries-platform-admin-only";
const REPO = "agentsfleet/platform-ops";

beforeEach(() => {
  vi.clearAllMocks();
  hasScopeMock.mockResolvedValue(true);
});

describe("admin/fleet-libraries page", () => {
  it("redirects a caller without platform-library:write to the operator notice", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const { default: Page } = await import("../app/(dashboard)/admin/fleet-libraries/page");
    await expect(Page()).rejects.toThrow(`redirect:${NOT_PLATFORM_ADMIN}`);
  });

  it("gates on exactly the platform-library:write scope", async () => {
    const { default: Page } = await import("../app/(dashboard)/admin/fleet-libraries/page");
    await Page();
    expect(hasScopeMock).toHaveBeenCalledWith(SCOPE.PLATFORM_LIBRARY_WRITE);
  });

  it("renders the onboarding surface for an operator", async () => {
    const { default: Page } = await import("../app/(dashboard)/admin/fleet-libraries/page");
    const html = renderToStaticMarkup(await Page());
    expect(html).toContain("Fleet libraries");
  });
});

describe("platform-library:write scope", () => {
  it("is the wire string the backend enforces", () => {
    expect(SCOPE.PLATFORM_LIBRARY_WRITE).toBe("platform-library:write"); // pin test: literal is the contract
  });

  it("is independent — no other operator scope expands into it", () => {
    const held = expandScopes([SCOPE.MODEL_ADMIN, SCOPE.RUNNER_WRITE]);
    expect(held.has(SCOPE.PLATFORM_LIBRARY_WRITE)).toBe(false);
    expect(expandScopes([SCOPE.PLATFORM_LIBRARY_WRITE]).has(SCOPE.PLATFORM_LIBRARY_WRITE)).toBe(true);
  });
});

describe("onboardPlatformLibraryAction", () => {
  async function loadAction() {
    // with-token is mocked per-test so the action's scope gate and error mapping
    // are exercised without a Clerk session.
    vi.doMock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
    const mod = await import("../app/(dashboard)/admin/fleet-libraries/actions");
    return mod.onboardPlatformLibraryAction;
  }

  it("refuses before any round-trip when the session lacks the scope", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const action = await loadAction();
    const result = await action({ source_kind: "github", source_ref: REPO });
    expect(result.ok).toBe(false);
    expect(withTokenMock).not.toHaveBeenCalled();
    expect(onboardPlatformFleetLibraryMock).not.toHaveBeenCalled();
  });

  it("posts the admin endpoint with the operator's token on the happy path", async () => {
    const entry = { id: "platform-ops", name: "Platform operations diagnostician" };
    withTokenMock.mockImplementationOnce(async (fn: (t: string) => Promise<unknown>) => ({
      ok: true,
      data: await fn("tok"),
    }));
    onboardPlatformFleetLibraryMock.mockResolvedValueOnce(entry);
    const action = await loadAction();
    const result = await action({ source_kind: "github", source_ref: REPO });
    expect(result).toEqual({ ok: true, data: entry });
    expect(onboardPlatformFleetLibraryMock).toHaveBeenCalledWith(
      { source_kind: "github", source_ref: REPO },
      "tok",
    );
  });

  it("maps a backend 403 to the UZ error code rather than throwing", async () => {
    withTokenMock.mockImplementationOnce(async (fn: (t: string) => Promise<unknown>) => {
      try {
        await fn("tok");
        return { ok: true };
      } catch (e) {
        const err = e as ApiError;
        return { ok: false, error: err.message, status: err.status, errorCode: err.code };
      }
    });
    onboardPlatformFleetLibraryMock.mockRejectedValueOnce(
      new ApiError("insufficient scope", 403, "UZ-AUTH-022"),
    );
    const action = await loadAction();
    const result = await action({ source_kind: "github", source_ref: REPO });
    expect(result).toMatchObject({ ok: false, errorCode: "UZ-AUTH-022" });
  });
});
