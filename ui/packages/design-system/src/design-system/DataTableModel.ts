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
  const table = useReactTable({
    columns: tableColumns,
    data: rows,
    getRowId: (row) => rowKey(row),
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: clientPagination ? getPaginationRowModel() : undefined,
    manualSorting: externallySorted,
    manualPagination: !clientPagination,
    onSortingChange: setSorting,
    onPaginationChange: setPage,
    state: {
      sorting: externallySorted ? controlledSorting : sorting,
      pagination: { pageIndex: page.pageIndex, pageSize },
    },
  });
  return { columnsByKey, table };
}
