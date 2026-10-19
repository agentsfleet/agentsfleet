import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { DRAFT, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { DraftSearchResultsDynamic } from "./DraftSearchResultsDynamic";

type SearchModule = typeof import("./DraftSearchResults");
vi.mock("next/navigation", () => ({useSearchParams: () => new URLSearchParams(), useRouter: () => ({replace: vi.fn()})}));
afterEach(() => {cleanup(); vi.doUnmock("./DraftSearchResults"); vi.restoreAllMocks();});

it("retries a failed module import and runs the real search reader after the chunk loads", async () => {
  const chunk = Promise.withResolvers<SearchModule>();
  const importing = Promise.withResolvers<void>();
  // Fail the module-import boundary; retain the real loader, subscription and retry behavior.
  vi.doMock("./DraftSearchResults", () => {importing.resolve(); return chunk.promise;});
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(JSON.stringify({items: [], next_cursor: null})));
  const props = {query: "failures", onNavigate: vi.fn(), history: {workspace: WORKSPACE, subject: "reader", cards: [],
    fresh: null, remove: null, owner: null, locked: false, current: DRAFT.id, more: null, status: "ready" as const, searchVersion: null}};
  render(<DraftSearchResultsDynamic {...props} />);
  expect(screen.getByRole("status").textContent).toBe("Searching chats…");
  await importing.promise;
  chunk.reject(new Error("Chunk unavailable"));
  expect(await screen.findByText("Couldn’t load chat search.")).toBeTruthy();
  expect(fetcher).not.toHaveBeenCalled();
  vi.doUnmock("./DraftSearchResults");
  fireEvent.click(screen.getByRole("button", {name: "Retry search"}));
  expect(await screen.findByText("No matching chats")).toBeTruthy();
  expect(fetcher).toHaveBeenCalledOnce();
});
