import { FLEET_ID } from "../helpers/draft-fixtures";
import { happyBilling, mockFetchBilling } from "./harness";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { fetchMock } from "../helpers/dashboard-mocks";

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
    expect(markup).toContain("platform-ops");
    expect(markup).not.toContain("Balance exhausted");
  });

  it("shows saved setup readiness and recovery while a fleet waits for access", async () => {
    mockFetchBilling(happyBilling, {fleet: {name: "fresh-bot", status: "installing"}});
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
      }),
    );
    // The saved fleet stays addressable while readiness blocks activation.
    expect(markup).toContain("installing");
    expect(markup).toContain('aria-label="Fleet status: installing"');
    expect(markup).not.toContain(">Killed<");
    expect(markup).toContain('aria-label="Saved fleet setup"');
    expect(markup).toContain("fresh-bot setup is waiting");
    expect(markup).toContain("Connect GitHub");
    expect(markup).toContain("Continue setup");
    expect(markup).not.toContain("Pending approvals");
  });

  it("fleet Trigger view degrades cleanly and never renders a webhook box", async () => {
    const respond = mockFetchBilling(happyBilling);
    fetchMock.mockImplementation(async (url: string) => {
      if (url.includes("/approvals")) throw new Error("approvals down");
      if (url.includes("/memories")) throw new Error("memories down");
      if (url.includes("/events")) throw new Error("events down");
      return respond(url);
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({
        params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
        searchParams: Promise.resolve({ view: "trigger" }),
      }),
    );
    expect(markup).toContain("platform-ops");
    expect(markup).toContain("Configured triggers");
    expect(markup).toContain("Chat and steer messages only");
    expect(markup).not.toContain("Webhook URL");
  });
});
