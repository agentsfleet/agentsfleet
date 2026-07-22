"use client";

import { EmptyState } from "./EmptyState";
import { hasExternalPaginationNavigation, useDataTableModel } from "./DataTableModel";
import type { DataTablePagination, DataTableProps } from "./DataTable.types";
import { DataTableFooter, DataTableView } from "./DataTableView";

export type {
  ClientDataTablePagination,
  DataTableColumn,
  DataTablePagination,
  DataTableProps,
} from "./DataTable.types";

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
    if (!hasExternalPaginationNavigation(pagination)) return <>{emptyState}</>;
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
