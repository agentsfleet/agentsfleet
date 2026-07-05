import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ApiError } from "@/lib/api/errors";

// ── Shared mocks ───────────────────────────────────────────────────────────

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const authMock = vi.fn();
const listApiKeysMock = vi.fn();

vi.mock("next/navigation", () => ({ redirect, usePathname: () => "/", useRouter: () => ({ refresh: vi.fn() }) }));
vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));
vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));

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

// The "Create key" trigger ships behind a next/dynamic shim (M101 §5) — stub
// it out so the page test stays focused on page-level behaviour.
vi.mock("@/components/domain/island-dynamic/CreateApiKeyDialogDynamic", () => ({
  default: () => React.createElement("button", { type: "button" }, "Create key"),
}));

function mockAuth(token: string | null = "tok") {
  authMock.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(token), userId: "usr_1" });
}

beforeEach(() => vi.clearAllMocks());

// ── /settings redirect ────────────────────────────────────────────────────

describe("settings index redirect", () => {
  it("redirects /settings to /settings/api-keys (Workspace tab folded in)", async () => {
    mockAuth();
    const { default: SettingsPage } = await import("../app/(dashboard)/settings/page");
    await expect(SettingsPage()).rejects.toThrow("redirect:/settings/api-keys");
  });

  it("redirects /settings straight to /sign-in when there is no token (no double redirect)", async () => {
    mockAuth(null);
    const { default: SettingsPage } = await import("../app/(dashboard)/settings/page");
    await expect(SettingsPage()).rejects.toThrow("redirect:/sign-in");
  });
});

// ── /settings/api-keys page ───────────────────────────────────────────────

describe("api-keys page", () => {
  it("redirects to /sign-in when there is no token", async () => {
    mockAuth(null);
    const { default: Page } = await import("../app/(dashboard)/settings/api-keys/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("shows the API keys need admin access notice inline on a 403 — no redirect", async () => {
    mockAuth();
    listApiKeysMock.mockRejectedValueOnce(new ApiError("forbidden", 403, "UZ-AUTH-001"));
    const { default: Page } = await import("../app/(dashboard)/settings/api-keys/page");
    const html = renderToStaticMarkup(await Page());
    expect(html).toMatch(/API keys need admin access/i);
    expect(redirect).not.toHaveBeenCalled();
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

  it("operator: renders the API Keys title and lists keys newest-first", async () => {
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
    expect(html).toMatch(/API Keys/);
    expect(html).toContain("ci-runner");
    expect(html).toContain("old-zapier");
    expect(listApiKeysMock).toHaveBeenCalledWith("tok", expect.objectContaining({ sort: "-created_at" }));
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
