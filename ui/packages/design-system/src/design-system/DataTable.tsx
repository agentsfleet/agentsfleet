"use client";

import { EmptyState } from "./EmptyState";
import { hasExternalPaginationNavigation, useDataTableModel } from "./DataTableModel";
import type { DataTablePagination, DataTableProps, DataTableRowData } from "./DataTable.types";
import { DATA_TABLE_SURFACE } from "./DataTable.types";
import { DataTableFooter, DataTableView } from "./DataTableView";

export type {
  ClientDataTablePagination,
  DataTableColumn,
  DataTableDisclosure,
  DataTablePagination,
  DataTableProps,
  DataTableSurface,
} from "./DataTable.types";
export { DATA_TABLE_SURFACE } from "./DataTable.types";

function isPaginationLoading(pagination: DataTablePagination | undefined): boolean {
  return pagination !== undefined
    && pagination !== false
    && "isLoading" in pagination
    && pagination.isLoading === true;
}

export function DataTable<T extends DataTableRowData>(props: DataTableProps<T>) {
  const { rows, pagination, empty, isLoading, surface = DATA_TABLE_SURFACE.panel } = props;
  const { columnsByKey, table } = useDataTableModel(props);
  const emptyState = empty ?? <EmptyState title="Nothing to show yet" surface={surface} />;
  const loading = isLoading || isPaginationLoading(pagination);

  if (rows.length === 0 && surface !== DATA_TABLE_SURFACE.plain) {
    if (hasExternalPaginationNavigation(pagination)) {
      return (
        <>
          {emptyState}
          <DataTableFooter table={table} pagination={pagination} totalRows={0} isLoading={loading} />
        </>
      );
    }
    if (!loading) return <>{emptyState}</>;
  }

  return (
    <DataTableView
      {...props}
      table={table}
      columnsByKey={columnsByKey}
      surface={surface}
      empty={emptyState}
      isLoading={loading}
      stickyHeader={props.stickyHeader ?? true}
      pagination={pagination}
      totalRows={rows.length}
    />
  );
}
