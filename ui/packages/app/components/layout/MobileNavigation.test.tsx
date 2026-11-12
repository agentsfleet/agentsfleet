import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { TooltipProvider } from "@agentsfleet/design-system";
import { MobileNavigation } from "./MobileNavigation";

vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({userId: "reviewer", isLoaded: true})}));
vi.mock("@/lib/analytics/posthog", () => ({trackNavigationClicked: vi.fn()}));
vi.mock("./DraftSidebarHistory", () => ({DraftSidebarHistory: ({account, workspace}: {account: string | null; workspace: string | null}) => <p>Recent chats: {account} / {workspace}</p>}));
afterEach(cleanup);

it("opens scoped navigation in the shared dialog, closes on Escape and on route selection", async () => {
  render(<TooltipProvider><MobileNavigation pathname="/w/workspace/fleets" workspaceId="workspace" account="account" /></TooltipProvider>);
  const trigger = screen.getByRole("button", {name: "Open navigation"});
  await act(async () => {fireEvent.click(trigger);});
  expect(screen.getByRole("dialog", {name: "Navigation"})).toBeTruthy();
  expect(screen.getByText("Recent chats: account / workspace")).toBeTruthy();
  fireEvent.keyDown(document.activeElement!, {key: "Escape"});
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(document.activeElement).toBe(trigger);
  await act(async () => {fireEvent.click(trigger);});
  fireEvent.click(screen.getByRole("link", {name: "Plugins"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});
