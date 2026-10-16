import { useLayoutEffect, useRef } from "react";
import type { ReactTable } from "@tanstack/react-table";
import { ArrowDown, ArrowUp, ChevronRight, ChevronsUpDown } from "lucide-react";

import { cn } from "../utils";
import { Button } from "./Button";
import {
  hasExternalPaginationNavigation,
  isClientPagination,
  type DataTableFeatures,
} from "./DataTableModel";
import type {
  DataTableColumn,
  DataTablePagination,
  DataTableRowData,
} from "./DataTable.types";
import {
  DEFAULT_PAGE_SIZE_OPTIONS,
  PAGINATION_KIND,
  Pagination,
} from "./Pagination";

type ColumnMap<T extends DataTableRowData> = Map<string, DataTableColumn<T>>;
type TableHeader<T extends DataTableRowData> = ReturnType<ReactTable<DataTableFeatures, T>["getHeaderGroups"]>[number]["headers"][number];

// One nominal size across all three states, so the header does not resize as
// sorting changes.
const SORT_ICON_SIZE = 14;
const INTERACTIVE_CELL_SELECTOR = "a, button, input, select, textarea, [role='button'], [role='link'], [role='checkbox'], [role='combobox'], [contenteditable='true']";

// The unsorted state is a chevron pair, not a double-headed arrow: lucide draws
// ArrowUpDown with two full shafts spanning 16 of its 24 units against the
// single arrow's 14, so beside a compact column label it read as the
// heaviest glyph in the header while meaning "no sort applied".
function sortIndicator(direction: false | "asc" | "desc") {
  if (direction === "asc") return <ArrowUp size={SORT_ICON_SIZE} aria-hidden="true" />;
  if (direction === "desc") return <ArrowDown size={SORT_ICON_SIZE} aria-hidden="true" />;
  return <ChevronsUpDown size={SORT_ICON_SIZE} aria-hidden="true" />;
}

function DataTableHead<T extends DataTableRowData>({
  table,
  columnsByKey,
  sticky,
  isLoading,
  onSortChange,
  interactive,
}: {
  table: ReactTable<DataTableFeatures, T>;
  columnsByKey: ColumnMap<T>;
  sticky: boolean;
  isLoading?: boolean;
  onSortChange?: (key: string) => void;
  interactive: boolean;
}) {
  return (
    <thead className={cn("bg-table-header", sticky && "sticky top-0 z-10")}>
      {table.getHeaderGroups().map((group) => (
        <tr key={group.id}>
          {group.headers.map((header) => (
            <DataTableHeading key={header.id} header={header} definition={columnsByKey.get(header.column.id)} isLoading={isLoading} onSortChange={onSortChange} />
          ))}
          {interactive ? <th scope="col" className="w-10 border-b border-border"><span className="sr-only">Open</span></th> : null}
        </tr>
      ))}
    </thead>
  );
}

function DataTableHeading<T extends DataTableRowData>({ header, definition, isLoading, onSortChange }: {
  header: TableHeader<T>;
  definition: DataTableColumn<T> | undefined;
  isLoading?: boolean;
  onSortChange?: (key: string) => void;
}) {
            const direction = header.column.getIsSorted();
            const canSort = header.column.getCanSort();
            const ariaSort = direction === "asc" ? "ascending" : direction === "desc" ? "descending" : "none";
            return (
              <th
                key={header.id}
                scope="col"
                aria-sort={canSort ? ariaSort : undefined}
                className={cn(
                  "h-11 border-b border-border text-left font-sans text-label font-medium text-muted-foreground",
                  canSort ? "p-0" : "px-table-inline py-table-block",
                  direction && "text-foreground",
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
                      "h-11 w-full justify-start gap-1.5 rounded-none border-0 px-table-inline text-label hover:bg-table-row-hover focus-visible:ring-inset focus-visible:ring-offset-0 motion-reduce:transition-none [&_svg]:size-3.5",
                      direction && "text-pulse",
                      definition?.numeric && "justify-end",
                    )}
                  >
                    {definition!.header}
                    {sortIndicator(direction)}
                  </Button>
                ) : definition!.header}
              </th>
            );
}

