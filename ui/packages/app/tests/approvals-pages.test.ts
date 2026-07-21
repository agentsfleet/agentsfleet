import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

const WORKSPACE_ID = "ws_pages_001";
const GATE_ID = "01999999-0000-7000-8000-000000000001";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701";
const TOKEN = "token_pages";

const { getTokenMock, listApprovalsMock, getApprovalMock, notFound, redirect } =
  vi.hoisted(() => ({
    getTokenMock: vi.fn(),
    listApprovalsMock: vi.fn(),
    getApprovalMock: vi.fn(),
    notFound: vi.fn(() => {
      throw new Error("notFound");
    }),
    redirect: vi.fn((path: string) => {
      throw new Error(`redirect:${path}`);
    }),
  }));

vi.mock("next/navigation", () => ({ notFound, redirect }));
vi.mock("@clerk/nextjs/server", () => ({
  auth: vi.fn(async () => ({ getToken: getTokenMock })),
}));
vi.mock("@/lib/api/approvals", () => ({
  listApprovals: listApprovalsMock,
  getApproval: getApprovalMock,
}));
vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));
// Client child components — stub to keep server-component tests synchronous.
vi.mock("@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList", () => ({
  default: (props: { initialItems: unknown[]; fleetId?: string }) =>
    React.createElement(
      "div",
      {
        "data-stub": "ApprovalsList",
        "data-initial-items": String(props.initialItems.length),
        "data-fleet-id": props.fleetId,
      },
      "approvals-list-stub",
    ),
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/approvals/[gateId]/ResolveButtons", () => ({
  default: () => React.createElement("div", { "data-stub": "ResolveButtons" }, "resolve-stub"),
}));

beforeEach(() => {
  getTokenMock.mockResolvedValue(TOKEN);
  listApprovalsMock.mockResolvedValue({ items: [], next_cursor: null });
});

afterEach(() => {
  getTokenMock.mockReset();
  listApprovalsMock.mockReset();
  getApprovalMock.mockReset();
  notFound.mockClear();
});

// ── /approvals (workspace inbox) ──────────────────────────────────────

describe("ApprovalsPage (workspace inbox)", () => {
  it("redirects to /sign-in when no token", async () => {
    getTokenMock.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/page");
    await expect(Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID }) })).rejects.toThrow(
      "redirect:/sign-in",
    );
  });

  it("page shell streams the header + skeleton before the inbox", async () => {
    // ApprovalsData is an async child, so renderToStaticMarkup renders the
    // Suspense skeleton in its place — the list stub stays absent until it
    // streams in, but the header title paints immediately.
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/page");
    const markup = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID }) }));
    expect(markup).toContain("Approvals"); // PageTitle in the shell
    expect(markup).toContain("animate-pulse"); // Skeleton fallback
    expect(markup).not.toContain("approvals-list-stub"); // data not yet resolved
  });

  it("accepts a fleet filter in the page-shell query", async () => {
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: WORKSPACE_ID }),
        searchParams: Promise.resolve({ fleetId: FLEET_ID }),
      }),
    );
    expect(markup).toContain("Approvals");
  });

  it("ApprovalsData returns null when the token is missing", async () => {
    getTokenMock.mockResolvedValueOnce(null);
    const { ApprovalsData } = await import("../app/(dashboard)/w/[workspaceId]/approvals/page");
    expect(await ApprovalsData({ workspaceId: WORKSPACE_ID })).toBeNull();
  });

  it("renders the inbox section and forwards items to the list stub", async () => {
    listApprovalsMock.mockResolvedValueOnce({
      items: [
        {
          gate_id: GATE_ID,
          fleet_id: FLEET_ID,
          fleet_name: "approvals-a",
          workspace_id: WORKSPACE_ID,
          action_id: "act_001",
          tool_name: "write_repo",
          action_name: "create_pr",
          gate_kind: "destructive_action",
          proposed_action: "Open PR titled X",
          evidence: {},
          blast_radius: "single repo branch",
          status: "pending",
          detail: "",
          requested_at: 1,
          timeout_at: 2,
          updated_at: null,
          resolved_by: "",
        },
      ],
      next_cursor: null,
    });
    const { ApprovalsData } = await import("../app/(dashboard)/w/[workspaceId]/approvals/page");
    const markup = renderToStaticMarkup(
      React.createElement(React.Fragment, null, await ApprovalsData({ workspaceId: WORKSPACE_ID })),
    );
    expect(markup).toContain("Pending"); // section aria-label
    expect(markup).toContain("approvals-list-stub");
    expect(markup).toContain('data-initial-items="1"');
  });

  it("falls back to empty initial list when listApprovals rejects", async () => {
    listApprovalsMock.mockRejectedValueOnce(new Error("upstream 503"));
    const { ApprovalsData } = await import("../app/(dashboard)/w/[workspaceId]/approvals/page");
    const markup = renderToStaticMarkup(
      React.createElement(React.Fragment, null, await ApprovalsData({ workspaceId: WORKSPACE_ID })),
    );
    expect(markup).toContain('data-initial-items="0"');
  });

  it("filters the inbox and list pagination to one fleet", async () => {
    const { ApprovalsData } = await import("../app/(dashboard)/w/[workspaceId]/approvals/page");
    const markup = renderToStaticMarkup(
      React.createElement(
        React.Fragment,
        null,
        await ApprovalsData({ workspaceId: WORKSPACE_ID, fleetId: FLEET_ID }),
      ),
    );
    expect(listApprovalsMock).toHaveBeenCalledWith(WORKSPACE_ID, TOKEN, {
      limit: 50,
      fleetId: FLEET_ID,
    });
    expect(markup).toContain(`data-fleet-id="${FLEET_ID}"`);
  });
});

