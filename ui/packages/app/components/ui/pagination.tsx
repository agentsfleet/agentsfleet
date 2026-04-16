import { cn } from "@usezombie/design-system/utils";
import { Button } from "./button";

// Two shapes: cursor-paginated (activity feed, telemetry) and page-paginated
// (zombies list with numeric pages). Both live behind one component so pages
// use the same UI regardless of the backend contract.

export interface CursorPaginationProps {
  kind: "cursor";
  nextCursor: string | null;
  onNext: (cursor: string) => void;
  isLoading?: boolean;
  className?: string;
}

export interface PagePaginationProps {
  kind: "page";
  page: number;
  pageSize: number;
  total?: number;
  onPageChange: (page: number) => void;
  isLoading?: boolean;
  className?: string;
}

export type PaginationProps = CursorPaginationProps | PagePaginationProps;

export function Pagination(props: PaginationProps) {
  if (props.kind === "cursor") return <CursorPagination {...props} />;
  return <PagePagination {...props} />;
}

function CursorPagination({ nextCursor, onNext, isLoading, className }: CursorPaginationProps) {
  const exhausted = nextCursor === null;
  return (
    <nav
      data-slot="pagination-cursor"
      data-testid="pagination-cursor"
      role="navigation"
      aria-label="Feed pagination"
      className={cn("flex flex-wrap items-center justify-end gap-2 py-3", className)}
    >
      <Button
        type="button"
        variant="ghost"
        size="sm"
        disabled={exhausted || isLoading}
        onClick={() => { if (nextCursor) onNext(nextCursor); }}
        aria-label="Load more items"
      >
        {isLoading ? "Loading…" : exhausted ? "End of feed" : "Load more"}
      </Button>
    </nav>
  );
}

function PagePagination({ page, pageSize, total, onPageChange, isLoading, className }: PagePaginationProps) {
  const totalPages = total != null ? Math.max(1, Math.ceil(total / pageSize)) : null;
  const hasPrev = page > 1;
  const hasNext = totalPages == null ? true : page < totalPages;
  return (
    <nav
      data-slot="pagination-page"
      data-testid="pagination-page"
      role="navigation"
      aria-label="Pagination"
      className={cn("flex flex-wrap items-center justify-end gap-2 py-3", className)}
    >
      <span
        className="mr-auto text-xs text-muted-foreground tabular-nums"
        aria-live="polite"
        aria-atomic="true"
      >
        {totalPages != null ? `Page ${page} of ${totalPages}` : `Page ${page}`}
      </span>
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
