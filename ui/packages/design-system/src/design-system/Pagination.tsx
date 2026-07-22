import { Button } from "./Button";
import { Spinner } from "./Spinner";
import { cn } from "../utils";

export const PAGINATION_KIND = {
  client: "client",
  cursor: "cursor",
  page: "page",
} as const;

/*
 * Pagination — two shapes under one component:
 *   • cursor: opaque next-cursor string (activity feed, telemetry).
 *   • page:   numeric pages (agents list).
 * Both land on the same UI so pages render identically regardless of the
 * server pagination rules. React Server Component (RSC)-safe — event handlers
 * are passed as props and forwarded to the RSC-safe shared Button.
 */

export interface CursorPaginationProps {
  kind: typeof PAGINATION_KIND.cursor;
  nextCursor: string | null;
  onNext: (cursor: string) => void;
  /** Number of rows fetched into this append-only cursor feed. */
  loadedCount?: number;
  isLoading?: boolean;
  className?: string;
}

export interface PagePaginationProps {
  kind: typeof PAGINATION_KIND.page;
  page: number;
  pageSize: number;
  total?: number;
  /** Plural noun shown after the total count. Defaults to "items". */
  totalLabel?: string;
  onPageChange: (page: number) => void;
  isLoading?: boolean;
  className?: string;
}

export type PaginationProps = CursorPaginationProps | PagePaginationProps;

export function Pagination(props: PaginationProps) {
  if (props.kind === PAGINATION_KIND.cursor) return <CursorPagination {...props} />;
  return <PagePagination {...props} />;
}

function CursorPagination({
  nextCursor,
  onNext,
  loadedCount,
  isLoading,
  className,
}: CursorPaginationProps) {
  const exhausted = nextCursor === null;
  return (
    <nav
      data-slot="pagination-cursor"
      data-testid="pagination-cursor"
      role="navigation"
      aria-label="Feed pagination"
      aria-busy={isLoading ? "true" : "false"}
      className={cn("flex flex-wrap items-center justify-end gap-2 py-3", className)}
    >
      {loadedCount !== undefined ? (
        <span
          className="mr-auto font-mono text-label leading-label text-muted-foreground tabular-nums"
          aria-live="polite"
          aria-atomic="true"
        >
          {loadedCount} loaded · sort scope: loaded
        </span>
      ) : null}
      <Button
        type="button"
        variant="ghost"
        size="sm"
        disabled={exhausted || isLoading}
        onClick={() => {
          if (nextCursor) onNext(nextCursor);
        }}
        aria-label="Load more items"
      >
        {isLoading ? <Spinner size="sm" srLabel="" /> : null}
        {isLoading ? "Loading…" : exhausted ? "End of feed" : "Load more"}
      </Button>
    </nav>
  );
}

function PagePagination({
  page,
  pageSize,
  total,
  totalLabel = "items",
  onPageChange,
  isLoading,
  className,
}: PagePaginationProps) {
  const totalPages = total != null ? Math.max(1, Math.ceil(total / pageSize)) : null;
  const hasPrev = page > 1;
  const hasNext = totalPages == null ? true : page < totalPages;
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
        disabled={!hasNext || isLoading}
        onClick={() => onPageChange(page + 1)}
        aria-label="Next page"
      >
        Next
      </Button>
    </nav>
  );
}

export default Pagination;
