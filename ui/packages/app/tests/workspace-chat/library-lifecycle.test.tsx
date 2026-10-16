import {afterEach, expect, it, vi} from "vitest";
import {act, cleanup, fireEvent, render, screen} from "@testing-library/react";
import {flushSync} from "react-dom";
import {LibraryBrowser} from "@/components/domain/chat/LibraryBrowser";
import {LIBRARY, LIBRARY_CARD, WORKSPACE, response} from "@/tests/helpers/draft-fixtures";

const HISTORY = {items: [], next_cursor: null};
const CURRENT_NAME = "Current workspace recipe";
const COMPLETION_INTERLEAVINGS = 16;
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it.each(["list", "inspection", "history"])("keeps the selected collection when an older %s finishes during navigation", async (target) => {
  // Exercise navigation before, during and after response decoding. A fetch can
  // finish just before cleanup while its consumer callback is still queued.
  for (let delay = 0; delay < COMPLETION_INTERLEAVINGS; delay += 1) {
    const pending = Promise.withResolvers<Response>();
    const onSelect = vi.fn();
    vi.stubGlobal("fetch", vi.fn((input: string) => {
      const path = new URL(input, "https://app.test").pathname;
      if (path.endsWith("/owned")) return Promise.resolve(response({items: [{...LIBRARY_CARD, name: CURRENT_NAME}], next_cursor: null}));
      if (path.endsWith("/fleet-libraries")) return target === "list" ? pending.promise
        : Promise.resolve(response({items: [LIBRARY_CARD], next_cursor: null}));
      if (path.endsWith("/revisions")) return target === "history" ? pending.promise : Promise.resolve(response(HISTORY));
      return target === "inspection" ? pending.promise : Promise.resolve(response(LIBRARY));
    }));
    const view = render(<LibraryBrowser workspace={WORKSPACE} onSelect={onSelect} />);
    if (target !== "list") fireEvent.click(await screen.findByRole("button", {name: "Inspect"}));
    await act(async () => {
      pending.resolve(response(target === "list" ? {items: [LIBRARY_CARD], next_cursor: null} : target === "history" ? HISTORY : LIBRARY));
      for (let step = 0; step < delay; step += 1) await Promise.resolve();
      flushSync(() => {fireEvent.click(screen.getByRole("button", {name: "Workspace recipes"}));});
    });
    expect(await screen.findByText(CURRENT_NAME)).toBeTruthy();
    expect(screen.queryByText(LIBRARY_CARD.name)).toBeNull();
    expect(screen.queryByRole("button", {name: "Start chat from this revision"})).toBeNull();
    expect(screen.queryByLabelText("Published revisions")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(onSelect).not.toHaveBeenCalled();
    view.unmount();
  }
});
