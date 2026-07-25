import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook } from "@testing-library/react";

const { routerPushMock, searchParamsRef } = vi.hoisted(() => ({
  routerPushMock: vi.fn(),
  searchParamsRef: { current: new URLSearchParams() },
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: routerPushMock }),
  usePathname: () => "/w/ws_1/events",
  useSearchParams: () => searchParamsRef.current,
}));

import { useUrlCursorPages } from "./use-url-cursor-pages";

afterEach(() => {
  cleanup();
  routerPushMock.mockReset();
  searchParamsRef.current = new URLSearchParams();
});

describe("useUrlCursorPages", () => {
  it("reads the page number and hasNext from the URL trail", () => {
    searchParamsRef.current = new URLSearchParams("c=tok_1&cps=25");
    const { result } = renderHook(() => useUrlCursorPages("tok_2", 25));
    expect(result.current.page).toBe(2);
    expect(result.current.hasNext).toBe(true);

    const { result: last } = renderHook(() => useUrlCursorPages(null, 25));
    expect(last.current.hasNext).toBe(false);
  });

  it("appends the next cursor when stepping forward one page", () => {
    const { result } = renderHook(() => useUrlCursorPages("tok_next", 25));
    result.current.goToPage(2);
    expect(routerPushMock).toHaveBeenCalledTimes(1);
    expect(String(routerPushMock.mock.calls[0]?.[0])).toContain("c=tok_next");
    expect(String(routerPushMock.mock.calls[0]?.[0])).toContain("cps=25");
  });

  it("drops the last cursor when stepping back one page", () => {
    searchParamsRef.current = new URLSearchParams("c=tok_1&c=tok_2&cps=25");
    const { result } = renderHook(() => useUrlCursorPages(null, 25));
    result.current.goToPage(2); // from page 3 back to page 2
    expect(routerPushMock).toHaveBeenCalledTimes(1);
    // The trail drops tok_2 and keeps tok_1.
    const pushed = new URLSearchParams(
      String(routerPushMock.mock.calls[0]?.[0]).split("?")[1],
    );
    expect(pushed.getAll("c")).toEqual(["tok_1"]);
  });

  it("ignores a stale click against a page that has already moved", () => {
    // The pager only ever offers one step either way. A target two pages
    // ahead (or the current page) is a stale click and must be a no-op, not a
    // navigation to a page whose cursor we do not hold.
    const { result } = renderHook(() => useUrlCursorPages("tok_next", 25));
    result.current.goToPage(5); // from page 1, way out of range
    result.current.goToPage(1); // the page we are already on
    expect(routerPushMock).not.toHaveBeenCalled();
  });

  it("should reset the cursor trail and preserve unrelated query state when page size changes", () => {
    searchParamsRef.current = new URLSearchParams(
      "view=events&c=tok_1&c=tok_2&cps=25",
    );
    const { result } = renderHook(() => useUrlCursorPages("tok_next", 25));

    result.current.changePageSize(50);

    const pushed = new URLSearchParams(
      String(routerPushMock.mock.calls[0]?.[0]).split("?")[1],
    );
    expect(pushed.get("view")).toBe("events");
    expect(pushed.get("ps")).toBe("50");
    expect(pushed.getAll("c")).toEqual([]);
    expect(pushed.has("cps")).toBe(false);
  });

  it("should canonicalize the default page size by removing its query value", () => {
    searchParamsRef.current = new URLSearchParams(
      "view=events&ps=100&c=tok_1&cps=100",
    );
    const { result } = renderHook(() => useUrlCursorPages("tok_next", 100));

    result.current.changePageSize(25);

    const pushed = new URLSearchParams(
      String(routerPushMock.mock.calls[0]?.[0]).split("?")[1],
    );
    expect(pushed.get("view")).toBe("events");
    expect(pushed.has("ps")).toBe(false);
    expect(pushed.has("c")).toBe(false);
    expect(pushed.has("cps")).toBe(false);
  });

  it("should return to the query-free path when page size is the only state", () => {
    searchParamsRef.current = new URLSearchParams("ps=100");
    const { result } = renderHook(() => useUrlCursorPages("tok_next", 100));

    result.current.changePageSize(25);

    expect(routerPushMock).toHaveBeenCalledWith("/w/ws_1/events", {
      scroll: true,
    });
  });

  it("should ignore unsupported page sizes without navigating", () => {
    const { result } = renderHook(() => useUrlCursorPages("tok_next", 25));

    result.current.changePageSize(26);

    expect(routerPushMock).not.toHaveBeenCalled();
  });

  it("resets to page one when the cursor row count does not match", () => {
    searchParamsRef.current = new URLSearchParams("ps=100&c=tok_1&cps=25");
    const { result } = renderHook(() => useUrlCursorPages("tok_next", 100));

    expect(result.current.page).toBe(1);
    result.current.goToPage(2);

    const pushed = new URLSearchParams(
      String(routerPushMock.mock.calls[0]?.[0]).split("?")[1],
    );
    expect(pushed.getAll("c")).toEqual(["tok_next"]);
    expect(pushed.get("cps")).toBe("100");
  });
});
