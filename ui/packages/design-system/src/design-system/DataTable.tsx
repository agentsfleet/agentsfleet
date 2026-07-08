import * as React from "react";
import { ArrowDown, ArrowUp } from "lucide-react";
import { cn } from "../utils";
import { EmptyState } from "./EmptyState";

export type DataTableColumn<T> = {
  key: string;
  header: React.ReactNode;
  /** Render the cell for a row. Return a string for plain text. */
  cell: (row: T) => React.ReactNode;
  /** Optional right-align (common for numeric/spend cells). */
  numeric?: boolean;
  /** Hide on mobile (< sm breakpoint) to reduce horizontal scroll. */
  hideOnMobile?: boolean;
  /**
   * Renders the header as a clickable control that reports clicks via
   * `onSortChange`. Current direction/aria-sort are derived from
   * `sortKey`/`sortDirection`, not set per-column — a column is never
   * self-describing about the *current* sort, only about being sortable.
   */
  sortable?: boolean;
};

export interface DataTableProps<T> {
  columns: DataTableColumn<T>[];
  rows: T[];
  rowKey: (row: T) => string;
  caption?: string;
  onRowClick?: (row: T) => void;
  /** Rendered when rows.length === 0. Supplying your own disables the default. */
  empty?: React.ReactNode;
  className?: string;
  /** aria-busy while loading. Skeleton is the caller's job (Suspense fallback). */
  isLoading?: boolean;
  /**
   * Bounds the table to a scrollable region with the header pinned via
   * `position: sticky` — keeps column labels in view on long lists. Default
   * false: renders exactly as before, growing with the page. Opt-in because
   * a bounded height is a deliberate choice per call site, not every table.
   */
  stickyHeader?: boolean;
  /** Key of the column currently driving sort order, for `sortable` columns. */
  sortKey?: string;
  sortDirection?: "ascending" | "descending";
  /** Fired with a column's `key` when its (sortable) header is clicked. */
  onSortChange?: (key: string) => void;
}

export function DataTable<T>({
  columns,
  rows,
  rowKey,
  caption,
  onRowClick,
  empty,
  className,
  isLoading,
  stickyHeader,
  sortKey,
  sortDirection,
  onSortChange,
}: DataTableProps<T>) {
  if (!isLoading && rows.length === 0) {
    return <>{empty ?? <EmptyState title="Nothing to show yet" />}</>;
  }

  const table = (
    <table
      className="w-full border-collapse text-sm"
      aria-busy={isLoading ? "true" : "false"}
    >
      {caption ? <caption className="sr-only">{caption}</caption> : null}
      <thead className={cn("bg-muted", stickyHeader && "sticky top-0 z-10")}>
        <tr>
          {columns.map((c) => {
            const active = c.sortable && sortKey === c.key;
            const ariaSort = active ? sortDirection : "none";
            return (
              <th
                key={c.key}
                scope="col"
                aria-sort={c.sortable ? ariaSort : undefined}
                className={cn(
                  "px-3 py-1.5 text-left text-xs font-medium uppercase tracking-wide text-muted-foreground",
                  c.numeric && "text-right",
                  c.hideOnMobile && "hidden sm:table-cell",
                )}
              >
                {c.sortable && onSortChange ? (
                  <button
                    type="button"
                    onClick={() => onSortChange(c.key)}
                    className="inline-flex items-center gap-1 hover:text-foreground"
                  >
                    {c.header}
                    {active ? (
                      sortDirection === "ascending" ? (
                        <ArrowUp size={12} aria-hidden="true" />
                      ) : (
                        <ArrowDown size={12} aria-hidden="true" />
                      )
                    ) : null}
                  </button>
                ) : (
                  c.header
                )}
              </th>
            );
          })}
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => {
          const key = rowKey(row);
          const clickable = !!onRowClick;
          return (
            <tr
              key={key}
              className={cn(
                "border-t border-border transition-colors",
                clickable && "cursor-pointer hover:bg-muted focus-within:bg-muted",
                "motion-reduce:transition-none",
              )}
              onClick={clickable ? () => onRowClick!(row) : undefined}
              onKeyDown={clickable ? (e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onRowClick!(row);
                }
              } : undefined}
              tabIndex={clickable ? 0 : undefined}
            >
              {columns.map((c) => (
                <td
                  key={c.key}
                  className={cn(
                    "px-3 py-1.5 align-middle text-foreground",
                    c.numeric && "text-right tabular-nums",
                    c.hideOnMobile && "hidden sm:table-cell",
                  )}
                >
                  {c.cell(row)}
                </td>
              ))}
            </tr>
          );
        })}
      </tbody>
    </table>
  );

  return (
    <div
      data-slot="data-table"
      data-testid="data-table"
      className={cn("w-full overflow-x-auto rounded-md border border-border", className)}
    >
      {stickyHeader ? (
        <div
          className="max-h-96 overflow-y-auto"
          tabIndex={0}
          role="region"
          aria-label={caption ? `${caption}, scrollable` : "Scrollable table"}
        >
          {table}
        </div>
      ) : (
        table
      )}
    </div>
  );
}
