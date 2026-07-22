import { useMemo, useState } from "react";
import {
  getCoreRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  useReactTable,
  type ColumnDef,
  type PaginationState,
  type SortingState,
} from "@tanstack/react-table";

import type {
  ClientDataTablePagination,
  DataTableColumn,
  DataTablePagination,
  DataTableProps,
} from "./DataTable.types";
import { PAGINATION_KIND } from "./Pagination";

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
  if (pagination.kind === PAGINATION_KIND.cursor) return pagination.nextCursor !== null;
  return pagination.page > 1
    || pagination.total === undefined
    || pagination.total > pagination.pageSize;
}

function buildColumns<T>(columns: DataTableColumn<T>[], externallySorted: boolean): ColumnDef<T>[] {
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

type ModelProps<T> = Pick<
  DataTableProps<T>,
  "columns" | "rows" | "rowKey" | "sortKey" | "sortDirection" | "onSortChange" | "pagination"
>;

export function useDataTableModel<T>({
  columns,
  rows,
  rowKey,
  sortKey,
  sortDirection,
  onSortChange,
  pagination,
}: ModelProps<T>) {
  const clientPagination = isClientPagination(pagination);
  const pageSize = clientPagination ? pagination?.pageSize ?? DEFAULT_PAGE_SIZE : rows.length || DEFAULT_PAGE_SIZE;
  const [sorting, setSorting] = useState<SortingState>([]);
  const [page, setPage] = useState<PaginationState>({ pageIndex: 0, pageSize });
  const externallySorted = onSortChange !== undefined;
  const tableColumns = useMemo(() => buildColumns(columns, externallySorted), [columns, externallySorted]);
  const columnsByKey = useMemo(() => new Map(columns.map((column) => [column.key, column])), [columns]);
  const controlledSorting: SortingState = sortKey
    ? [{ id: sortKey, desc: sortDirection === "descending" }]
    : [];
  const lastClientPage = Math.max(0, Math.ceil(rows.length / pageSize) - 1);
  const pageIndex = clientPagination ? Math.min(page.pageIndex, lastClientPage) : page.pageIndex;

  // Keep internal state canonical when rows shrink. React immediately retries
  // this render, so a later row-count increase cannot revive an invalid page.
  if (page.pageIndex !== pageIndex) {
    setPage((current) => ({ ...current, pageIndex }));
  }

  const table = useReactTable({
    columns: tableColumns,
    data: rows,
    getRowId: (row) => rowKey(row),
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: clientPagination ? getPaginationRowModel() : undefined,
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
      pagination: { pageIndex, pageSize },
    },
  });
  return { columnsByKey, table };
}
