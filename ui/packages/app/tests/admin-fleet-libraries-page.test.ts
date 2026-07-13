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
const listPlatformFleetLibraryMock = vi.fn();
const patchPlatformFleetLibraryEntryMock = vi.fn();
const deletePlatformFleetLibraryEntryMock = vi.fn();
// Every write revalidates the page — the table IS the confirmation, so the action
// must not return before asking Next to re-read it. Outside a request scope
// `revalidatePath` throws, so it is stubbed here and asserted on.
const revalidatePathMock = vi.fn();

vi.mock("next/navigation", () => ({ redirect }));
vi.mock("next/cache", () => ({ revalidatePath: revalidatePathMock }));
vi.mock("@/lib/auth/platform", () => ({ hasScope: hasScopeMock }));
vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/fleet-library", () => ({
  onboardPlatformFleetLibrary: onboardPlatformFleetLibraryMock,
  listPlatformFleetLibrary: listPlatformFleetLibraryMock,
  patchPlatformFleetLibraryEntry: patchPlatformFleetLibraryEntryMock,
  deletePlatformFleetLibraryEntry: deletePlatformFleetLibraryEntryMock,
}));

vi.mock("@/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView", () => ({
  default: () => React.createElement("div", { "data-fleet-libraries-view": "1" }, "Fleet library"),
}));

const NOT_PLATFORM_ADMIN = "/settings?notice=fleet-libraries-platform-admin-only";
const REPO = "agentsfleet/platform-ops";

beforeEach(() => {
  vi.clearAllMocks();
  hasScopeMock.mockResolvedValue(true);
  withTokenMock.mockReset();
  // The page reads the catalog server-side; by default it succeeds and is empty.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
  listPlatformFleetLibraryMock.mockResolvedValue({ entries: [] });
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

  it("renders the catalog surface for an operator", async () => {
    const { default: Page } = await import("../app/(dashboard)/admin/fleet-libraries/page");
    const html = renderToStaticMarkup(await Page());
    expect(html).toContain("Fleet library");
    expect(listPlatformFleetLibraryMock).toHaveBeenCalledWith("tok");
  });

  // "The catalog is empty" and "we could not reach the catalog" are different
  // facts, and an operator acts differently on each. A failed read must never
  // fall through to an empty table.
  it("renders the failure when the catalog read fails, not an empty table", async () => {
    withTokenMock.mockResolvedValueOnce({
      ok: false,
      error: "database unavailable",
      errorCode: "UZ-DB-001",
    });
    const { default: Page } = await import("../app/(dashboard)/admin/fleet-libraries/page");
    const html = renderToStaticMarkup(await Page());
    // Neither the empty state nor the table: the operator is told the read failed,
    // not shown a catalog that looks empty.
    expect(html).not.toContain("No fleets in the catalog");
    expect(html).not.toContain("Create fleet library");
    expect(html).toContain("load the fleet catalog");
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

// Every write revalidates the admin path. The table IS the confirmation — an
// operator must never have to guess whether the thing they just did took, and a
// stale table after a successful publish is exactly that guess.
describe("catalog write actions", () => {
  async function loadActions() {
    vi.doMock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
    return import("../app/(dashboard)/admin/fleet-libraries/actions");
  }

  it("patch: curates and publishes through the entry endpoint, then revalidates", async () => {
    const entry = { id: "platform-ops", visibility: "public" };
    withTokenMock.mockImplementationOnce(async (fn: (t: string) => Promise<unknown>) => ({
      ok: true,
      data: await fn("tok"),
    }));
    patchPlatformFleetLibraryEntryMock.mockResolvedValueOnce(entry);

    const { patchPlatformLibraryAction } = await loadActions();
    const result = await patchPlatformLibraryAction("platform-ops", { published: true });

    expect(result).toEqual({ ok: true, data: entry });
    expect(patchPlatformFleetLibraryEntryMock).toHaveBeenCalledWith(
      "platform-ops",
      { published: true },
      "tok",
    );
    expect(revalidatePathMock).toHaveBeenCalledWith("/admin/fleet-libraries");
  });

  it("patch: refuses before any round-trip when the session lacks the scope", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const { patchPlatformLibraryAction } = await loadActions();
    const result = await patchPlatformLibraryAction("platform-ops", { published: true });

    expect(result.ok).toBe(false);
    expect(patchPlatformFleetLibraryEntryMock).not.toHaveBeenCalled();
  });

  // A refused publish (UZ-CATALOG-002) must map, not throw — and must NOT
  // revalidate, because nothing changed.
  it("patch: maps a backend 409 and does not revalidate", async () => {
    withTokenMock.mockResolvedValueOnce({
      ok: false,
      error: "no bundle",
      errorCode: "UZ-CATALOG-002",
    });
    const { patchPlatformLibraryAction } = await loadActions();
    const result = await patchPlatformLibraryAction("platform-ops", { published: true });

    expect(result).toMatchObject({ ok: false, errorCode: "UZ-CATALOG-002" });
    expect(revalidatePathMock).not.toHaveBeenCalled();
  });

  it("delete: removes the entry and revalidates", async () => {
    withTokenMock.mockImplementationOnce(async (fn: (t: string) => Promise<unknown>) => ({
      ok: true,
      data: await fn("tok"),
    }));
    deletePlatformFleetLibraryEntryMock.mockResolvedValueOnce(undefined);

    const { deletePlatformLibraryAction } = await loadActions();
    const result = await deletePlatformLibraryAction("platform-ops");

    expect(result.ok).toBe(true);
    expect(deletePlatformFleetLibraryEntryMock).toHaveBeenCalledWith("platform-ops", "tok");
    expect(revalidatePathMock).toHaveBeenCalledWith("/admin/fleet-libraries");
  });

  it("delete: refuses before any round-trip when the session lacks the scope", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const { deletePlatformLibraryAction } = await loadActions();
    const result = await deletePlatformLibraryAction("platform-ops");

    expect(result.ok).toBe(false);
    expect(deletePlatformFleetLibraryEntryMock).not.toHaveBeenCalled();
  });

  // The route refuses to delete a published fleet (UZ-CATALOG-003). The action
  // surfaces that rather than pretending it worked.
  it("delete: maps the published-fleet refusal", async () => {
    withTokenMock.mockResolvedValueOnce({
      ok: false,
      error: "published",
      errorCode: "UZ-CATALOG-003",
    });
    const { deletePlatformLibraryAction } = await loadActions();
    const result = await deletePlatformLibraryAction("platform-ops");

    expect(result).toMatchObject({ ok: false, errorCode: "UZ-CATALOG-003" });
    expect(revalidatePathMock).not.toHaveBeenCalled();
  });

  it("add: revalidates on success so the new draft appears in the table", async () => {
    withTokenMock.mockImplementationOnce(async (fn: (t: string) => Promise<unknown>) => ({
      ok: true,
      data: await fn("tok"),
    }));
    onboardPlatformFleetLibraryMock.mockResolvedValueOnce({ id: "platform-ops" });

    const { onboardPlatformLibraryAction } = await loadActions();
    await onboardPlatformLibraryAction({ source_kind: "github", source_ref: REPO });

    expect(revalidatePathMock).toHaveBeenCalledWith("/admin/fleet-libraries");
  });
});
