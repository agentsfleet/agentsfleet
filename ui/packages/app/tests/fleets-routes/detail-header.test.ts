import { detailResponse, happyBilling } from "./harness";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { fetchMock } from "../helpers/dashboard-mocks";

describe("fleets routes — detail header and summary", () => {
  it("fleets detail page omits the WakePulse dot when the fleet is not active", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ items: [], next_cursor: null }),
        };
      }
      if (url.includes("/memories")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ items: [], total: 0, next_cursor: null }),
        };
      }
      // The chat's thread read: a real (empty) thread page, not the list
      // envelope the fall-through returns — the strip builds its summary from
      // this shape and rightly refuses one without `items`.
      if (url.includes("/messages")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ items: [], total: null, next_cursor: null }),
        };
      }
      if (url.includes("/events")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ items: [], next_cursor: null }),
        };
      }
      return detailResponse({ name: "platform-ops", status: "paused" });
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }),
      }),
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
      fleet_id: "zom_1",
      workspace_id: "ws_1",
      actor: "fleet",
      event_type: "fleet_run",
      status: "processed",
      tokens: null,
      wall_ms: null,
      failure_label: null,
      failure_detail: null,
      checkpoint_id: null,
      cost_nanos: null,
      created_at: 1_777_507_200_000,
      updated_at: 1_777_507_200_000,
    };
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) {
        return { ok: true, status: 200, json: async () => ({ items: [], next_cursor: null }) };
      }
      if (url.includes("/memories")) {
        return { ok: true, status: 200, json: async () => ({ items: [], total: 0, next_cursor: null }) };
      }
      // The thread read is what the chat renders from; the strip builds its
      // summary off this page and refuses a body without `items`.
      if (url.includes("/messages")) {
        return { ok: true, status: 200, json: async () => ({ items: [listRow], total: null, next_cursor: null }) };
      }
      // The single-event read carries an identifier after /events/; the list
      // read does not. Order matters — the list branch would swallow both.
      if (/\/events\/[^/?]+/.test(url)) {
        return { ok: false, status: 500, json: async () => ({ detail: "detail read failed" }) };
      }
      if (url.includes("/events")) {
        return { ok: true, status: 200, json: async () => ({ items: [listRow], next_cursor: null }) };
      }
      return detailResponse({ name: "platform-ops", status: "active" });
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }),
      }),
    );
    expect(markup).toContain("platform-ops");
  });

  it("fleet summary links an exact pending count to the filtered Approvals inbox", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) {
        throw new Error("the chat never reads the inbox to render its count");
      }
      if (url.includes("/memories")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ items: [], total: 0, next_cursor: null }),
        };
      }
      // The chat's thread read: a real (empty) thread page, not the list
      // envelope the fall-through returns — the strip builds its summary from
      // this shape and rightly refuses one without `items`.
      if (url.includes("/messages")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ items: [], total: null, next_cursor: null }),
        };
      }
      if (url.includes("/events")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ items: [], next_cursor: null }),
        };
      }
      return detailResponse({ name: "platform-ops", status: "active", pending_approvals: 1 });
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }),
      }),
    );
    expect(markup).toContain("1 approval waiting");
    expect(markup).toContain('href="/w/ws_1/approvals?fleetId=zom_1"');
  });

  it("redirects an obsolete Settings query to the canonical Chat URL", async () => {
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(
      Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }),
        searchParams: Promise.resolve({ view: "settings" }),
      }),
    ).rejects.toThrow("redirect:/w/ws_1/fleets/zom_1");
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
