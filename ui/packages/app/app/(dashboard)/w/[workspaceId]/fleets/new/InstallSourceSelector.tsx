"use client";

import { useState, useTransition } from "react";
import { Button, EmptyState, SectionLabel } from "@agentsfleet/design-system";
import { LayoutTemplateIcon } from "lucide-react";
import type { FleetLibraryPageResult } from "@/lib/api/fleet-library";
import { LIBRARY_AFTER_PARAM, LIBRARY_ERROR_KIND, readErrorFrom, type LibraryError } from "@/lib/api/library-types";
import type { FleetLibraryGalleryEntry } from "@/lib/types";
import AddLibraryDialog from "./AddLibraryDialog";
import { readFleetLibraryPageAction } from "./actions";
import {
  LibraryDocsLink,
  FLEET_LIBRARY_EMPTY_DESCRIPTION,
  FLEET_LIBRARY_EMPTY_DESCRIPTION_READONLY,
  FLEET_LIBRARY_EMPTY_TITLE,
} from "./library-docs";
import { LibraryCard } from "./LibraryCard";

type Props = {
  workspaceId: string;
  /** First gallery page, or null when the read failed — see `initialError`. */
  initialPage: FleetLibraryPageResult | null;
  /** Typed read failure. Distinct from an empty library, and never both. */
  initialError: LibraryError | null;
  /** A `library_id` was deep-linked and is not on the loaded page. */
  selectionNotFound?: boolean;
  onUseLibraryEntry: (entry: FleetLibraryGalleryEntry) => void;
  canAddLibraryEntry?: boolean;
  /** Open the add-library-entry dialog on first render (?create=1 deep link). */
  initialCreateOpen?: boolean;
};

const NOT_FOUND_COPY =
  "That library entry is not on this page. It may have been removed, or it may be further down the library.";

/**
 * Write the current list position into the URL.
 *
 * REPLACES rather than pushes: load-more is not a navigation, and pushing
 * would make Back walk the user backwards one page-load at a time instead of
 * leaving the screen. Uses `history` directly rather than the router so the
 * route does not re-render — the rows are already in state, and a re-render
 * would refetch the page we just appended.
 *
 * Exported for the same reason `galleryErrorCopy` is: reaching the no-window
 * arm through a render is impossible, because the only environment that lacks
 * `window` is also the one that cannot run the click that calls this.
 */
export function mirrorCursorIntoUrl(cursor: string | null) {
  if (typeof window === "undefined") return;
  const url = new URL(window.location.href);
  if (cursor === null) url.searchParams.delete(LIBRARY_AFTER_PARAM);
  else url.searchParams.set(LIBRARY_AFTER_PARAM, cursor);
  window.history.replaceState(window.history.state, "", url);
}

/**
 * Pure — user-facing copy for a typed gallery failure. Separate strings per
 * kind because "sign in", "ask for access" and "try again" are different
 * instructions, and an empty gallery communicated none of them.
 */
export function galleryErrorCopy(error: LibraryError): string {
  switch (error.kind) {
    case LIBRARY_ERROR_KIND.unauthenticated:
      return "Your session expired. Sign in to browse the fleet library.";
    case LIBRARY_ERROR_KIND.forbidden:
      return "You do not have access to this workspace's fleet library.";
    case LIBRARY_ERROR_KIND.unavailable:
      return "The fleet library is temporarily unavailable.";
    default:
      return "Could not load the fleet library.";
  }
}

