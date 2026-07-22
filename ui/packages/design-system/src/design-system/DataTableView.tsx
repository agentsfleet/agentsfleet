import { flexRender, type Table } from "@tanstack/react-table";
import { ArrowDown, ArrowUp, ArrowUpDown } from "lucide-react";

import { cn } from "../utils";
import { Button } from "./Button";
import { DEFAULT_PAGE_SIZE, isClientPagination } from "./DataTableModel";
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
                  "px-3 py-1.5 text-left font-mono text-label font-medium uppercase tracking-label text-muted-foreground",
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
                      "h-auto min-h-0 w-full justify-start gap-1.5 rounded-none border-0 p-0 uppercase tracking-label hover:bg-transparent motion-reduce:transition-none",
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
  if (
    pagination.kind === PAGINATION_KIND.cursor
    && pagination.nextCursor === null
    && !pagination.isLoading
  ) return null;
  if (
    pagination.kind === PAGINATION_KIND.page
    && pagination.page === 1
    && pagination.total !== undefined
    && pagination.total <= pagination.pageSize
  ) return null;
  return <Pagination {...pagination} className={cn("border-t border-border px-3", pagination.className)} />;
}

export function DataTableView<T>({
  table,
  columnsByKey,
  caption,
  onRowClick,
  className,
  isLoading,
  stickyHeader,
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
  onSortChange?: (key: string) => void;
  pagination: DataTablePagination | undefined;
  totalRows: number;
}) {
  return (
    <div
      data-slot="data-table"
      data-testid="data-table"
      className={cn("w-full overflow-hidden rounded-md border border-border bg-card", className)}
    >
      <div
        className={cn(
          "overflow-x-auto overscroll-contain motion-safe:scroll-smooth focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-pulse",
          stickyHeader && "max-h-96 overflow-y-auto",
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
            onSortChange={onSortChange}
          />
          <DataTableBody table={table} columnsByKey={columnsByKey} onRowClick={onRowClick} />
        </table>
      </div>
      <DataTableFooter table={table} pagination={pagination} totalRows={totalRows} />
    </div>
  );
}
