import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

const { auth, getCatalog } = vi.hoisted(() => ({ auth: vi.fn(), getCatalog: vi.fn() }));
vi.mock("@clerk/nextjs/server", () => ({ auth }));
vi.mock("next/navigation", () => ({ redirect: (path: string) => { throw new Error(`redirect:${path}`); } }));
vi.mock("@/lib/api/connectors", () => ({ getConnectorCatalog: getCatalog }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/integrations/components/IntegrationsConnectors", () => ({
  default: ({ catalog, workspaceId, catalogError }: {
    catalog: ReadonlyArray<{ id: string; connected: boolean; label: string | null }>;
    workspaceId: string; catalogError?: { code: string; status: number | null };
  }) => React.createElement("div", {
    "data-workspace": workspaceId, "data-catalog": JSON.stringify(catalog),
    "data-error": catalogError ? `${catalogError.code}:${catalogError.status ?? "none"}` : "",
  }),
}));
import { ConnectorStatus } from "@/app/(dashboard)/w/[workspaceId]/integrations/components/connector-status";
import { ApiError } from "@/lib/api/errors";

const WORKSPACE = "ws_1";
const CATALOG = [{ id: "jira", connected: true, label: "Support Jira" }, { id: "slack", connected: false, label: null }];
beforeEach(() => {
  vi.resetAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("session-token") });
  getCatalog.mockResolvedValue(CATALOG);
});

describe("workspace connector loading", () => {
  it("uses one workspace request for every card, status and label", async () => {
    const markup = renderToStaticMarkup(await ConnectorStatus({ workspaceId: WORKSPACE }));
    expect(getCatalog).toHaveBeenCalledExactlyOnceWith(WORKSPACE, "session-token");
    expect(markup).toContain("Support Jira");
    expect(markup).toContain("slack");
    expect(markup).toContain('data-workspace="ws_1"');
  });

  it.each([new Error("unavailable"), new ApiError("unavailable", 503, "UZ-INTERNAL-003")])("shows a retryable load failure, never invented connection data: %s", async error => {
    getCatalog.mockRejectedValueOnce(error);
    const markup = renderToStaticMarkup(await ConnectorStatus({ workspaceId: WORKSPACE }));
    expect(markup).toContain('data-catalog="[]"');
    expect(markup).toContain(error instanceof ApiError ? "UZ-INTERNAL-003:503" : "UZ-UNKNOWN:none");
    expect(getCatalog).toHaveBeenCalledOnce();
  });

  it("does not request workspace data without a session", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    await expect(ConnectorStatus({ workspaceId: WORKSPACE })).rejects.toThrow("redirect:/sign-in");
    expect(getCatalog).not.toHaveBeenCalled();
  });

  it("returns the page shell without waiting for connector data", async () => {
    getCatalog.mockReturnValue(new Promise(() => {}));
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/integrations/page");
    const page = await Page({ params: Promise.resolve({ workspaceId: WORKSPACE }) });
    expect(React.isValidElement(page)).toBe(true);
    expect(getCatalog).not.toHaveBeenCalled();
  });
});
