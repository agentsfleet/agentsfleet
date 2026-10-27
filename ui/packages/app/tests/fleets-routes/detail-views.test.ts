import { FLEET_ID } from "../helpers/draft-fixtures";
import { detailBody, detailResponse, exhaustedBilling, happyBilling, mockFetchBilling } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { fetchMock, authMock as auth, listTenantWorkspacesCached } from "../helpers/dashboard-mocks";

describe("fleets routes — detail views", () => {
  it("fleets detail page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it.each([400, 404])("fleets detail uses notFound for a malformed or missing id (%i)", async (status) => {
    mockFetchBilling(happyBilling, {missingStatus: status});
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "missing" }) }),
    ).rejects.toThrow("notFound");
  });

  it("fleets detail page rethrows a server failure instead of rendering notFound", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith(`/fleets/${FLEET_ID}`)) {
        return {
          ok: false,
          status: 500,
          headers: { get: () => null },
          json: async () => ({
            error_code: "UZ-INTERNAL-001",
            detail: "Fleet read failed",
          }),
        };
      }
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/memories")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ items: [], total: 0, next_cursor: null }),
        };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({ items: [], next_cursor: null }),
      };
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }) }),
    ).rejects.toThrow("Fleet read failed");
  });

  it.each(["events", "memory", "setup"])("sends an old %s view link to the single fleet conversation", async view => {
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(Page({params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID}),
      searchParams: Promise.resolve({view})})).rejects.toThrow(`redirect:/w/ws_1/fleets/${FLEET_ID}`);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("fleet Chat remains available when the thread read fails, and never reads the inbox", async () => {
    const asked: string[] = [];
    fetchMock.mockImplementation(async (url: string) => {
      asked.push(url);
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      // `/messages` is the chat's thread read — the one request that replaced
      // the events-list-plus-per-turn-detail fan-out this view used to issue.
      if (url.includes("/messages") || url.includes("/events")) {
        throw new Error("summary down");
      }
      return detailResponse({ pending_approvals: 2 });
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    expect(markup).toContain("Latest outcome");
    expect(markup).toContain("Latest data unavailable.");
    expect(markup).not.toContain("No outcome recorded yet.");
    // The pending count comes off the fleet detail the page already holds:
    // no approvals read is issued to render it.
    expect(markup).toContain("2 approvals waiting");
    expect(asked.some((url) => url.includes("/approvals"))).toBe(false);
    expect(markup).toContain('data-fleet-workspace');
  });

  it("fleets detail page renders panels + exhaustion badge when tenant is exhausted", async () => {
    mockFetchBilling(exhaustedBilling);
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    expect(markup).toContain(detailBody().name);
    expect(markup).toContain("Balance exhausted");
  });

  it("opens directly on conversation without a Chat, Activity or Memory tab row", async () => {
    mockFetchBilling(happyBilling);
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(await Page({params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID})}));
    expect(markup).not.toContain('aria-label="Fleet sections"');
    expect(markup).not.toContain('?view=events');
    expect(markup).not.toContain('?view=memory');
    expect(markup).toContain('aria-label="Fleet actions"');
    expect(fetchMock.mock.calls.some(([url]) => String(url).includes("/memories"))).toBe(false);
  });

  it("fleets detail page renders without badge when not exhausted", async () => {
    mockFetchBilling(happyBilling);
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    expect(markup).not.toContain("Balance exhausted");
  });

  it("opens full-page setup editing from the fleet header", async () => {
    mockFetchBilling(happyBilling);
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(await Page({params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID})}));
    expect(markup).toContain(">Edit setup</a>");
    expect(markup).toContain(`/fleets/new?fleet=${FLEET_ID}&amp;setup_revision=1`);
    expect(markup).not.toContain("?view=setup");
  });

  it.each([false, true])("keeps runtime chat available while setup needs workspace access (available=%s)", async (available) => {
    mockFetchBilling(happyBilling);
    if (available) listTenantWorkspacesCached.mockResolvedValueOnce({items: [
      {id: "another-workspace", account: {tenant_id: "other-account"}},
      {id: "ws_1", account: {tenant_id: "authorized-account"}},
    ], total: 2});
    else listTenantWorkspacesCached.mockRejectedValueOnce(new TypeError("Workspace service unavailable"));
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(await Page({params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID})}));
    expect(markup).not.toContain('aria-label="Fleet sections"');
    expect(markup).toContain(">Edit setup</a>");
    expect(markup).toContain(`/fleets/new?fleet=${FLEET_ID}`);
  });
});
