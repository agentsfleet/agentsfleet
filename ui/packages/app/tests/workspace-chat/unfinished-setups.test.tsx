import type { DraftSidebarState } from "@/components/layout/draft-sidebar-state";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { UnfinishedSetups } from "@/components/domain/chat/UnfinishedSetups";
import { HistoryContext } from "@/components/layout/draft-sidebar-state";
import { DRAFT_LIST_STATUS } from "@/lib/api/draft-list-types";
import { CHAT_ACTIVITY } from "@/lib/api/draft-summary";
import { DRAFT, WORKSPACE } from "@/tests/helpers/draft-fixtures";

const SUBJECT = "unfinished-reader";
const RETRY = "Try again";
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({userId: "unfinished-reader"})}));
vi.mock("next/navigation", () => ({useRouter: () => ({replace: vi.fn()}), useSearchParams: () => new URLSearchParams()}));
afterEach(cleanup);

const history: DraftSidebarState = {workspace: WORKSPACE, subject: SUBJECT,
  cards: [{id: DRAFT.id, title: DRAFT.title, detail: "In progress", activity: CHAT_ACTIVITY.draft}],
  fresh: null, remove: null, owner: null, locked: false, current: null, more: null, status: DRAFT_LIST_STATUS.unavailable};

it("preserves loaded unfinished setups when their retry fails and keeps navigation usable", async () => {
  const reload = vi.fn().mockRejectedValue(new Error("history unavailable"));
  render(<HistoryContext value={history}><UnfinishedSetups workspace={WORKSPACE} onReload={reload} /></HistoryContext>);
  const link = screen.getByRole("link", {name: DRAFT.title});
  fireEvent.click(screen.getByRole("button", {name: RETRY}));
  await waitFor(() => expect(reload).toHaveBeenCalledOnce());
  expect(screen.getByRole("link", {name: DRAFT.title})).toBe(link);
  expect(screen.getByText(/Unfinished setups could not be loaded/)).toBeTruthy();
  fireEvent.click(link);
  expect(link.getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/new?draft=${DRAFT.id}`);
  expect(screen.getByRole("button", {name: RETRY})).toHaveProperty("disabled", false);
});

it("does not display private unfinished setups for a different workspace or person", () => {
  const reload = vi.fn();
  const rendered = render(<HistoryContext value={{...history, workspace: "another-workspace"}}>
    <UnfinishedSetups workspace={WORKSPACE} onReload={reload} /></HistoryContext>);
  expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull();
  rendered.rerender(<HistoryContext value={{...history, subject: "another-reader"}}>
    <UnfinishedSetups workspace={WORKSPACE} onReload={reload} /></HistoryContext>);
  expect(screen.queryByRole("link", {name: DRAFT.title})).toBeNull();
  expect(screen.queryByRole("button", {name: RETRY})).toBeNull();
  expect(reload).not.toHaveBeenCalled();
});
