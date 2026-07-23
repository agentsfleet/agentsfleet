import { Button } from "./Button";
import { Spinner } from "./Spinner";
import { cn } from "../utils";

export const PAGINATION_KIND = {
  client: "client",
  page: "page",
} as const;

/*
 * Pagination — ONE shape: numbered pages. Every table paginates identically,
 * whether the server counts pages or hands back opaque cursors, because an
 * operator moving between two tables should not have to learn two controls.
 *
 * There is deliberately no append-style "load more": it grows one list without
 * bound, pushes its own control off the bottom of the screen, and leaves the
 * operator scrolling to reach it. A cursor-backed caller pages through the
 * feed instead and reports `hasNext` from the cursor it holds.
 *
 * React Server Component (RSC)-safe — event handlers are passed as props and
 * forwarded to the RSC-safe shared Button.
 */

export interface PagePaginationProps {
  kind: typeof PAGINATION_KIND.page;
  page: number;
  pageSize: number;
  /** Omitted when the source cannot know it — a cursor feed never does. */
  total?: number;
  /**
   * Whether a further page exists. Required for a cursor-backed feed, whose
   * `total` is unknowable: without it "Next" would stay live at the end of
   * the feed and hand the operator an empty page.
   */
  hasNext?: boolean;
  /** Plural noun shown after the total count. Defaults to "items". */
  totalLabel?: string;
  onPageChange: (page: number) => void;
  isLoading?: boolean;
  className?: string;
}

export type PaginationProps = PagePaginationProps;

export function Pagination(props: PaginationProps) {
  return <PagePagination {...props} />;
}

function PagePagination({
  page,
  pageSize,
  total,
  hasNext,
  totalLabel = "items",
  onPageChange,
  isLoading,
  className,
}: PagePaginationProps) {
  const totalPages = total != null ? Math.max(1, Math.ceil(total / pageSize)) : null;
  const hasPrev = page > 1;
  // An explicit `hasNext` always wins: only the caller holding the cursor
  // knows whether the feed continues.
  const canAdvance = hasNext ?? (totalPages == null ? true : page < totalPages);
  return (
    <nav
      data-slot="pagination-page"
      data-testid="pagination-page"
      role="navigation"
      aria-label="Pagination"
      aria-busy={isLoading ? "true" : "false"}
      className={cn("flex flex-wrap items-center justify-end gap-2 py-3", className)}
    >
      <div className="mr-auto flex items-center gap-2 text-xs text-muted-foreground tabular-nums">
        <span aria-live="polite" aria-atomic="true">
          {totalPages != null
            ? `Page ${page} of ${totalPages} · ${total} ${totalLabel}`
            : `Page ${page}`}
        </span>
        {isLoading ? <Spinner size="sm" label="Loading…" className="border-0 bg-transparent p-0" /> : null}
      </div>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        disabled={!hasPrev || isLoading}
        onClick={() => onPageChange(page - 1)}
        aria-label="Previous page"
      >
        Previous
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        disabled={!canAdvance || isLoading}
        onClick={() => onPageChange(page + 1)}
        aria-label="Next page"
      >
        Next
      </Button>
    </nav>
  );
}

export default Pagination;
