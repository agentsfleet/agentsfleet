import { FLEET_ID, response } from "../helpers/draft-fixtures";
import { happyBilling, mockFetchBilling } from "./harness";
import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { TooltipProvider } from "@agentsfleet/design-system";
import { fetchMock } from "../helpers/dashboard-mocks";

describe("fleets routes — detail header and summary", () => {
  it("fleets detail page omits the WakePulse dot when the fleet is not active", async () => {
    mockFetchBilling(happyBilling, {fleet: {status: "paused"}});
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        await Page({
          params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
        }),
      ),
    );
    expect(markup).toContain("paused");
    expect(markup).not.toContain("data-live");
  });

  // The transcript is the one surface that wants the bodies, and the list read
  // no longer carries them — so each row is re-read as a detail. A detail that
  // fails must degrade to its list row: the turn keeps its header and outcome
  // and the page still renders. Without that arm the rejection propagates out
  // of Promise.all and the whole fleet page dies on one bad event.
  it("fleet transcript survives a turn whose detail re-read fails", async () => {
    const listRow = {
      event_id: "ev_1",
      fleet_id: FLEET_ID,
      workspace_id: "ws_1",
      actor: "fleet",
      event_type: "fleet_run",
      status: "processed",
      tokens: null,
      wall_ms: null,
      failure_label: null,
      failure_detail: null,
      cost_nanos: null,
      created_at: "2026-04-30T00:00:00Z",
      updated_at: "2026-04-30T00:00:00Z",
    };
    const respond = mockFetchBilling(happyBilling);
    fetchMock.mockImplementation(async (url: string) => {
      if (url.includes("/messages")) return response({items: [listRow], total: null, next_cursor: null});
      if (/\/events\/[^/?]+/.test(url)) return response({detail: "detail read failed"}, 500);
      if (url.includes("/events")) return response({items: [listRow], next_cursor: null});
      return respond(url);
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        await Page({
          params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
        }),
      ),
    );
    expect(markup).toContain("platform-ops");
  });

  it("fleet summary links an exact pending count to the filtered Approvals inbox", async () => {
    const respond = mockFetchBilling(happyBilling, {fleet: {pending_approvals: 1}});
    fetchMock.mockImplementation(async (url: string) => {
      if (url.includes("/approvals")) throw new Error("the chat never reads the inbox to render its count");
      return respond(url);
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        await Page({
          params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
        }),
      ),
    );
    expect(markup).toContain("1 approval waiting");
    expect(markup).toContain(`href="/w/ws_1/approvals?fleetId=${FLEET_ID}"`);
  });

  it("redirects an obsolete Settings query to the canonical Chat URL", async () => {
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(
      Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
        searchParams: Promise.resolve({ view: "settings" }),
      }),
    ).rejects.toThrow(`redirect:/w/ws_1/fleets/${FLEET_ID}`);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
