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
    searchParamsRef.current = new URLSearchParams("c=tok_1");
    const { result } = renderHook(() => useUrlCursorPages("tok_2"));
    expect(result.current.page).toBe(2);
    expect(result.current.hasNext).toBe(true);

    const { result: last } = renderHook(() => useUrlCursorPages(null));
    expect(last.current.hasNext).toBe(false);
  });

  it("appends the next cursor when stepping forward one page", () => {
    const { result } = renderHook(() => useUrlCursorPages("tok_next"));
    result.current.goToPage(2);
    expect(routerPushMock).toHaveBeenCalledTimes(1);
    expect(String(routerPushMock.mock.calls[0]?.[0])).toContain("c=tok_next");
  });

  it("drops the last cursor when stepping back one page", () => {
    searchParamsRef.current = new URLSearchParams("c=tok_1&c=tok_2");
    const { result } = renderHook(() => useUrlCursorPages(null));
    result.current.goToPage(2); // from page 3 back to page 2
    expect(routerPushMock).toHaveBeenCalledTimes(1);
    // The trail drops tok_2 and keeps tok_1.
    const pushed = new URLSearchParams(String(routerPushMock.mock.calls[0]?.[0]).split("?")[1]);
    expect(pushed.getAll("c")).toEqual(["tok_1"]);
  });

  it("ignores a stale click against a page that has already moved", () => {
    // The pager only ever offers one step either way. A target two pages
    // ahead (or the current page) is a stale click and must be a no-op, not a
    // navigation to a page whose cursor we do not hold.
    const { result } = renderHook(() => useUrlCursorPages("tok_next"));
    result.current.goToPage(5); // from page 1, way out of range
    result.current.goToPage(1); // the page we are already on
    expect(routerPushMock).not.toHaveBeenCalled();
  });
});
