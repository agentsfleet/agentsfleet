import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

const {credential, workspaces, chat} = vi.hoisted(() => ({credential: vi.fn(), workspaces: vi.fn(), chat: vi.fn()}));
vi.mock("@/lib/auth/platform", () => ({hasScope: () => Promise.resolve(false)}));
vi.mock("@/lib/auth/credential", () => ({requireCredential: credential}));
vi.mock("@/lib/workspace", () => ({listTenantWorkspacesCached: workspaces}));
vi.mock("next/navigation", () => ({notFound: () => { throw new Error("not-found"); }}));
vi.mock("@/components/domain/chat/WorkspaceChat", () => ({WorkspaceChat: chat}));
import Page from "../../app/(dashboard)/w/[workspaceId]/fleets/new/page";

const WORKSPACE = "0199a801-1000-7000-8000-000000000001";
const DRAFT = "0199a801-1000-7000-8000-000000000002";
const ACCOUNT = "account-owner";

beforeEach(() => {
  vi.resetAllMocks();
  credential.mockResolvedValue("synthetic-session");
  workspaces.mockResolvedValue({items: [{id: WORKSPACE, account: {tenant_id: ACCOUNT}}]});
  chat.mockReturnValue(<p>Private chat</p>);
});

function page(query: Record<string, string | string[] | undefined> = {}, workspace = WORKSPACE) {
  return Page({params: Promise.resolve({workspaceId: workspace}), searchParams: Promise.resolve(query)});
}

describe("native fleet chat entry", () => {
  it("authenticates before resolving ownership or rendering private chat", async () => {
    credential.mockRejectedValue(new Error("sign-in"));
    await expect(page()).rejects.toThrow("sign-in");
    expect(workspaces).not.toHaveBeenCalled(); expect(chat).not.toHaveBeenCalled();
  });
  it("binds the owning account and workspace without allocating a draft", async () => {
    const markup = renderToStaticMarkup(await page());
    expect(markup).toContain("Private chat");
    expect(chat.mock.calls[0]?.[0]).toMatchObject({account: ACCOUNT, workspace: WORKSPACE, draft: undefined});
    expect(workspaces).toHaveBeenCalledWith("synthetic-session");
  });
  it("passes a validated private draft selection without reading its text on the page", async () => {
    renderToStaticMarkup(await page({draft: DRAFT}));
    expect(chat.mock.calls[0]?.[0]).toMatchObject({draft: DRAFT});
  });
  it("starts from the selected published revision, including named platform recipes", async () => {
    renderToStaticMarkup(await page({library: "checkout-watch", revision: DRAFT}));
    expect(chat.mock.calls[0]?.[0]).toMatchObject({source: {kind: "library", library_id: "checkout-watch", revision_id: DRAFT}});
  });
  it("pins improvement to the selected fleet and setup revision", async () => {
    renderToStaticMarkup(await page({fleet: DRAFT, setup_revision: "2"}));
    expect(chat.mock.calls[0]?.[0]).toMatchObject({source: {kind: "fleet", fleet_id: DRAFT, setup_revision: "2"}});
  });
  it.each([{library: "checkout-watch"}, {library: "", revision: DRAFT}, {library: "checkout-watch", revision: "bad"}, {library: "checkout-watch", revision: DRAFT, draft: DRAFT}])("refuses incomplete or mixed library selections", async (query) => {
    await expect(page(query)).rejects.toThrow("not-found");
    expect(chat).not.toHaveBeenCalled();
  });
  it.each(["foreign", ".."])("hides an unowned workspace %s", async (workspace) => {
    await expect(page({}, workspace)).rejects.toThrow("not-found");
    expect(chat).not.toHaveBeenCalled();
  });
  it.each([{draft: ".."}, {draft: [DRAFT, DRAFT]}])("refuses malformed or repeated draft selections", async (query) => {
    await expect(page(query)).rejects.toThrow("not-found"); expect(chat).not.toHaveBeenCalled();
  });
  it("preserves dependency failure without guessing an account", async () => {
    workspaces.mockRejectedValue(new Error("workspace dependency unavailable"));
    await expect(page()).rejects.toThrow("workspace dependency unavailable");
    expect(chat).not.toHaveBeenCalled();
  });
});
