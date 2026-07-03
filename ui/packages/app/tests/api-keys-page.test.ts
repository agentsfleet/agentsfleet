import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ApiError } from "@/lib/api/errors";

// ── Shared mocks ───────────────────────────────────────────────────────────

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const authMock = vi.fn();
const resolveActiveWorkspaceMock = vi.fn().mockResolvedValue({ id: "ws_1", name: "Prod" });
const listTenantWorkspacesCachedMock = vi.fn().mockResolvedValue({ items: [], total: 0 });
const listApiKeysMock = vi.fn();

vi.mock("next/navigation", () => ({ redirect, usePathname: () => "/", useRouter: () => ({ refresh: vi.fn() }) }));
vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));
vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));
vi.mock("@/lib/workspace", () => ({
  listTenantWorkspacesCached: listTenantWorkspacesCachedMock,
  resolveActiveWorkspace: resolveActiveWorkspaceMock,
  // The settings index renders via resolveActiveWorkspaceId — derive it from
  // the legacy mock so existing `.mockResolvedValue({ id })` setups still drive it.
  resolveActiveWorkspaceId: async (token: string) => {
    const ws = (await resolveActiveWorkspaceMock(token)) as { id: string } | null;
    return ws ? { id: ws.id, source: "cookie" as const } : null;
  },
}));

// Partial mock — keep the real DEFAULT_SORT / DEFAULT_PAGE_SIZE the page passes.
vi.mock("@/lib/api/api_keys", async (orig) => ({
  ...(await orig<typeof import("@/lib/api/api_keys")>()),
  listApiKeys: listApiKeysMock,
}));

// Stub the client list so the page test stays focused on page-level behaviour.
vi.mock("@/app/(dashboard)/settings/api-keys/components/ApiKeyList", () => ({
  default: ({ initial }: { initial: { items: Array<{ key_name: string }> } }) =>
    React.createElement(
      "div",
      { "data-api-key-list": "1" },
      initial.items.map((i) => React.createElement("span", { key: i.key_name }, i.key_name)),
    ),
}));

function mockAuth(token: string | null = "tok") {
  authMock.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(token), userId: "usr_1" });
}

beforeEach(() => vi.clearAllMocks());

// ── Settings index card ──────────────────────────────────────────────────

describe("settings index", () => {
  it("renders a card linking to /settings/api-keys", async () => {
    mockAuth();
    const { default: SettingsPage } = await import("../app/(dashboard)/settings/page");
    const html = renderToStaticMarkup(await SettingsPage());
    expect(html).toContain('href="/settings/api-keys"');
    expect(html).toMatch(/API keys/i);
    expect(html).toMatch(/Manage workspace/i);
  });

  it("renders the standard workspace subtitle and a copy control for the workspace ID", async () => {
    // The Workspace page carries a PageHeader subtitle like every other page,
    // and the workspace ID (what the CLI/API target) gets a one-click copy
    // affordance. The mocked workspace resolves an id (name null), so the ID
    // copy control renders even when the name is absent.
    mockAuth();
    const { default: SettingsPage } = await import("../app/(dashboard)/settings/page");
    const html = renderToStaticMarkup(await SettingsPage());
    expect(html).toContain("Switch workspaces or create one.");
    expect(html).toContain("Copy workspace ID");
  });

  it("shows the admin-access notice when redirected from the api-keys guard", async () => {
    mockAuth();
    const { default: SettingsPage } = await import("../app/(dashboard)/settings/page");
    const html = renderToStaticMarkup(
      await SettingsPage({ searchParams: Promise.resolve({ notice: "api-keys-operator-only" }) }),
    );
    expect(html).toMatch(/API keys need admin access/i);
  });

  it("omits the notice when no redirect param is present", async () => {
    mockAuth();
    const { default: SettingsPage } = await import("../app/(dashboard)/settings/page");
    const html = renderToStaticMarkup(await SettingsPage({ searchParams: Promise.resolve({}) }));
    expect(html).not.toMatch(/need admin access/i);
  });
});

// ── /settings/api-keys page ───────────────────────────────────────────────

describe("api-keys page", () => {
  it("redirects a user-role principal (backend 403) to /settings with a notice", async () => {
    mockAuth();
    listApiKeysMock.mockRejectedValueOnce(new ApiError("forbidden", 403, "UZ-AUTH-001"));
    const { default: Page } = await import("../app/(dashboard)/settings/api-keys/page");
    await expect(Page()).rejects.toThrow("redirect:/settings?notice=api-keys-operator-only");
  });

  it("redirects to /sign-in when there is no token", async () => {
    mockAuth(null);
    const { default: Page } = await import("../app/(dashboard)/settings/api-keys/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("operator: lists keys, requesting the default newest-first sort", async () => {
    mockAuth();
    listApiKeysMock.mockResolvedValueOnce({
      items: [
        { id: "a", key_name: "ci-runner", active: true, created_at: 2, last_used_at: null, revoked_at: null },
        { id: "b", key_name: "old-zapier", active: false, created_at: 1, last_used_at: null, revoked_at: 1 },
      ],
      total: 2,
      page: 1,
      page_size: 25,
    });
    const { default: Page } = await import("../app/(dashboard)/settings/api-keys/page");
    const html = renderToStaticMarkup(await Page());
    expect(html).toContain("ci-runner");
    expect(html).toContain("old-zapier");
    expect(listApiKeysMock).toHaveBeenCalledWith("tok", expect.objectContaining({ sort: "-created_at" }));
  });

  it("redirects to /sign-in when the backend returns 401", async () => {
    mockAuth();
    listApiKeysMock.mockRejectedValueOnce(new ApiError("session expired", 401, "UZ-AUTH-006"));
    const { default: Page } = await import("../app/(dashboard)/settings/api-keys/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("re-throws a non-403/401 ApiError instead of redirecting", async () => {
    mockAuth();
    listApiKeysMock.mockRejectedValueOnce(new ApiError("backend exploded", 500, "UZ-INTERNAL-003"));
    const { default: Page } = await import("../app/(dashboard)/settings/api-keys/page");
    await expect(Page()).rejects.toThrow("backend exploded");
  });
});

// ── loading skeleton ──────────────────────────────────────────────────────

describe("api-keys loading skeleton", () => {
  it("renders the page title above skeleton placeholders", async () => {
    const { default: Loading } = await import("../app/(dashboard)/settings/api-keys/loading");
    const html = renderToStaticMarkup(React.createElement(Loading));
    expect(html).toMatch(/API keys/i);
  });
});
