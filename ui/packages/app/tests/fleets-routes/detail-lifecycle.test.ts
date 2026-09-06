import { detailResponse, happyBilling } from "./harness";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { fetchMock } from "../helpers/dashboard-mocks";

describe("fleets routes — detail lifecycle states", () => {
  it("shows Delete instead of lifecycle controls after a fleet is killed", async () => {
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
      return detailResponse({ status: "killed" });
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }),
      }),
    );

    expect(markup).toContain("Delete fleet");
    expect(markup).not.toContain(">Stop<");
    expect(markup).not.toContain(">Kill<");
  });

  it("shows an unknown fleet status without falsely presenting terminal actions", async () => {
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
      return detailResponse({ status: "draining" });
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }),
      }),
    );

    expect(markup).toContain('aria-label="Fleet status: draining"');
    expect(markup).toContain("draining");
    expect(markup).not.toContain(">Killed<");
    expect(markup).not.toContain("Delete fleet");
    expect(markup).not.toContain(">Stop<");
    expect(markup).not.toContain(">Kill<");
  });

  it("fleets detail page handles billing fetch failure gracefully (catch branch)", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        throw new Error("network down");
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
      return detailResponse();
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }),
      }),
    );
    expect(markup).toContain("platform-ops");
    expect(markup).not.toContain("Balance exhausted");
  });

  // A still-provisioning fleet shows the install states on its own page (the
  // gate holds the panels until ready), with an installing indicator in the
  // header — so progress is never hidden, and "Open fleet" lands here while
  // installing and resolves in place.
  it("test_installing_fleet_always_visible — detail page shows install states + indicator while installing", async () => {
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
      return detailResponse({ name: "fresh-bot", status: "installing" });
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }),
      }),
    );
    // Header carries the status label + the installing live indicator.
    expect(markup).toContain("installing");
    expect(markup).toContain('aria-label="Fleet status: installing"');
    expect(markup).not.toContain(">Killed<");
    expect(markup).toContain("data-live");
    // The install states surface is shown; the gate withholds the lower panels.
    expect(markup).toContain("Install states");
    expect(markup).not.toContain("Pending approvals");
  });

  it("fleet Trigger view degrades cleanly and never renders a webhook box", async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      if (url.includes("/approvals")) throw new Error("approvals down");
      if (url.includes("/memories")) throw new Error("memories down");
      if (url.includes("/events")) throw new Error("events down");
      return detailResponse({ name: "platform-ops", status: "active" });
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }),
        searchParams: Promise.resolve({ view: "trigger" }),
      }),
    );
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("Trigger source");
    expect(markup).toContain("No triggers declared");
    expect(markup).not.toContain("Webhook URL");
  });
});
