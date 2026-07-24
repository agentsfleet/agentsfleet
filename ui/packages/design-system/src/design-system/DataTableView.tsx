import { useLayoutEffect, useRef } from "react";
import { flexRender, type Table } from "@tanstack/react-table";
import { ArrowDown, ArrowUp, ArrowUpDown } from "lucide-react";

import { cn } from "../utils";
import { Button } from "./Button";
import {
  DEFAULT_PAGE_SIZE,
  hasExternalPaginationNavigation,
  isClientPagination,
} from "./DataTableModel";
import type { DataTableColumn, DataTablePagination } from "./DataTable.types";
import { PAGINATION_KIND, Pagination } from "./Pagination";

type ColumnMap<T> = Map<string, DataTableColumn<T>>;

function sortIndicator(direction: false | "asc" | "desc") {
  if (direction === "asc") return <ArrowUp size={14} aria-hidden="true" />;
  if (direction === "desc") return <ArrowDown size={14} aria-hidden="true" />;
  return <ArrowUpDown size={14} aria-hidden="true" />;
}

function DataTableHead<T>({
  table,
  columnsByKey,
  sticky,
  isLoading,
  onSortChange,
}: {
  table: Table<T>;
  columnsByKey: ColumnMap<T>;
  sticky: boolean;
  isLoading?: boolean;
  onSortChange?: (key: string) => void;
}) {
  return (
    <thead className={cn("bg-muted", sticky && "sticky top-0 z-10")}>
      {table.getHeaderGroups().map((group) => (
        <tr key={group.id}>
          {group.headers.map((header) => {
            const definition = columnsByKey.get(header.column.id);
            const direction = header.column.getIsSorted();
            const canSort = header.column.getCanSort();
            const ariaSort = direction === "asc" ? "ascending" : direction === "desc" ? "descending" : "none";
            return (
              <th
                key={header.id}
                scope="col"
                aria-sort={canSort ? ariaSort : undefined}
                className={cn(
                  "text-left font-mono text-label font-medium uppercase tracking-label text-muted-foreground",
                  canSort ? "p-0" : "px-3 py-1.5",
                  definition?.numeric && "text-right",
                  definition?.hideOnMobile && "hidden sm:table-cell",
                )}
              >
                {canSort ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={isLoading}
                    onClick={onSortChange
                      ? () => onSortChange(header.column.id)
                      : header.column.getToggleSortingHandler()}
                    className={cn(
                      "w-full justify-start gap-1.5 rounded-none border-0 uppercase tracking-label hover:bg-transparent focus-visible:ring-inset focus-visible:ring-offset-0 motion-reduce:transition-none",
                      definition?.numeric && "justify-end",
                    )}
                  >
                    {flexRender(header.column.columnDef.header, header.getContext())}
                    {sortIndicator(direction)}
                  </Button>
                ) : flexRender(header.column.columnDef.header, header.getContext())}
              </th>
            );
          })}
        </tr>
      ))}
    </thead>
  );
}

function DataTableBody<T>({
  table,
  columnsByKey,
  onRowClick,
}: {
  table: Table<T>;
  columnsByKey: ColumnMap<T>;
  onRowClick?: (row: T) => void;
}) {
  const state = table.getState();
  const renderKey = `${state.sorting.map((sort) => `${sort.id}:${sort.desc}`).join("|")}:${state.pagination.pageIndex}`;
  return (
    <tbody key={renderKey} className="motion-safe:animate-in motion-safe:fade-in motion-safe:duration-snap">
      {table.getRowModel().rows.map((row) => (
        <tr
          key={row.id}
          className={cn(
            "border-t border-border transition-colors duration-snap ease-snap motion-reduce:transition-none",
            onRowClick && "cursor-pointer hover:bg-muted focus-within:bg-muted",
          )}
          onClick={onRowClick ? () => onRowClick(row.original) : undefined}
          onKeyDown={onRowClick ? (event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              onRowClick(row.original);
            }
          } : undefined}
          tabIndex={onRowClick ? 0 : undefined}
        >
          {row.getVisibleCells().map((cell) => {
            const definition = columnsByKey.get(cell.column.id);
            return (
              <td
                key={cell.id}
                className={cn(
                  "px-3 py-1.5 align-middle text-foreground",
                  definition?.numeric && "text-right tabular-nums",
                  definition?.hideOnMobile && "hidden sm:table-cell",
                )}
              >
                {flexRender(cell.column.columnDef.cell, cell.getContext())}
              </td>
            );
          })}
        </tr>
      ))}
    </tbody>
  );
}

