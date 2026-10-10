import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { DataTablePagination } from "./DataTable.types";
import { useDataTableModel } from "./DataTableModel";

const ROWS = Array.from({ length: 6 }, (_, id) => ({ id: String(id) }));
const COLUMNS = [{ key: "id", header: "Identifier", cell: (row: { id: string }) => row.id }];
const rowKey = (row: { id: string }) => row.id;
const INITIAL_PROPS: { pagination: DataTablePagination } = { pagination: false };

describe("table pagination configuration", () => {
  it.each([1, 3])("starts on the first page when rows shrink while page size changes to %i", (pageSize) => {
    const { result, rerender } = renderHook(
      ({ rows, pageSize }) => useDataTableModel({
        columns: COLUMNS, rows, rowKey, pagination: { kind: "client", pageSize },
      }),
      { initialProps: { rows: ROWS, pageSize: 2 } },
    );
    act(() => result.current.table.setPageIndex(2));
    expect(result.current.table.getRowModel().rows.map((row) => row.original.id)).toEqual(["4", "5"]);

    rerender({ rows: ROWS.slice(0, 4), pageSize });

    expect(result.current.table.state.pagination).toEqual({ pageIndex: 0, pageSize });
    expect(result.current.table.getRowModel().rows.map((row) => row.original.id))
      .toEqual(ROWS.slice(0, pageSize).map((row) => row.id));
  });

  it("preserves a valid page when enabling client pagination at the selected size", () => {
    const { result, rerender } = renderHook(
      ({ pagination }: { pagination: DataTablePagination }) =>
        useDataTableModel({ columns: COLUMNS, rows: ROWS, rowKey, pagination }),
      { initialProps: INITIAL_PROPS },
    );
    act(() => {
      result.current.table.setPageSize(2);
      result.current.table.setPageIndex(1);
    });
    rerender({ pagination: { kind: "client", pageSize: 2 } });
    expect(result.current.table.state.pagination).toEqual({ pageIndex: 1, pageSize: 2 });
    expect(result.current.table.getRowModel().rows.map((row) => row.original.id)).toEqual(["2", "3"]);
  });
});

describe("table column comparison", () => {
  type Ranked = { id: string; name: string; rank: number };
  // The names sort alphabetically one way and the ranks the other, so only the
  // column's own comparison can produce the rank order.
  const RANKED: Ranked[] = [
    { id: "a", name: "alpha", rank: 3 },
    { id: "b", name: "beta", rank: 1 },
    { id: "c", name: "gamma", rank: 2 },
  ];
  const byRank = (a: Ranked, b: Ranked) => a.rank - b.rank;

  it("sorts a column by its own comparison, in either direction", () => {
    const columns = [
      { key: "name", header: "Name", cell: (row: Ranked) => row.name, sortValue: (row: Ranked) => row.name, compare: byRank },
    ];
    const { result } = renderHook(() => useDataTableModel({ columns, rows: RANKED, rowKey: (row: Ranked) => row.id }));
    const order = () => result.current.table.getRowModel().rows.map((row) => row.original.name);

    act(() => result.current.table.setSorting([{ id: "name", desc: false }]));
    expect(order()).toEqual(["beta", "gamma", "alpha"]);
    act(() => result.current.table.setSorting([{ id: "name", desc: true }]));
    expect(order()).toEqual(["alpha", "gamma", "beta"]);
  });
});
