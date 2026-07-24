import type * as React from "react";

import {
  PAGINATION_KIND,
  type PagePaginationProps,
} from "./Pagination";

export type DataTableColumn<T> = {
  key: string;
  header: React.ReactNode;
  /** Render the cell for a row. Return a string for plain text. */
  cell: (row: T) => React.ReactNode;
  /** Scalar used by the built-in client sorter. Also opts the column into sorting. */
  sortValue?: (row: T) => string | number;
  /** Optional right-align (common for numeric/spend cells). */
  numeric?: boolean;
  /** Hide on mobile (< sm breakpoint) to reduce horizontal scroll. */
  hideOnMobile?: boolean;
  /** Override whether the header exposes a sorting control. */
  sortable?: boolean;
};

export type ClientDataTablePagination = {
  kind?: typeof PAGINATION_KIND.client;
  pageSize?: number;
};

export type DataTablePagination =
  | false
  | ClientDataTablePagination
  | PagePaginationProps;

export interface DataTableProps<T> {
  columns: DataTableColumn<T>[];
  rows: T[];
  rowKey: (row: T) => string;
  caption?: string;
  onRowClick?: (row: T) => void;
  /** Rendered when rows.length === 0. Supplying your own disables the default. */
  empty?: React.ReactNode;
  className?: string;
  /** aria-busy while loading. Skeleton is the caller's job (Suspense fallback). */
  isLoading?: boolean;
  /** Bounds the table and pins its header. Defaults to true. */
  stickyHeader?: boolean;
  /** Additional classes for the scrollable rows viewport. */
  viewportClassName?: string;
  /** Key of the column currently driving externally controlled sorting. */
  sortKey?: string;
  sortDirection?: "ascending" | "descending";
  /** Enables externally controlled sorting without exposing TanStack types. */
  onSortChange?: (key: string) => void;
  /**
   * Local pagination by default; pass false, page, or cursor configuration to override.
   * Built-in sorting on a cursor feed applies to the rows loaded so far.
   */
  pagination?: DataTablePagination;
}
