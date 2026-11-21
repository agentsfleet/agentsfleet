import { FLEET_ID, response } from "../helpers/draft-fixtures";
import { happyBilling, mockFetchBilling, detailResponse } from "./harness";
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

  it("opens setup when the workspace has no fleets", async () => {
    fetchMock.mockResolvedValue(response({items: [], next_cursor: null, total: null}));
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/page");
    await expect(Page({params: Promise.resolve({workspaceId: "ws_1"})})).rejects.toThrow("redirect:/w/ws_1/fleets/new");
  });
});

describe("edit a saved fleet from history", () => {
  const params = Promise.resolve({workspaceId: "ws_1", id: FLEET_ID});
  const page = async () => (await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/setup/page")).default({params});

  it("edits the current saved revision", async () => {
    fetchMock.mockResolvedValue(detailResponse({setup_revision: "7"}));
    await expect(page()).rejects.toThrow(`redirect:/w/ws_1/fleets/new?fleet=${FLEET_ID}&setup_revision=7`);
  });
  it("opens archived history without editing", async () => {
    fetchMock.mockResolvedValue(detailResponse({status: "archived"}));
    await expect(page()).rejects.toThrow(`redirect:/w/ws_1/fleets/${FLEET_ID}`);
  });
  it("shows the missing-fleet page for a deleted fleet", async () => {
    fetchMock.mockResolvedValue(response({}, 404));
    await expect(page()).rejects.toThrow("notFound");
  });
  it("propagates service and decoding errors", async () => {
    fetchMock.mockResolvedValueOnce(response({detail: "Fleet unavailable"}, 503)).mockResolvedValueOnce(response({}));
    await expect(page()).rejects.toMatchObject({status: 503});
    await expect(page()).rejects.toThrow();
  });
});
