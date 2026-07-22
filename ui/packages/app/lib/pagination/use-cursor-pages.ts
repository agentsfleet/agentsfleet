"use client";

import { useCallback, useRef, useState, useTransition } from "react";

// Paging for a keyset (cursor) feed, presented as numbered pages.
//
// A cursor feed cannot count itself, so the obvious control is an append-style
// "load more". That control is the problem: the list grows without bound, the
// operator scrolls further on every press, and the button walks off the bottom
// of the screen. Every other table in the console pages, so these did too.
//
// Each page is fetched once and kept, so Previous is instant and never re-asks
// the server for a page it already showed. Only forward motion past the last
// fetched page costs a request.

/** One fetched page of a keyset feed — the shape every list endpoint returns. */
export type CursorPage<T> = {
  items: T[];
  next_cursor: string | null;
};

export type FetchPageResult<T> =
  | { ok: true; data: CursorPage<T> }
  | { ok: false; error: string; errorCode?: string };

export type CursorPages<T> = {
  /** The rows of the page currently on screen — never an accumulation. */
  items: T[];
  /** 1-based, for the pager label. */
  page: number;
  /** False once the held cursor is exhausted, so Next stops at the end. */
  hasNext: boolean;
  isLoading: boolean;
  error: string | null;
  /**
   * 1-based target, matching the pager's own numbering. Referentially stable
   * for the component's lifetime, so paging does not hand the table a new
   * callback and force it to rebuild its row model (the React Compiler is off
   * in `next.config.ts`, so this has to be earned rather than assumed).
   */
  goToPage: (page: number) => void;
};

type Cache<T> = { pages: CursorPage<T>[]; index: number };

export function useCursorPages<T>(
  initial: CursorPage<T>,
  fetchPage: (cursor: string) => Promise<FetchPageResult<T>>,
  presentError: (result: { error: string; errorCode?: string }) => string,
): CursorPages<T> {
  const [cache, setCache] = useState<Cache<T>>({ pages: [initial], index: 0 });
  const [error, setError] = useState<string | null>(null);
  const [isLoading, startTransition] = useTransition();

  // The callback reads the cache through a ref so it does not have to list it
  // as a dependency. Listing it would rebuild `goToPage` on every page turn —
  // the one moment the table is already re-rendering.
  const cacheRef = useRef(cache);
  cacheRef.current = cache;

  const goToPage = useCallback(
    (target: number) => {
      const { pages, index } = cacheRef.current;
      const nextIndex = target - 1;
      if (nextIndex < 0 || nextIndex === index) return;
      setError(null);
      // Already fetched — including every backward step — so show it now.
      if (nextIndex < pages.length) {
        setCache((prev) => ({ ...prev, index: nextIndex }));
        return;
      }
      // Only ever one page beyond what we hold, because the pager offers a
      // single step forward.
      const cursor = pages[pages.length - 1]?.next_cursor;
      if (nextIndex !== pages.length || !cursor) return;
      startTransition(async () => {
        const result = await fetchPage(cursor);
        if (!result.ok) {
          setError(presentError(result));
          return;
        }
        // Appended functionally, so a second click that lands while this one
        // is still in flight cannot drop the page it raced with. Advance only
        // after the page arrives, so a failed fetch leaves the operator on the
        // page they can still read.
        setCache((prev) => ({ pages: [...prev.pages, result.data], index: prev.pages.length }));
      });
    },
    [fetchPage, presentError],
  );

  const current = cache.pages[cache.index] ?? initial;
  const hasNext = cache.index < cache.pages.length - 1 || current.next_cursor !== null;
  return { items: current.items, page: cache.index + 1, hasNext, isLoading, error, goToPage };
}
