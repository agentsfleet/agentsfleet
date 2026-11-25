import { FLEET_ID } from "../helpers/fleet-fixtures";
import { detailBody, happyBilling, mockFetchBilling } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { renderToReadableStream, renderToStaticMarkup } from "react-dom/server";
import { fetchMock } from "../helpers/dashboard-mocks";

describe("fleets routes — detail lifecycle states", () => {
  it("shows archived history and keeps destructive controls in the action menu", async () => {
    mockFetchBilling(happyBilling, {fleet: {status: "archived"}});
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );

    expect(markup).toContain('aria-label="Fleet status: archived"');
    expect(markup).toContain('aria-label="Fleet actions"');
    expect(markup).not.toContain('aria-label="Chat composer"');
    expect(markup).not.toContain(">Stop<");
    expect(markup).not.toContain(">Resume<");
    expect(markup).not.toContain(">Kill<");
  });

  it("shows an unknown fleet status without falsely presenting terminal actions", async () => {
    mockFetchBilling(happyBilling, {fleet: {status: "draining"}});
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
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
    const respond = mockFetchBilling(happyBilling);
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) throw new Error("network down");
      return respond(url);
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    expect(markup).toContain(detailBody().name);
    expect(markup).not.toContain("Balance exhausted");
  });

  it("opens a new fleet straight onto its chat, with nothing to set up", async () => {
    mockFetchBilling(happyBilling, {fleet: {name: "fresh-bot", status: "active"}});
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const stream = await renderToReadableStream(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    await stream.allReady;
    const markup = await new Response(stream).text();
    expect(markup).toContain(">fresh-bot</h1>");
    expect(markup).not.toContain("Fleet status:");
    expect(markup.toLowerCase()).not.toContain("setup");
    expect(markup).not.toContain("Pending approvals");
  });

  it("redirects the removed Setup view to fleet Chat", async () => {
    mockFetchBilling(happyBilling);
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(Page({params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID}),
      searchParams: Promise.resolve({view: "setup"})})).rejects.toThrow(`redirect:/w/ws_1/fleets/${FLEET_ID}`);
  });
});
