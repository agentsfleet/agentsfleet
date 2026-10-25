import { useLayoutEffect, useRef, type ReactNode } from "react";
import type { ReactTable } from "@tanstack/react-table";
import { ArrowDown, ArrowUp, ChevronsUpDown } from "lucide-react";

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
  DataTableDisclosure,
  DataTableRowData,
  DataTableSurface,
} from "./DataTable.types";
import { DATA_TABLE_ROLE, DATA_TABLE_SURFACE } from "./DataTable.types";
import { DataTableRows } from "./DataTableRows";
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
  surface,
}: {
  table: ReactTable<DataTableFeatures, T>;
  columnsByKey: ColumnMap<T>;
  sticky: boolean;
  isLoading?: boolean;
  onSortChange?: (key: string) => void;
  interactive: boolean;
  surface: DataTableSurface;
}) {
  return (
    <thead role={DATA_TABLE_ROLE.rowgroup} className={cn(surface === DATA_TABLE_SURFACE.plain ? "bg-background" : "bg-table-header", sticky && "sticky top-0 z-10")}>
      {table.getHeaderGroups().map((group) => (
        <tr key={group.id} role={DATA_TABLE_ROLE.row}>
          {group.headers.map((header) => (
            <DataTableHeading key={header.id} header={header} definition={columnsByKey.get(header.column.id)} isLoading={isLoading} onSortChange={onSortChange} />
          ))}
          {interactive ? <th role={DATA_TABLE_ROLE.columnheader} scope="col" className="w-10 border-b border-border"><span className="sr-only">Open</span></th> : null}
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
                role={DATA_TABLE_ROLE.columnheader}
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

export function DataTableFooter<T extends DataTableRowData>({
  table,
  pagination,
  totalRows,
  isLoading,
  surface = DATA_TABLE_SURFACE.panel,
}: {
  table: ReactTable<DataTableFeatures, T>;
  pagination: DataTablePagination | undefined;
  totalRows: number;
  isLoading?: boolean;
  surface?: DataTableSurface;
}) {
  if (pagination === false) return null;
  const className = surface === DATA_TABLE_SURFACE.plain
    ? "border-t border-border" : "border-t border-border bg-table-header px-table-inline";
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
        className={className}
      />
    );
  }
  if (surface !== DATA_TABLE_SURFACE.plain && totalRows === 0 && !hasExternalPaginationNavigation(pagination)) return null;
  return (
    <Pagination
      {...pagination}
      isLoading={isLoading || pagination.isLoading}
      className={cn(className, pagination.className)}
    />
  );
}

type DataTableViewProps<T extends DataTableRowData> = {
  table: ReactTable<DataTableFeatures, T>;
  columnsByKey: ColumnMap<T>;
  caption?: string;
  onRowClick?: (row: T) => void;
  rowDisclosure?: DataTableDisclosure<T>;
  surface: DataTableSurface;
  empty: ReactNode;
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

function DataTableContent<T extends DataTableRowData>({
  table, columnsByKey, caption, onRowClick, isLoading, stickyHeader, onSortChange, surface, rowDisclosure, empty,
}: DataTableViewProps<T>) {
  return <table className={cn("w-full min-w-full border-collapse font-sans text-body-sm",
    surface === DATA_TABLE_SURFACE.plain && "[&_th:first-child]:pl-0 [&_td:first-child]:pl-0 [&_th:last-child]:pr-0 [&_td:last-child]:pr-0")}
    role={DATA_TABLE_ROLE.table} aria-busy={isLoading ? "true" : "false"}>
    {caption ? <caption className="sr-only">{caption}</caption> : null}
    <DataTableHead table={table} columnsByKey={columnsByKey} sticky={stickyHeader}
      isLoading={isLoading} onSortChange={onSortChange} interactive={Boolean(onRowClick)} surface={surface} />
    <DataTableRows table={table} columnsByKey={columnsByKey} onRowClick={onRowClick}
      rowDisclosure={rowDisclosure} surface={surface} isLoading={isLoading} empty={empty} />
  </table>;
}

export function DataTableView<T extends DataTableRowData>(props: DataTableViewProps<T>) {
  const {
  table, columnsByKey, caption, onRowClick, className, isLoading,
  viewportClassName, onSortChange, pagination, totalRows, surface,
  } = props;
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
      data-surface={surface}
      className={cn(
        "flex w-full min-h-0 flex-1 flex-col overflow-hidden",
        surface === DATA_TABLE_SURFACE.panel && "rounded-lg border border-border bg-table-row",
        className,
      )}
    >
      <div
        ref={viewportRef}
        // Rows scroll inside a height-constrained page; other pages grow naturally.
        className={cn(
          "min-h-0 flex-1 overflow-x-auto overscroll-x-contain motion-safe:scroll-smooth",
          "overflow-y-auto focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-pulse",
          viewportClassName,
        )}
        tabIndex={0}
        role="region"
        aria-label={caption ? `${caption}, scrollable` : "Scrollable table"}
      >
        <DataTableContent {...props} columnsByKey={columnsByKey} onRowClick={onRowClick} onSortChange={handleSortChange} />
      </div>
      <DataTableFooter table={table} pagination={pagination} totalRows={totalRows} isLoading={isLoading} surface={surface} />
    </div>
  );
}
