import { FLEET_ID } from "../helpers/draft-fixtures";
import { detailBody, happyBilling, mockFetchBilling } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { renderToReadableStream, renderToStaticMarkup } from "react-dom/server";
import { fetchMock } from "../helpers/dashboard-mocks";

vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetPendingChat", () => ({
  FleetPendingChat: ({fleet}: {fleet: {id: string}}) => `Setup conversation for ${fleet.id}`,
}));

describe("fleets routes — detail lifecycle states", () => {
  it("shows Delete instead of lifecycle controls after a fleet is killed", async () => {
    mockFetchBilling(happyBilling, {fleet: {status: "killed"}});
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );

    expect(markup).toContain("Delete fleet");
    expect(markup).not.toContain(">Stop<");
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

  it("routes an installing fleet to its setup conversation instead of the execution composer", async () => {
    mockFetchBilling(happyBilling, {fleet: {name: "fresh-bot", status: "installing"}});
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const stream = await renderToReadableStream(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    await stream.allReady;
    const markup = await new Response(stream).text();
    expect(markup).toContain(`Setup conversation for ${FLEET_ID}`);
    expect(markup).not.toContain('aria-label="Fleet status: installing"');
    expect(markup).not.toContain(">Killed<");
    expect(markup).not.toContain("Continue setup in chat");
    expect(markup).not.toContain("Pending approvals");
  });

  it("redirects the removed Setup view to fleet Chat", async () => {
    mockFetchBilling(happyBilling);
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    await expect(Page({params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID}),
      searchParams: Promise.resolve({view: "setup"})})).rejects.toThrow(`redirect:/w/ws_1/fleets/${FLEET_ID}`);
  });
});
