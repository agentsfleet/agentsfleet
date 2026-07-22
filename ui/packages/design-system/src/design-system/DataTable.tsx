"use client";

import { EmptyState } from "./EmptyState";
import { useDataTableModel } from "./DataTableModel";
import type { DataTablePagination, DataTableProps } from "./DataTable.types";
import { DataTableFooter, DataTableView } from "./DataTableView";
import { PAGINATION_KIND, type PagePaginationProps } from "./Pagination";

export type {
  ClientDataTablePagination,
  DataTableColumn,
  DataTablePagination,
  DataTableProps,
} from "./DataTable.types";

function hasEmptyPageNavigation(pagination: DataTablePagination | undefined): boolean {
  if (
    pagination === false
    || pagination === undefined
    || pagination.kind === undefined
    || pagination.kind === PAGINATION_KIND.client
  ) {
    return false;
  }
  if (pagination.kind === PAGINATION_KIND.cursor) return pagination.nextCursor !== null;
  const pagePagination = pagination as PagePaginationProps;
  return pagePagination.page > 1
    || pagePagination.total === undefined
    || pagePagination.total > pagePagination.pageSize;
}

function isPaginationLoading(pagination: DataTablePagination | undefined): boolean {
  return pagination !== undefined
    && pagination !== false
    && "isLoading" in pagination
    && pagination.isLoading === true;
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
  stickyHeader = true,
  sortKey,
  sortDirection,
  onSortChange,
  pagination,
}: DataTableProps<T>) {
  const { columnsByKey, table } = useDataTableModel({
    columns,
    rows,
    rowKey,
    sortKey,
    sortDirection,
    onSortChange,
    pagination,
  });
  const emptyState = empty ?? <EmptyState title="Nothing to show yet" />;
  const loading = isLoading || isPaginationLoading(pagination);

  if (!loading && rows.length === 0) {
    if (!hasEmptyPageNavigation(pagination)) return <>{emptyState}</>;
    return (
      <>
        {emptyState}
        <DataTableFooter table={table} pagination={pagination} totalRows={0} />
      </>
    );
  }

  return (
    <DataTableView
      table={table}
      columnsByKey={columnsByKey}
      caption={caption}
      onRowClick={onRowClick}
      className={className}
      isLoading={loading}
      stickyHeader={stickyHeader}
      onSortChange={onSortChange}
      pagination={pagination}
      totalRows={rows.length}
    />
  );
}
