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
