import { happyBilling, mockFetchBilling } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { authMock } from "../helpers/dashboard-mocks";

const VIEWER = "user_viewer";

// What the page hands the chat, captured instead of rendered: the unsent-message
// ledger is keyed by `viewer` before Clerk loads in the browser, so the page is
// the only place that can name the user on the first paint.
const chatViewProps = vi.hoisted(() => [] as { viewer: string | null }[]);
vi.mock("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView", () => ({
  ChatView: (props: { viewer: string | null }) => {
    chatViewProps.push(props);
    return null;
  },
}));

async function renderChat(): Promise<string | null | undefined> {
  mockFetchBilling(happyBilling);
  const { default: Page } = await import("../../app/(dashboard)/w/[workspaceId]/fleets/[id]/page");
  renderToStaticMarkup(await Page({ params: Promise.resolve({ workspaceId: "ws_1", id: "zom_1" }) }));
  return chatViewProps.at(-1)?.viewer;
}

describe("fleets routes — the chat's viewer", () => {
  it("hands the chat the user the request's verified claims name", async () => {
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_abc"), sessionClaims: { sub: VIEWER } });
    expect(await renderChat()).toBe(VIEWER);
  });

  it("hands the chat no user when the claims name none", async () => {
    authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_abc"), sessionClaims: { sub: 7 } });
    expect(await renderChat()).toBeNull();
  });
});
