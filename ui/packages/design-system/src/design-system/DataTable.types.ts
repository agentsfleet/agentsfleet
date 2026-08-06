import type * as React from "react";

import {
  PAGINATION_KIND,
  type PagePaginationProps,
} from "./Pagination";

/**
 * Structural bound the table engine requires of row data.
 *
 * Mirrored here rather than imported: v9 tightened its own row constraint from
 * unconstrained to `Record<string, any> | Array<any>`, and re-exporting that
 * type would put a TanStack name in the public props surface — which this
 * file's `onSortChange` contract deliberately avoids. The shape is copied
 * exactly, so a row type that satisfies this satisfies the engine.
 *
 * `any` rather than `unknown` is load-bearing and not laziness: callers pass
 * `interface` row types, and an interface has no implicit index signature, so
 * it is not assignable to `Record<string, unknown>`. Narrowing this bound would
 * reject every existing caller and force nine dashboard components to restate
 * their row types as aliases for no gain in safety — `T` is still whatever the
 * caller declared, and every column accessor stays checked against it.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- see note above
export type DataTableRowData = Record<string, any> | Array<any>;

export type DataTableColumn<T extends DataTableRowData> = {
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
  pageSizeOptions?: readonly number[];
};

export type DataTablePagination =
  | false
  | ClientDataTablePagination
  | PagePaginationProps;

export interface DataTableProps<T extends DataTableRowData> {
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
