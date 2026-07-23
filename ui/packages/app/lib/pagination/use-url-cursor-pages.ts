"use client";

import { useCallback, useTransition } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";

import { CURSOR_TRAIL_PARAM } from "./cursor-trail";

// The client half of URL-held paging: the pager that WRITES the cursor trail.
// The server-safe readers live in `cursor-trail.ts` (see the note there on why
// the split is load-bearing rather than tidiness).
//
// Moving paging into the URL also moves the fetch back to the server: the
// Server Component reads the cursor and renders that page, so no rows travel
// through a Server Action into a client cache and nothing is re-fetched after
// a reload.
//
// The trail is the list of cursors walked to reach this page, repeated under
// one key (`?c=…&c=…`). It exists so Previous is a step the client can take on
// its own — a keyset feed only walks forwards, so the way back has to be
// remembered rather than requested.

export type UrlCursorPages = {
  /** 1-based, matching the pager's own numbering. */
  page: number;
  hasNext: boolean;
  /** True while the next page is being fetched on the server. */
  isLoading: boolean;
  goToPage: (page: number) => void;
};

export function useUrlCursorPages(nextCursor: string | null): UrlCursorPages {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const [isLoading, startTransition] = useTransition();

  const trail = params.getAll(CURSOR_TRAIL_PARAM).filter((entry) => entry.length > 0);
  const page = trail.length + 1;

  const goToPage = useCallback(
    (target: number) => {
      const current = params.getAll(CURSOR_TRAIL_PARAM).filter((entry) => entry.length > 0);
      // Rebuilt from the live params so every OTHER query value — the fleet
      // page's open tab, most of all — survives a page turn untouched.
      const next = new URLSearchParams(params.toString());
      next.delete(CURSOR_TRAIL_PARAM);

      let trailNext: string[];
      if (target === current.length + 2 && nextCursor) trailNext = [...current, nextCursor];
      else if (target === current.length && current.length > 0) trailNext = current.slice(0, -1);
      // The pager only ever offers one step either way; anything else is a
      // stale click against a page that has already moved.
      else return;

      for (const cursor of trailNext) next.append(CURSOR_TRAIL_PARAM, cursor);
      const query = next.toString();
      // A transition, so the pager can say it is working while the server
      // renders the next page instead of the surface simply freezing.
      startTransition(() => {
        router.push(query.length > 0 ? `${pathname}?${query}` : pathname, { scroll: true });
      });
    },
    [nextCursor, params, pathname, router],
  );

  return { page, hasNext: nextCursor !== null, isLoading, goToPage };
}