function DataTableBody<T extends DataTableRowData>({
  table,
  columnsByKey,
  onRowClick,
}: {
  table: ReactTable<DataTableFeatures, T>;
  columnsByKey: ColumnMap<T>;
  onRowClick?: (row: T) => void;
}) {
  return (
    <tbody>
      {table.getRowModel().rows.map((row) => (
        <tr
          key={row.id}
          className={cn(
            "group/row border-b border-table-divider bg-table-row even:bg-table-row-alternate last:border-b-0 transition-colors duration-snap ease-snap hover:bg-table-row-hover focus-within:bg-table-row-hover motion-reduce:transition-none",
            onRowClick && "cursor-pointer hover:shadow-table-indicator focus-within:shadow-table-indicator focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
          )}
          onClick={onRowClick ? (event) => {
            if (event.defaultPrevented || (event.target instanceof Element && event.target.closest(INTERACTIVE_CELL_SELECTOR))) return;
            onRowClick(row.original);
          } : undefined}
          onKeyDown={onRowClick ? (event) => {
            if (event.defaultPrevented || event.target !== event.currentTarget) return;
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              onRowClick(row.original);
            }
          } : undefined}
          tabIndex={onRowClick ? 0 : undefined}
        >
          {row.getAllCells().map((cell) => {
            const definition = columnsByKey.get(cell.column.id);
            return (
              <td
                key={cell.id}
                className={cn(
                  "h-12 px-table-inline py-table-block align-middle text-foreground",
                  definition?.primary && "font-medium",
                  definition?.numeric && "text-right tabular-nums",
                  // The heading's trailing sort icon takes 14px plus a 6px
                  // gap; reserve the same space so values end beneath labels.
                  definition?.numeric && cell.column.getCanSort() && "pr-3xl",
                  definition?.hideOnMobile && "hidden sm:table-cell",
                )}
              >
                {/* These are render callbacks, not component types. Passing a
                    freshly built callback to flexRender remounts the cell and
                    discards its focus and local state on parent updates. */}
                {definition!.cell(row.original)}
              </td>
            );
          })}
          {onRowClick ? <td className="w-10 pr-table-inline text-muted-foreground group-hover/row:text-pulse group-focus-within/row:text-pulse"><ChevronRight size={16} aria-hidden="true" /></td> : null}
        </tr>
      ))}
    </tbody>
  );
}

export function DataTableFooter<T extends DataTableRowData>({
  table,
  pagination,
  totalRows,
  isLoading,
}: {
  table: ReactTable<DataTableFeatures, T>;
  pagination: DataTablePagination | undefined;
  totalRows: number;
  isLoading?: boolean;
}) {
  if (pagination === false) return null;
  if (isClientPagination(pagination)) {
    if (totalRows === 0) return null;
    const pageSize = table.state.pagination.pageSize;
    return (
      <Pagination
        kind={PAGINATION_KIND.page}
        page={table.state.pagination.pageIndex + 1}
        pageSize={pageSize}
        total={totalRows}
        isLoading={isLoading}
        onPageChange={(page) => table.setPageIndex(page - 1)}
        pageSizeOptions={pagination?.pageSizeOptions ?? DEFAULT_PAGE_SIZE_OPTIONS}
        onPageSizeChange={(nextPageSize) => {
          table.setPageSize(nextPageSize);
          table.setPageIndex(0);
        }}
        className="border-t border-border bg-table-header px-table-inline"
      />
    );
  }
  if (totalRows === 0 && !hasExternalPaginationNavigation(pagination)) return null;
  return (
    <Pagination
      {...pagination}
      isLoading={isLoading || pagination.isLoading}
      className={cn("border-t border-border bg-table-header px-table-inline", pagination.className)}
    />
  );
}

