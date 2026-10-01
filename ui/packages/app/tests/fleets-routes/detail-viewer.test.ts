import { happyBilling, mockFetchBilling } from "./harness";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { authMock } from "../helpers/dashboard-mocks";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces";
import type { WorkspaceMember } from "@/lib/api/tenant-members";
import type { SenderName } from "@/lib/events/sender-names";

const VIEWER = "user_viewer";
const WORKSPACE_ID = "ws_1";
const TOKEN = "token_abc";
const BOB_NAME = "Bob";
const BOB: WorkspaceMember = { user_id: "user_bob", display_name: BOB_NAME, role: ACCOUNT_ROLE.member, actor: "steer:user_bob" };
const NAMELESS: WorkspaceMember = { user_id: "user_anon", display_name: null, role: ACCOUNT_ROLE.member, actor: "steer:user_anon" };

type ChatViewProps = { viewer: string | null; senderNames: readonly SenderName[] };

// What the page hands the chat, captured instead of rendered: the unsent-message
// ledger is keyed by `viewer` before Clerk loads in the browser, so the page is
// the only place that can name the user on the first paint; `senderNames` is
// who the thread can name, read beside the thread itself.
const { chatViewProps, listWorkspaceMembersMock } = vi.hoisted(() => ({
  chatViewProps: [] as ChatViewProps[],
  listWorkspaceMembersMock: vi.fn(),
}));
vi.mock("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView", () => ({
  ChatView: (props: ChatViewProps) => {
    chatViewProps.push(props);
    return null;
  },
}));
vi.mock("@/lib/api/tenant-members", async (original) => ({
  ...(await original<Record<string, unknown>>()),
  listWorkspaceMembers: listWorkspaceMembersMock,
}));

beforeEach(() => {
  listWorkspaceMembersMock.mockResolvedValue([]);
  authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(TOKEN), sessionClaims: { sub: VIEWER } });
});

async function renderChat(): Promise<ChatViewProps | undefined> {
  mockFetchBilling(happyBilling);
  const { default: Page } = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
  renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID, id: "zom_1" }) }));
  return chatViewProps.at(-1);
}

describe("fleets routes — the chat's viewer", () => {
  it("hands the chat the user the request's verified claims name", async () => {
    expect((await renderChat())?.viewer).toBe(VIEWER);
  });

  it("hands the chat no user when the claims name none", async () => {
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(TOKEN), sessionClaims: { sub: 7 } });
    expect((await renderChat())?.viewer).toBeNull();
  });
});

describe("fleets routes — the chat's sender names", () => {
  it("should hand the chat each member with a display name when the workspace's members read", async () => {
    listWorkspaceMembersMock.mockResolvedValue([BOB, NAMELESS]);
    const props = await renderChat();
    expect(listWorkspaceMembersMock).toHaveBeenCalledExactlyOnceWith(WORKSPACE_ID, TOKEN);
    expect(props?.senderNames).toEqual([{ actor: BOB.actor, name: BOB_NAME }]);
  });

  it("should still render the chat, naming no one, when the members read fails", async () => {
    listWorkspaceMembersMock.mockRejectedValue(new Error("members unavailable"));
    const props = await renderChat();
    expect(props?.senderNames).toEqual([]);
    expect(props?.viewer).toBe(VIEWER);
  });
});
