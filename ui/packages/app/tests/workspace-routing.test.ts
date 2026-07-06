import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Entry redirect + ownership guard. The URL workspaceId is a UX selector; the
// ownership guard is defence-in-depth on top of the real security boundary
// (`ownsWithinTenant`, server-side, per backend call).

const notFound = vi.fn(() => {
  throw new Error("notFound");
});
const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const auth = vi.fn();
const listTenantWorkspacesCached = vi.fn();

vi.mock("next/navigation", () => ({ notFound, redirect }));
vi.mock("@clerk/nextjs/server", () => ({ auth }));
vi.mock("@/lib/workspace", () => ({ listTenantWorkspacesCached }));
// The zero-workspace entry state is a client island — stub it so the server
// component test stays synchronous and doesn't pull the dynamic dialog chunk.
vi.mock("@/components/layout/NoWorkspaceEmptyState", () => ({
  default: () => React.createElement("div", { "data-testid": "no-workspace-empty-state" }),
}));

const OWNED = {
  items: [
    { id: "ws_first", name: "Alpha", created_at: 0 },
    { id: "ws_second", name: "Beta", created_at: 1 },
  ],
  total: 2,
};

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("tok_1") });
});
afterEach(() => {
  vi.clearAllMocks();
});

// ── Ownership guard: un-owned/invalid id → notFound (Invariant 1) ────────────
describe("workspace ownership guard layout", () => {
  async function importLayout() {
    return (await import("../app/(dashboard)/w/[workspaceId]/layout")).default;
  }

  it("test_unowned_workspace_notfound: an un-owned id renders notFound, never another workspace's data", async () => {
    listTenantWorkspacesCached.mockResolvedValue(OWNED);
    const Layout = await importLayout();
    await expect(
      Layout({
        children: React.createElement("div", { "data-testid": "child" }),
        params: Promise.resolve({ workspaceId: "ws_not_mine" }),
      }),
    ).rejects.toThrow("notFound");
    expect(notFound).toHaveBeenCalledOnce();
  });

  it("renders children for an owned workspace id (no notFound)", async () => {
    listTenantWorkspacesCached.mockResolvedValue(OWNED);
    const Layout = await importLayout();
    const el = await Layout({
      children: React.createElement("div", null, "workspace content"),
      params: Promise.resolve({ workspaceId: "ws_first" }),
    });
    const markup = renderToStaticMarkup(el as React.ReactElement);
    expect(markup).toContain("workspace content");
    expect(notFound).not.toHaveBeenCalled();
  });

  it("a deleted/stale id (empty owned list) → notFound, not a crash", async () => {
    listTenantWorkspacesCached.mockResolvedValue({ items: [], total: 0 });
    const Layout = await importLayout();
    await expect(
      Layout({
        children: React.createElement("div"),
        params: Promise.resolve({ workspaceId: "ws_ghost" }),
      }),
    ).rejects.toThrow("notFound");
  });

  it("a transient list-read failure fails OPEN (renders children) — the backend still gates", async () => {
    // The guard is UX/defence-in-depth; blanking a possibly-owned workspace to a
    // hard 404 on a list-endpoint blip is worse than letting `ownsWithinTenant`
    // gate the actual data calls. So a list error must NOT notFound.
    listTenantWorkspacesCached.mockRejectedValue(new Error("list endpoint down"));
    const Layout = await importLayout();
    const el = await Layout({
      children: React.createElement("div", null, "still renders"),
      params: Promise.resolve({ workspaceId: "ws_first" }),
    });
    const markup = renderToStaticMarkup(el as React.ReactElement);
    expect(markup).toContain("still renders");
    expect(notFound).not.toHaveBeenCalled();
  });

  it("redirects to /sign-in without a token", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const Layout = await importLayout();
    await expect(
      Layout({
        children: React.createElement("div"),
        params: Promise.resolve({ workspaceId: "ws_first" }),
      }),
    ).rejects.toThrow("redirect:/sign-in");
    expect(notFound).not.toHaveBeenCalled();
  });
});

// ── Entry redirect + zero-workspace empty state ──────────────────────────────
describe("dashboard entry page", () => {
  async function importEntry() {
    return (await import("../app/(dashboard)/page")).default;
  }

  it("test_root_redirects_to_default_workspace: redirects to the first owned workspace", async () => {
    listTenantWorkspacesCached.mockResolvedValue(OWNED);
    const Page = await importEntry();
    await expect(Page()).rejects.toThrow("redirect:/w/ws_first");
  });

  it("test_no_workspace_empty_state: zero workspaces → create-workspace empty state, no throw", async () => {
    listTenantWorkspacesCached.mockResolvedValue({ items: [], total: 0 });
    const Page = await importEntry();
    const markup = renderToStaticMarkup((await Page()) as React.ReactElement);
    expect(markup).toContain("no-workspace-empty-state");
    expect(redirect).not.toHaveBeenCalledWith(expect.stringMatching(/^\/w\//));
  });

  it("redirects to /sign-in without a token", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const Page = await importEntry();
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });
});
