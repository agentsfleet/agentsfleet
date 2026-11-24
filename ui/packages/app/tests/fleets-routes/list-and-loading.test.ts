import { FLEET_ID, response } from "../helpers/fleet-fixtures";
import { happyBilling, mockFetchBilling } from "./harness";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { fetchMock, authMock as auth } from "../helpers/dashboard-mocks";

describe("fleets routes — list and loading", () => {
  it("loading.tsx provides a common accessible page loader", async () => {
    const { default: Loading } = await import("../../app/(dashboard)/w/[workspaceId]/fleets/loading");
    render(React.createElement(Loading));
    const el = screen.getByRole("status", { name: "Loading page" });
    expect(el.textContent).toBe("Loading…");
    expect(el.getAttribute("aria-busy")).toBe("true");
  });

  it("fleets list page redirects to /sign-in when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/page");
    await expect(
      Page({ params: Promise.resolve({ workspaceId: "ws_1" }) }),
    ).rejects.toThrow("redirect:/sign-in");
  });

  it("opens the first conversation returned for this workspace", async () => {
    mockFetchBilling(happyBilling);
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/page");
    await expect(Page({params: Promise.resolve({workspaceId: "ws_1"})})).rejects.toThrow(`redirect:/w/ws_1/fleets/${FLEET_ID}`);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("opens New fleet when the workspace has no fleets", async () => {
    fetchMock.mockResolvedValue(response({items: [], next_cursor: null, total: null}));
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/page");
    await expect(Page({params: Promise.resolve({workspaceId: "ws_1"})})).rejects.toThrow("redirect:/w/ws_1/fleets/new");
  });
});