type DataTableViewProps<T extends DataTableRowData> = {
  table: ReactTable<DataTableFeatures, T>;
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
};

function useTableViewport<T extends DataTableRowData>({ table, pagination }: Pick<DataTableViewProps<T>, "table" | "pagination">) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const numericPage = isClientPagination(pagination)
    ? table.state.pagination.pageIndex
    : pagination !== false && pagination.kind === PAGINATION_KIND.page
      ? pagination.page
      : null;
  const sortingSignature = table.state.sorting
    .map((sort) => `${sort.id}:${sort.desc}`)
    .join("|");
  const pageSize = isClientPagination(pagination)
    ? table.state.pagination.pageSize
    : pagination === false
      ? null
      : pagination.pageSize;
  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    // The ref is attached to a div this component renders unconditionally, and a
    // layout effect runs only once that div is in the DOM — so React cannot make
    // this null case true; it is a type-narrowing formality. Excluded from the
    // 100% coverage gate for the same reason as any unreachable defensive branch
    // (cf. website/src/components/HowItWorks.tsx).
    /* v8 ignore next */
    if (!viewport) return;
    viewport.scrollTo({ top: 0 });
  }, [numericPage, pageSize, sortingSignature]);
  return viewportRef;
}

export function DataTableView<T extends DataTableRowData>({
  table, columnsByKey, caption, onRowClick, className, isLoading,
  stickyHeader, viewportClassName, onSortChange, pagination, totalRows,
}: DataTableViewProps<T>) {
  const viewportRef = useTableViewport({ table, pagination });
  const handleSortChange = onSortChange
    ? (key: string) => {
        if (isClientPagination(pagination)) table.setPageIndex(0);
        onSortChange(key);
      }
    : undefined;
  return (
    <div
      data-slot="data-table"
      data-testid="data-table"
      // A column that can shrink, so the viewport below it has something to
      // bound against once a page constrains the height.
      className={cn(
        "flex w-full min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-table-row",
        className,
      )}
    >
      <div
        ref={viewportRef}
        // The ROWS scroll, not the page. Four call sites reached this by hand
        // with `viewportClassName="min-h-0 flex-1 max-h-none"` and six did not,
        // so Billing and Model library kept their headers while Fleet library,
        // Secrets, Approvals and API keys scrolled the whole canvas away. A
        // capability only the informed caller took up is the same drift that
        // put a 12px <h2> on nine routes; the primitive owns it now.
        //
        // This bounds against an ancestor that constrains height — a page
        // passing `fullHeight` to `PageLayout`. Where nothing constrains, the
        // flex basis simply grows and the page scrolls as it always did, so an
        // unconverted route is unchanged rather than broken.
        //
        // The old `max-h-96` default is still not what this is: that put a
        // scrollbar inside the cell padding, moving header text 6px in from the
        // right while the footer outside the box stayed put.
        className={cn(
          "min-h-0 flex-1 overflow-x-auto overscroll-x-contain motion-safe:scroll-smooth",
          "overflow-y-auto focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-pulse",
          viewportClassName,
        )}
        tabIndex={0}
        role="region"
        aria-label={caption ? `${caption}, scrollable` : "Scrollable table"}
      >
        <table className="w-full min-w-full border-collapse font-sans text-body-sm" aria-busy={isLoading ? "true" : "false"}>
          {caption ? <caption className="sr-only">{caption}</caption> : null}
          <DataTableHead
            table={table}
            columnsByKey={columnsByKey}
            sticky={stickyHeader}
            isLoading={isLoading}
            onSortChange={handleSortChange}
            interactive={Boolean(onRowClick)}
          />
          <DataTableBody table={table} columnsByKey={columnsByKey} onRowClick={onRowClick} />
        </table>
      </div>
      <DataTableFooter table={table} pagination={pagination} totalRows={totalRows} isLoading={isLoading} />
    </div>
  );
}