// ── /approvals/[gateId] (detail) ──────────────────────────────────────

describe("ApprovalDetailPage", () => {
  function gateFixture(over: Partial<Record<string, unknown>> = {}) {
    return {
      gate_id: GATE_ID,
      fleet_id: FLEET_ID,
      fleet_name: "approvals-a",
      workspace_id: WORKSPACE_ID,
      action_id: "act_001",
      tool_name: "write_repo",
      action_name: "create_pr",
      gate_kind: "destructive_action",
      proposed_action: "Open PR titled wire approval inbox",
      evidence: { files: ["a", "b"] },
      blast_radius: "single repo branch",
      status: "pending",
      detail: "",
      requested_at: 1_700_000_000_000,
      timeout_at: 1_700_086_400_000,
      updated_at: null,
      resolved_by: "",
      ...over,
    };
  }

  it("redirects to /sign-in when no token", async () => {
    getTokenMock.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page");
    await expect(Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID, gateId: GATE_ID }) })).rejects.toThrow(
      "redirect:/sign-in",
    );
  });

  it("notFound when getApproval returns null", async () => {
    getApprovalMock.mockRejectedValueOnce(new Error("404"));
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page");
    await expect(Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID, gateId: GATE_ID }) })).rejects.toThrow(
      "notFound",
    );
  });

  it("renders proposed action, evidence JSON, and Resolve panel for pending gates", async () => {
    getApprovalMock.mockResolvedValueOnce(gateFixture());
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page");
    const markup = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID, gateId: GATE_ID }) }));
    expect(markup).toContain("Open PR titled wire approval inbox");
    expect(markup).toContain("approvals-a");
    expect(markup).toContain("destructive_action");
    expect(markup).toContain("write_repo");
    expect(markup).toContain("single repo branch");
    expect(markup).toContain("&quot;files&quot;");
    expect(markup).toContain("resolve-stub"); // ResolveButtons rendered
    expect(markup).toContain("pending");
  });

  it("shows Resolution section instead of Resolve panel when status is approved", async () => {
    getApprovalMock.mockResolvedValueOnce(
      gateFixture({
        status: "approved",
        resolved_by: "user:user_x",
        updated_at: 1_700_000_001_000,
      }),
    );
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page");
    const markup = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID, gateId: GATE_ID }) }));
    expect(markup).toContain("Resolution");
    expect(markup).toContain("approved");
    expect(markup).toContain("user:user_x");
    expect(markup).not.toContain("resolve-stub"); // ResolveButtons NOT rendered
  });

  it("renders Resolution panel for denied / timed_out without crashing on null updated_at", async () => {
    getApprovalMock.mockResolvedValueOnce(
      gateFixture({ status: "denied", resolved_by: "user:user_x", updated_at: null }),
    );
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page");
    const markup = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID, gateId: GATE_ID }) }));
    expect(markup).toContain("denied");
  });

  it("renders timed_out + auto_killed status badges in the amber variant", async () => {
    getApprovalMock.mockResolvedValueOnce(
      gateFixture({ status: "timed_out", resolved_by: "system:timeout" }),
    );
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page");
    const markup = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID, gateId: GATE_ID }) }));
    expect(markup).toContain("timed_out");
    expect(markup).toContain("system:timeout");
  });

  it("falls back to tool_name:action_name when proposed_action is empty", async () => {
    getApprovalMock.mockResolvedValueOnce(
      gateFixture({ proposed_action: "", tool_name: "write_repo", action_name: "create_pr" }),
    );
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page");
    const markup = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID, gateId: GATE_ID }) }));
    expect(markup).toContain("write_repo:create_pr");
  });

  it("hides Kind and Blast radius rows when those fields are empty", async () => {
    getApprovalMock.mockResolvedValueOnce(
      gateFixture({ gate_kind: "", blast_radius: "" }),
    );
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page");
    const markup = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID, gateId: GATE_ID }) }));
    expect(markup).not.toContain(">Kind<");
    expect(markup).not.toContain(">Blast radius<");
  });

  it("Resolution renders (unknown) when resolved_by is empty and detail when present", async () => {
    getApprovalMock.mockResolvedValueOnce(
      gateFixture({
        status: "approved",
        resolved_by: "",
        updated_at: 1_700_000_001_000,
        detail: "approved with caveat",
      }),
    );
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page");
    const markup = renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID, gateId: GATE_ID }) }));
    expect(markup).toContain("(unknown)");
    expect(markup).toContain("approved with caveat");
  });
});