export function DataTableFooter<T>({
  table,
  pagination,
  totalRows,
}: {
  table: Table<T>;
  pagination: DataTablePagination | undefined;
  totalRows: number;
}) {
  if (pagination === false) return null;
  if (isClientPagination(pagination)) {
    const pageSize = pagination?.pageSize ?? DEFAULT_PAGE_SIZE;
    if (totalRows <= pageSize) return null;
    return (
      <Pagination
        kind={PAGINATION_KIND.page}
        page={table.getState().pagination.pageIndex + 1}
        pageSize={pageSize}
        total={totalRows}
        onPageChange={(page) => table.setPageIndex(page - 1)}
        className="border-t border-border px-3"
      />
    );
  }
  if (!hasExternalPaginationNavigation(pagination)) return null;
  return (
    <Pagination
      {...pagination}
      className={cn("border-t border-border px-3", pagination.className)}
    />
  );
}

export function DataTableView<T>({
  table,
  columnsByKey,
  caption,
  onRowClick,
  className,
  isLoading,
  stickyHeader,
  viewportClassName,
  onSortChange,
  pagination,
  totalRows,
}: {
  table: Table<T>;
  columnsByKey: ColumnMap<T>;
  caption?: string;
  onRowClick?: (row: T) => void;
  className?: string;
  isLoading?: boolean;
  stickyHeader: boolean;
  viewportClassName?: string;
  onSortChange?: (key: string) => void;
  pagination: DataTablePagination | undefined;
  totalRows: number;
}) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const numericPage = isClientPagination(pagination)
    ? table.getState().pagination.pageIndex
    : pagination !== false && pagination.kind === PAGINATION_KIND.page
      ? pagination.page
      : null;
  const sortingSignature = table.getState().sorting
    .map((sort) => `${sort.id}:${sort.desc}`)
    .join("|");
  const handleSortChange = onSortChange
    ? (key: string) => {
        if (isClientPagination(pagination)) table.setPageIndex(0);
        onSortChange(key);
      }
    : undefined;

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    if (typeof viewport.scrollTo === "function") viewport.scrollTo({ top: 0 });
    else viewport.scrollTop = 0;
  }, [numericPage, sortingSignature]);

  return (
    <div
      data-slot="data-table"
      data-testid="data-table"
      className={cn("w-full overflow-hidden rounded-md border border-border bg-card", className)}
    >
      <div
        ref={viewportRef}
        className={cn(
          "overflow-x-auto overscroll-contain motion-safe:scroll-smooth focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-pulse",
          stickyHeader && "overflow-y-auto",
          stickyHeader && !viewportClassName && "max-h-96",
          viewportClassName,
        )}
        tabIndex={0}
        role="region"
        aria-label={caption ? `${caption}, scrollable` : "Scrollable table"}
      >
        <table className="w-full min-w-full border-collapse font-mono text-mono" aria-busy={isLoading ? "true" : "false"}>
          {caption ? <caption className="sr-only">{caption}</caption> : null}
          <DataTableHead
            table={table}
            columnsByKey={columnsByKey}
            sticky={stickyHeader}
            isLoading={isLoading}
            onSortChange={handleSortChange}
          />
          <DataTableBody table={table} columnsByKey={columnsByKey} onRowClick={onRowClick} />
        </table>
      </div>
      <DataTableFooter table={table} pagination={pagination} totalRows={totalRows} />
    </div>
  );
}
