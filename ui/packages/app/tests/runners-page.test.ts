import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ApiError } from "@/lib/api/errors";

// ── Shared mocks ───────────────────────────────────────────────────────────

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const authMock = vi.fn();
const hasScopeMock = vi.fn();
const listRunnersMock = vi.fn();

vi.mock("next/navigation", () => ({ redirect }));
vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));
vi.mock("@/lib/auth/platform", () => ({ hasScope: hasScopeMock }));

// Partial mock — keep the real DEFAULT_SORT / DEFAULT_PAGE_SIZE the page passes.
vi.mock("@/lib/api/runners", async (orig) => ({
  ...(await orig<typeof import("@/lib/api/runners")>()),
  listRunners: listRunnersMock,
}));

// Stub the client list so the page test stays focused on page-level guards.
vi.mock("@/app/(dashboard)/admin/runners/components/RunnerList", () => ({
  default: ({ initial }: { initial: { items: Array<{ host_id: string }> } }) =>
    React.createElement(
      "div",
      { "data-runner-list": "1" },
      initial.items.map((i) => React.createElement("span", { key: i.host_id }, i.host_id)),
    ),
}));

const NOT_ADMIN = "/settings?notice=runners-platform-admin-only";

function mockAuth(token: string | null = "tok") {
  authMock.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(token) });
}

beforeEach(() => {
  vi.clearAllMocks();
  hasScopeMock.mockResolvedValue(true);
});

describe("admin/runners page", () => {
  it("redirects a caller without runner:read to settings with the operator notice (UI guard)", async () => {
    hasScopeMock.mockResolvedValueOnce(false);
    const { default: Page } = await import("../app/(dashboard)/admin/runners/page");
    await expect(Page()).rejects.toThrow(`redirect:${NOT_ADMIN}`);
    // The guard short-circuits before any token resolution or backend read.
    expect(listRunnersMock).not.toHaveBeenCalled();
  });

  it("redirects to /sign-in when the admin session has no token", async () => {
    mockAuth(null);
    const { default: Page } = await import("../app/(dashboard)/admin/runners/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("redirects to settings when the backend independently 403s the read", async () => {
    mockAuth();
    listRunnersMock.mockRejectedValueOnce(new ApiError("forbidden", 403, "UZ-AUTH-022"));
    const { default: Page } = await import("../app/(dashboard)/admin/runners/page");
    await expect(Page()).rejects.toThrow(`redirect:${NOT_ADMIN}`);
  });

  it("redirects to /sign-in when the backend returns 401", async () => {
    mockAuth();
    listRunnersMock.mockRejectedValueOnce(new ApiError("session expired", 401, "UZ-AUTH-401"));
    const { default: Page } = await import("../app/(dashboard)/admin/runners/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("re-throws a non-403/401 ApiError instead of redirecting", async () => {
    mockAuth();
    listRunnersMock.mockRejectedValueOnce(new ApiError("backend exploded", 500, "UZ-INTERNAL-001"));
    const { default: Page } = await import("../app/(dashboard)/admin/runners/page");
    await expect(Page()).rejects.toThrow("backend exploded");
  });

  it("platform admin: lists the fleet, requesting the default newest-first sort", async () => {
    mockAuth();
    listRunnersMock.mockResolvedValueOnce({
      items: [
        {
          id: "a",
          host_id: "web-prod-1",
          sandbox_tier: "landlock_full",
          admin_state: "active",
          liveness: "registered",
          labels: [],
          last_seen_at: 0,
          created_at: 2,
        },
      ],
      total: 1,
      page: 1,
      page_size: 25,
    });
    const { default: Page } = await import("../app/(dashboard)/admin/runners/page");
    const html = renderToStaticMarkup(await Page());
    expect(html).toContain("web-prod-1");
    expect(html).toMatch(/Runners/);
    expect(listRunnersMock).toHaveBeenCalledWith(
      "tok",
      expect.objectContaining({ page: 1, page_size: 25, sort: "-created_at" }),
    );
  });
});
