import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MODEL } from "../helpers/fleet-fixtures";

const {credential, workspaces, chat, request} = vi.hoisted(() => ({credential: vi.fn(), workspaces: vi.fn(), chat: vi.fn(), request: vi.fn()}));
vi.mock("@/lib/auth/platform", () => ({hasScope: () => Promise.resolve(false)}));
vi.mock("@/lib/auth/credential", () => ({requireCredential: credential}));
vi.mock("@/lib/workspace", () => ({listTenantWorkspacesCached: workspaces}));
vi.mock("@/lib/api/client", () => ({request}));
vi.mock("next/navigation", () => ({notFound: () => { throw new Error("not-found"); }}));
vi.mock("@/components/domain/fleet/NewFleetChat", () => ({NewFleetChat: chat}));
import Page from "../../app/(dashboard)/w/[workspaceId]/fleets/new/page";

const WORKSPACE = "0199a801-1000-7000-8000-000000000001";

beforeEach(() => {
  vi.resetAllMocks();
  credential.mockResolvedValue("synthetic-session");
  workspaces.mockResolvedValue({items: [{id: WORKSPACE, account: {tenant_id: "account-owner"}}]});
  request.mockResolvedValue({items: [MODEL], next_cursor: null});
  chat.mockReturnValue(<p>New fleet composer</p>);
});

function page(workspace = WORKSPACE) {
  return Page({params: Promise.resolve({workspaceId: workspace})});
}

describe("New fleet", () => {
  it("authenticates before reading the workspace or its models", async () => {
    credential.mockRejectedValue(new Error("sign-in"));
    await expect(page()).rejects.toThrow("sign-in");
    expect(workspaces).not.toHaveBeenCalled(); expect(request).not.toHaveBeenCalled(); expect(chat).not.toHaveBeenCalled();
  });
  it("renders the composer for the workspace with the models it can run, and creates nothing", async () => {
    expect(renderToStaticMarkup(await page())).toContain("New fleet composer");
    expect(chat.mock.calls[0]?.[0]).toEqual({workspace: WORKSPACE, models: [MODEL]});
    expect(request).toHaveBeenCalledExactlyOnceWith(`/v1/workspaces/${WORKSPACE}/models`, {method: "GET"}, "synthetic-session");
    expect(workspaces).toHaveBeenCalledWith("synthetic-session");
  });
  it("offers no models when the catalogue cannot be read, so the first message uses the workspace's model", async () => {
    request.mockRejectedValue(new Error("catalogue unavailable"));
    renderToStaticMarkup(await page());
    expect(chat.mock.calls[0]?.[0]).toEqual({workspace: WORKSPACE, models: []});
  });
  it.each(["foreign", ".."])("hides an unowned workspace %s", async (workspace) => {
    await expect(page(workspace)).rejects.toThrow("not-found");
    expect(chat).not.toHaveBeenCalled();
  });
  it("preserves dependency failure without guessing a workspace", async () => {
    workspaces.mockRejectedValue(new Error("workspace dependency unavailable"));
    await expect(page()).rejects.toThrow("workspace dependency unavailable");
    expect(chat).not.toHaveBeenCalled();
  });
});
