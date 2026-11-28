import { FLEET_ID, GRANT_ID, MODEL_REF, SCHEDULE, TRIGGER_ID, response } from "../helpers/fleet-fixtures";
import { detailBody, happyBilling, mockFetchBilling } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { Children, createElement, isValidElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { TooltipProvider } from "@agentsfleet/design-system";
import { authMock, fetchMock } from "../helpers/dashboard-mocks";
import { FleetWorkspace } from "@/components/domain/fleet/FleetWorkspace";
import { chatProps } from "./page-parts";

const FLEET_TITLE_MARKUP = ">platform-ops</h1>";

describe("fleets routes — detail header and summary", () => {
  it.each([MODEL_REF, "model:other"])("hands the fleet's model and network to its workspace and labels the composer (%s)", async (model) => {
    mockFetchBilling(happyBilling, {fleet: {model_ref: model, http_egress: "public", package_domains: true}});
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const page = await Page({
      params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID}),
    });
    const workspace = Children.toArray(page.props.children).find(child =>
      isValidElement(child) && child.type === FleetWorkspace);
    expect(workspace).toEqual(expect.objectContaining({props: expect.objectContaining({
      workspace: "ws_1", fleet: expect.objectContaining({id: FLEET_ID, model_ref: model, http_egress: "public", package_domains: true}),
    })}));
    expect(chatProps(page).modelLabel).toBe(model);
  });

  it("reads the repositories a run may reach from the approved GitHub grant, and from no pending one", async () => {
    authMock.mockResolvedValue({getToken: vi.fn().mockResolvedValue("token_abc"), sessionClaims: {sub: "fleet-reader", scopes: "fleet:read grant:read"}});
    const scope = {repositories: ["org/app"], access: "write"};
    mockFetchBilling(happyBilling, {fleet: {grants: [{id: GRANT_ID, kind: "integration", name: "github", status: "approved", scope}]}});
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    expect(chatProps(await Page({params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID})})).sources)
      .toEqual({repositories: ["org/app"], access: "write", publication: null});
    mockFetchBilling(happyBilling, {fleet: {grants: [{id: GRANT_ID, kind: "integration", name: "github", status: "pending", requested_scope: scope}]}});
    expect(chatProps(await Page({params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID})})).sources)
      .toEqual({repositories: [], access: null, publication: null});
  });

  it.each([
    ["fleet:read", false, false],
    ["fleet:read grant:read", false, true],
    ["fleet:read schedule:read", true, false],
    ["fleet:write schedule:write grant:write", true, true],
    ["fleet:read \tschedule:read \tgrant:read", false, false],
  ] as const)("sends only the lists %s can read to the chat, header and workspace", async (scopes, schedules, grants) => {
    authMock.mockResolvedValue({getToken: vi.fn().mockResolvedValue("token_abc"), sessionClaims: {sub: "fleet-reader", scopes}});
    const repo = "org/restricted-repository";
    const scope = {repositories: [repo], access: "write"};
    const trigger = {id: TRIGGER_ID, kind: "webhook", source: "readable-webhook", created_at: "2026-11-24T10:00:00Z"};
    mockFetchBilling(happyBilling, {fleet: {schedules: [SCHEDULE], grants: [{id: GRANT_ID, kind: "integration", name: "github", status: "approved", scope}], triggers: [trigger]}});
    const {default: Page} = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const page = await Page({params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID})});
    const payload = JSON.stringify(page);
    expect(payload.includes(SCHEDULE.message)).toBe(schedules);
    expect(payload.includes(repo)).toBe(grants);
    expect(payload).toContain(trigger.source);
    expect(chatProps(page).schedules).toEqual(schedules ? [SCHEDULE] : []);
    expect(chatProps(page).sources?.repositories).toEqual(grants ? [repo] : []);
  });

  it("shows one fleet title with the archived state", async () => {
    mockFetchBilling(happyBilling, {fleet: {status: "archived"}});
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
    expect(markup).toContain(FLEET_TITLE_MARKUP);
    expect(markup).not.toContain(`>agt-${FLEET_ID}</p>`);
    expect(markup).not.toContain(">archived</p>");
    expect(markup).not.toContain("data-live");
  });

  it("keeps the fleet title to the fleet's own name", async () => {
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const markup = renderToStaticMarkup(createElement(TooltipProvider, null, await Page({
      params: Promise.resolve({ workspaceId: "ws_1", id: FLEET_ID }),
    })));
    expect(markup).toContain(FLEET_TITLE_MARKUP);
    expect(markup).not.toContain(`>agt-${FLEET_ID}</p>`);
  });

  // The transcript is the one surface that wants the bodies, and the list read
  // no longer carries them — so each row is re-read as a detail. A detail that
  // fails must degrade to its list row: the turn keeps its header and outcome
  // and the page still renders. Without that arm the rejection propagates out
  // of Promise.all and the whole fleet page dies on one bad turn.
  it("fleet transcript survives a turn whose detail re-read fails", async () => {
    const listRow = {
      turn_id: "ev_1",
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
      if (/\/turns\/[^/?]+/.test(url)) return response({detail: "detail read failed"}, 500);
      if (url.includes("/turns")) return response({items: [listRow], next_cursor: null});
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
    expect(markup).toContain(detailBody().name);
  });

  it("fleet summary links an exact pending count to the filtered Approvals inbox", async () => {
    const respond = mockFetchBilling(happyBilling, {fleet: {pending_approvals: 1}});
    fetchMock.mockImplementation(async (url: string) => {
      if (url.includes("/approvals")) throw new Error("the chat never reads the inbox to render its count");
      return respond(url);
    });
    const { default: Page } =
      await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
    const props = chatProps(await Page({params: Promise.resolve({workspaceId: "ws_1", id: FLEET_ID})}));
    expect(props.initialSummary.pendingApprovals).toBe(1);
    expect(props.approvalsHref).toBe(`/w/ws_1/fleets/${FLEET_ID}#fleet-approvals`);
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
