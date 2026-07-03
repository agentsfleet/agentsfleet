import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ApiError } from "@/lib/api/errors";

// ── Shared mocks ───────────────────────────────────────────────────────────
// page.tsx is the UI guard: platform-admin only, redirecting everyone else
// before any backend read. We mock the claim, auth token, and catalogue read so
// only the page-level guards + error mapping are under test; ModelsView is
// stubbed so this stays a page test, not a client-component test.

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const authMock = vi.fn();
const hasScopeMock = vi.fn();
const listAdminModelsMock = vi.fn();

vi.mock("next/navigation", () => ({ redirect }));
vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));
vi.mock("@/lib/auth/platform", () => ({ hasScope: hasScopeMock }));
vi.mock("@/lib/api/admin_models", async (orig) => ({
  ...(await orig<typeof import("@/lib/api/admin_models")>()),
  listAdminModels: listAdminModelsMock,
}));

// Stub the client view so the page test stays focused on page-level guards.
vi.mock("@/app/(dashboard)/admin/models/components/ModelsView", () => ({
  default: ({ initial }: { initial: { models: Array<{ model_id: string }> } }) =>
    React.createElement(
      "div",
      { "data-models-view": "1" },
      initial.models.map((m) => React.createElement("span", { key: m.model_id }, m.model_id)),
    ),
}));

const NOT_ADMIN = "/settings?notice=models-platform-admin-only";

function mockAuth(token: string | null = "tok") {
  authMock.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(token) });
}

beforeEach(() => {
  vi.clearAllMocks();
  hasScopeMock.mockResolvedValue(true);
});

describe("admin/models page", () => {
  it("redirects a caller without model:read to settings with the operator notice (UI guard)", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const { default: Page } = await import("../app/(dashboard)/admin/models/page");
    await expect(Page()).rejects.toThrow(`redirect:${NOT_ADMIN}`);
    // The guard short-circuits before any token resolution or backend read.
    expect(listAdminModelsMock).not.toHaveBeenCalled();
  });

  it("redirects to /sign-in when the admin session has no token", async () => {
    mockAuth(null);
    const { default: Page } = await import("../app/(dashboard)/admin/models/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
    expect(listAdminModelsMock).not.toHaveBeenCalled();
  });

  it("redirects to settings when the backend independently 403s the read", async () => {
    mockAuth();
    listAdminModelsMock.mockRejectedValueOnce(new ApiError("forbidden", 403, "UZ-AUTH-022"));
    const { default: Page } = await import("../app/(dashboard)/admin/models/page");
    await expect(Page()).rejects.toThrow(`redirect:${NOT_ADMIN}`);
  });

  it("redirects to /sign-in when the backend returns 401", async () => {
    mockAuth();
    listAdminModelsMock.mockRejectedValueOnce(new ApiError("session expired", 401, "UZ-AUTH-401"));
    const { default: Page } = await import("../app/(dashboard)/admin/models/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("re-throws a non-403/401 ApiError instead of redirecting", async () => {
    mockAuth();
    listAdminModelsMock.mockRejectedValueOnce(new ApiError("backend exploded", 500, "UZ-INTERNAL-001"));
    const { default: Page } = await import("../app/(dashboard)/admin/models/page");
    await expect(Page()).rejects.toThrow("backend exploded");
  });

  it("platform admin: renders ModelsView seeded with the catalogue", async () => {
    mockAuth();
    listAdminModelsMock.mockResolvedValueOnce({ models: [{ model_id: "glm-5.2" }, { model_id: "claude-opus-4-8" }] });
    const { default: Page } = await import("../app/(dashboard)/admin/models/page");
    const html = renderToStaticMarkup(await Page());
    expect(html).toContain("glm-5.2");
    expect(html).toContain("claude-opus-4-8");
    expect(listAdminModelsMock).toHaveBeenCalledWith("tok");
  });
});
