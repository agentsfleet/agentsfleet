import { useEffect, useMemo, useState } from "react";
import {
  createCoreRowModel,
  createPaginatedRowModel,
  createSortedRowModel,
  rowPaginationFeature,
  rowSortingFeature,
  sortFn_alphanumeric,
  sortFn_basic,
  sortFn_datetime,
  sortFn_text,
  tableFeatures,
  useTable,
  type ColumnDef,
  type PaginationState,
  type SortingState,
} from "@tanstack/react-table";

import type {
  ClientDataTablePagination,
  DataTableColumn,
  DataTablePagination,
  DataTableProps,
  DataTableRowData,
} from "./DataTable.types";
import { PAGINATION_KIND } from "./Pagination";

/**
 * The feature set this table is built from. Declared at module scope on
 * purpose: v9 stitches features into the table's TYPE, so building this per
 * render would both defeat inference and rebuild the instance every pass.
 *
 * `paginatedRowModel` is registered unconditionally even though external
 * pagination does not use it — v9 features are static, so the v8 trick of
 * passing the row model only for client pagination is not expressible.
 * `manualPagination` is what actually decides whether it runs, which is the
 * option that carried that meaning in v8 too.
 */
export const dataTableFeatures = tableFeatures({
  rowSortingFeature,
  rowPaginationFeature,
  coreRowModel: createCoreRowModel(),
  sortedRowModel: createSortedRowModel(),
  paginatedRowModel: createPaginatedRowModel(),
  sortFns: {
    alphanumeric: sortFn_alphanumeric,
    basic: sortFn_basic,
    datetime: sortFn_datetime,
    text: sortFn_text,
  },
});

export type DataTableFeatures = typeof dataTableFeatures;

export const DEFAULT_PAGE_SIZE = 25;

export function isClientPagination(
  pagination: DataTablePagination | undefined,
): pagination is ClientDataTablePagination | undefined {
  return pagination === undefined || (
    pagination !== false && (pagination.kind === undefined || pagination.kind === PAGINATION_KIND.client)
  );
}

export function hasExternalPaginationNavigation(
  pagination: DataTablePagination | undefined,
): boolean {
  if (pagination === false || isClientPagination(pagination)) return false;
  if (pagination.isLoading) return true;
  if (pagination.page > 1) return true;
  // An explicit `hasNext` is authoritative on page one: a cursor feed that
  // fits in a single page would otherwise render a pager with both buttons
  // dead, purely because its total is unknowable.
  if (pagination.hasNext !== undefined) return pagination.hasNext;
  return pagination.total === undefined || pagination.total > pagination.pageSize;
}

function buildColumns<T extends DataTableRowData>(
  columns: DataTableColumn<T>[],
  externallySorted: boolean,
): ColumnDef<DataTableFeatures, T>[] {
  return columns.map((column) => {
    const sortingRequested = column.sortable ?? column.sortValue !== undefined;
    const sortingEnabled = sortingRequested && (externallySorted || column.sortValue !== undefined);
    const accessor = column.sortValue
      ? { accessorFn: column.sortValue }
      : sortingEnabled ? { accessorKey: column.key } : {};
    return {
      id: column.key,
      ...accessor,
      enableSorting: sortingEnabled,
      sortDescFirst: false,
      header: () => column.header,
      cell: (context) => column.cell(context.row.original),
    };
  });
}

type ModelProps<T extends DataTableRowData> = Pick<
  DataTableProps<T>,
  "columns" | "rows" | "rowKey" | "sortKey" | "sortDirection" | "onSortChange" | "pagination"
>;

export function useDataTableModel<T extends DataTableRowData>({
  columns,
  rows,
  rowKey,
  sortKey,
  sortDirection,
  onSortChange,
  pagination,
}: ModelProps<T>) {
  const clientPagination = isClientPagination(pagination);
  const initialPageSize = clientPagination
    ? pagination?.pageSize ?? DEFAULT_PAGE_SIZE
    : rows.length || DEFAULT_PAGE_SIZE;
  const [sorting, setSorting] = useState<SortingState>([]);
  const [page, setPage] = useState<PaginationState>({
    pageIndex: 0,
    pageSize: initialPageSize,
  });
  useEffect(() => {
    if (!clientPagination) return;
    setPage((current) => (
      current.pageSize === initialPageSize
        ? current
        : { pageIndex: 0, pageSize: initialPageSize }
    ));
  }, [clientPagination, initialPageSize]);
  const externallySorted = onSortChange !== undefined;
  const tableColumns = useMemo(() => buildColumns(columns, externallySorted), [columns, externallySorted]);
  const columnsByKey = useMemo(() => new Map(columns.map((column) => [column.key, column])), [columns]);
  const controlledSorting: SortingState = sortKey
    ? [{ id: sortKey, desc: sortDirection === "descending" }]
    : [];
  const lastClientPage = Math.max(0, Math.ceil(rows.length / page.pageSize) - 1);
  const pageIndex = clientPagination ? Math.min(page.pageIndex, lastClientPage) : page.pageIndex;

  // Keep internal state canonical when rows shrink. React immediately retries
  // this render, so a later row-count increase cannot revive an invalid page.
  if (page.pageIndex !== pageIndex) {
    setPage((current) => ({ ...current, pageIndex }));
  }

  const table = useTable({
    features: dataTableFeatures,
    columns: tableColumns,
    data: rows,
    getRowId: (row) => rowKey(row),
    manualSorting: externallySorted,
    manualPagination: !clientPagination,
    autoResetPageIndex: false,
    onSortingChange: (updater) => {
      setSorting(updater);
      if (clientPagination) setPage((current) => ({ ...current, pageIndex: 0 }));
    },
    onPaginationChange: setPage,
    state: {
      sorting: externallySorted ? controlledSorting : sorting,
      pagination: { pageIndex, pageSize: page.pageSize },
    },
  });
  return { columnsByKey, table };
}
