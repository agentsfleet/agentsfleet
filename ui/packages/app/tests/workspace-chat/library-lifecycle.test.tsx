import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {act, cleanup, fireEvent, render, screen} from "@testing-library/react";
import {flushSync} from "react-dom";
import {LibraryBrowser} from "@/components/domain/chat/LibraryBrowser";
import {LIBRARY, LIBRARY_CARD, WORKSPACE, response} from "@/tests/helpers/draft-fixtures";

const HISTORY = {items: [], next_cursor: null};
const NEXT_WORKSPACE = "019a1e5a-fd20-7000-8000-000000000099";
const CURRENT_NAME = "Current workspace recipe";
const COMPLETION_INTERLEAVINGS = 16;
const identity = vi.hoisted(() => ({isLoaded: true, isSignedIn: true, userId: "plugin-owner"}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => identity}));
beforeEach(() => {identity.isSignedIn = true; identity.userId = "plugin-owner";});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it.each(["another-user", "signed-out"])("discards loaded private plugins and pending inspection on %s", async next => {
  const pending = Promise.withResolvers<Response>();
  const fetcher = vi.fn().mockResolvedValueOnce(response({items: [LIBRARY_CARD], next_cursor: null}))
    .mockReturnValueOnce(pending.promise).mockResolvedValue(response({items: [], next_cursor: null}));
  vi.stubGlobal("fetch", fetcher);
  const onSelect = vi.fn();
  const view = render(<LibraryBrowser workspace={WORKSPACE} onSelect={onSelect} />);
  fireEvent.click(await screen.findByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
  identity.userId = next; identity.isSignedIn = next !== "signed-out";
  view.rerender(<LibraryBrowser workspace={WORKSPACE} onSelect={onSelect} />);
  expect(screen.queryByText(LIBRARY_CARD.name)).toBeNull();
  expect(fetcher.mock.calls[1]?.[1].signal.aborted).toBe(true);
  await act(async () => pending.resolve(response(LIBRARY)));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(onSelect).not.toHaveBeenCalled();
});

it.each(["list", "inspection", "history"])("keeps the selected collection when an older %s finishes during navigation", async (target) => {
  // Exercise navigation before, during and after response decoding. A fetch can
  // finish just before cleanup while its consumer callback is still queued.
  for (let delay = 0; delay < COMPLETION_INTERLEAVINGS; delay += 1) {
    const pending = Promise.withResolvers<Response>();
    const onSelect = vi.fn();
    vi.stubGlobal("fetch", vi.fn((input: string) => {
      const path = new URL(input, "https://app.test").pathname;
      if (path.includes(NEXT_WORKSPACE)) return Promise.resolve(response({items: [{...LIBRARY_CARD, name: CURRENT_NAME}], next_cursor: null}));
      if (path.endsWith("/owned")) return target === "list" ? pending.promise
        : Promise.resolve(response({items: [LIBRARY_CARD], next_cursor: null}));
      if (path.endsWith("/revisions")) return target === "history" ? pending.promise : Promise.resolve(response(HISTORY));
      return target === "inspection" ? pending.promise : Promise.resolve(response(LIBRARY));
    }));
    const view = render(<LibraryBrowser workspace={WORKSPACE} onSelect={onSelect} />);
    if (target !== "list") fireEvent.click(await screen.findByRole("button", {name: `Manage ${LIBRARY_CARD.name}`}));
    if (target === "history") fireEvent.click(screen.getByRole("button", {name: "Choose a saved version"}));
    await act(async () => {
      pending.resolve(response(target === "list" ? {items: [LIBRARY_CARD], next_cursor: null} : target === "history" ? HISTORY : LIBRARY));
      for (let step = 0; step < delay; step += 1) await Promise.resolve();
      flushSync(() => {view.rerender(<LibraryBrowser workspace={NEXT_WORKSPACE} onSelect={onSelect} />);});
    });
    expect(await screen.findByText(CURRENT_NAME)).toBeTruthy();
    expect(screen.queryByText(LIBRARY_CARD.name)).toBeNull();
    expect(screen.queryByRole("button", {name: "Use for new fleet"})).toBeNull();
    expect(screen.queryByRole("button", {name: "Edit setup"})).toBeNull();
    expect(screen.queryByLabelText("Published revisions")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(onSelect).not.toHaveBeenCalled();
    view.unmount();
  }
});