// Library gallery picker: the workspace's library entries (platform ∪ tenant)
// are the install surface. Picking one proceeds inline to the live install
// states — there is no review page. Rendered plainly under the page header
// (same shape as the dashboard's first-run gallery) — the page
// title/description already frame it, so no wrapping panel and no side guide.
export function InstallSourceSelector({
  workspaceId,
  initialPage,
  initialError,
  selectionNotFound = false,
  onUseLibraryEntry,
  canAddLibraryEntry = false,
  initialCreateOpen = false,
}: Props) {
  const [pending, startTransition] = useTransition();
  const [entries, setEntries] = useState<FleetLibraryGalleryEntry[]>(initialPage?.items ?? []);
  // Invariant 5: the cursor and total say what has NOT been loaded. Both are
  // rendered rather than implied by whether a button happens to be present.
  const [nextCursor, setNextCursor] = useState<string | null>(initialPage?.next_cursor ?? null);
  const [total, setTotal] = useState<number | null>(initialPage?.total ?? null);
  const [error, setError] = useState<LibraryError | null>(initialError);

  // One page per request. `append` retains every card already loaded (the
  // load-more path); replace is the recovery path, refilling a gallery that
  // never got its first page. A failed page keeps whatever is on screen and
  // surfaces a typed fault — it never blanks the gallery.
  function fetchPage(cursor: string | null, append: boolean) {
    startTransition(async () => {
      // try/catch because the action ROUND-TRIP itself can reject (network
      // failure, deploy skew) — `withToken` only catches server-side, and an
      // uncaught rejection would escape the transition into a route with no
      // error boundary.
      try {
        const r = await readFleetLibraryPageAction(workspaceId, cursor);
        if (!r.ok) {
          setError(readErrorFrom(r));
          return;
        }
        setError(null);
        setEntries((prior) => (append ? [...prior, ...r.data.items] : r.data.items));
        // A cursor that does not advance means the server is re-serving the
        // same page; another click would append it again as duplicate cards.
        // The exhaustive walk this replaced threw on exactly this defect —
        // treat it as terminal instead.
        setNextCursor(r.data.next_cursor === cursor ? null : r.data.next_cursor);
        setTotal(r.data.total);
        // Mirror the page we just loaded FROM into the URL, so a reload, a
        // shared link, or a back navigation out of a detail view lands here
        // rather than dumping the user back at the first page.
        mirrorCursorIntoUrl(cursor);
      } catch (cause) {
        setError({
          kind: LIBRARY_ERROR_KIND.unknown,
          detail: cause instanceof Error ? cause.message : undefined,
        });
      }
    });
  }

  // Re-attempt whichever read failed. With a cursor in hand the fault was a
  // load-more, so retry that same page; with none it was the server-render's
  // first-page read, which has no client twin to re-run — so read page one.
  // Keying this off `loadMore` alone left Retry permanently disabled in
  // exactly the failed-first-read state it existed for.
  function retryFailedRead() {
    if (nextCursor !== null) {
      fetchPage(nextCursor, true);
      return;
    }
    fetchPage(null, false);
  }

  const showAddLibraryEntry = canAddLibraryEntry;
  // Cards on screen are worth rendering even mid-failure, so the empty state
  // fires only on a genuinely empty, genuinely successful read.
  const hasEntries = entries.length > 0;

  return (
    <div className="space-y-sm">
      <div className="flex flex-wrap items-baseline justify-between gap-md">
        <SectionLabel>Fleet library</SectionLabel>
        {showAddLibraryEntry && hasEntries ? (
          <AddLibraryDialog workspaceId={workspaceId} defaultOpen={initialCreateOpen} />
        ) : null}
      </div>

      {/* `<output>` carries an implicit status role, so it announces without
          the explicit attribute the a11y lint (correctly) rejects. */}
      {selectionNotFound ? (
        <output className="block text-sm text-muted-foreground">{NOT_FOUND_COPY}</output>
      ) : null}

      {hasEntries ? (
        <>
          <div className="grid grid-cols-1 gap-md sm:grid-cols-2 lg:grid-cols-3">
            {entries.map((entry) => (
              <LibraryCard
                key={`${entry.visibility}:${entry.id}`}
                entry={entry}
                action={
                  <Button type="button" onClick={() => onUseLibraryEntry(entry)}>
                    Use entry
                  </Button>
                }
              />
            ))}
          </div>

          {nextCursor !== null ? (
            <div className="flex items-center gap-3">
              {/*
                Append the next page — exactly one request per click, where the
                exhaustive walk this replaced issued as many as the library had
                pages on every visit. The cursor is read here rather than behind
                a null guard inside a handler: this branch is the only thing
                that renders the control, so a guard could never have fired.
              */}
              <Button type="button" variant="secondary" onClick={() => fetchPage(nextCursor, true)} disabled={pending}>
                {pending ? "Loading…" : "Load more"}
              </Button>
              <p className="text-sm text-muted-foreground" aria-live="polite">
                {total !== null
                  ? `Showing ${entries.length} of ${total} entries`
                  : `Showing ${entries.length} entries — more available`}
              </p>
            </div>
          ) : null}
        </>
      ) : error === null ? (
        <EmptyState
          icon={<LayoutTemplateIcon size={28} />}
          title={FLEET_LIBRARY_EMPTY_TITLE}
          description={showAddLibraryEntry ? FLEET_LIBRARY_EMPTY_DESCRIPTION : FLEET_LIBRARY_EMPTY_DESCRIPTION_READONLY}
          action={
            <div className="flex flex-wrap items-center justify-center gap-md">
              <LibraryDocsLink />
              {showAddLibraryEntry ? (
                <AddLibraryDialog workspaceId={workspaceId} defaultOpen={initialCreateOpen} />
              ) : null}
            </div>
          }
        />
      ) : null}

      {error ? (
        <div role="alert" className="flex items-center gap-3">
          <p className="text-sm text-destructive">{galleryErrorCopy(error)}</p>
          <Button type="button" variant="secondary" onClick={retryFailedRead} disabled={pending}>
            Retry
          </Button>
        </div>
      ) : null}
    </div>
  );
}
