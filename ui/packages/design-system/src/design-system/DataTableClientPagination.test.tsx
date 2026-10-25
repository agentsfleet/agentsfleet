import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DataTable } from "./DataTable";
import { ROWS, COLUMNS, type Row } from "./DataTable.test-fixture";

describe("DataTable", () => {
  it("paginates local rows at 25 items without growing the page", () => {
    const rows = Array.from({ length: 26 }, (_, index) => ({
      id: String(index + 1),
      name: `Row ${index + 1}`,
      spend: index,
    }));
    render(<DataTable columns={COLUMNS} rows={rows} rowKey={(row) => row.id} />);

    expect(screen.getByText("Page 1 of 2 · 26 items")).toBeInTheDocument();
    expect(screen.queryByText("Row 26")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("Row 26")).toBeInTheDocument();
    expect(screen.getByText("Page 2 of 2 · 26 items")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Name" }));
    expect(screen.getByText("Page 1 of 2 · 26 items")).toBeInTheDocument();
  });

  it("preserves an equal-length rerender and does not revive a clamped client page", () => {
    const rows = Array.from({ length: 26 }, (_, index) => ({
      id: String(index + 1),
      name: `Row ${index + 1}`,
      spend: index,
    }));
    const { rerender } = render(<DataTable columns={COLUMNS} rows={rows} rowKey={(row) => row.id} />);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));

    rerender(<DataTable columns={COLUMNS} rows={[...rows]} rowKey={(row) => row.id} />);
    expect(screen.getByText("Page 2 of 2 · 26 items")).toBeInTheDocument();
    expect(screen.getByText("Row 26")).toBeInTheDocument();

    rerender(<DataTable columns={COLUMNS} rows={rows.slice(0, 2)} rowKey={(row) => row.id} />);
    expect(screen.getByText("Page 1 of 1 · 2 items")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
    expect(screen.getByText("Row 1")).toBeInTheDocument();

    rerender(<DataTable columns={COLUMNS} rows={rows} rowKey={(row) => row.id} />);
    expect(screen.getByText("Page 1 of 2 · 26 items")).toBeInTheDocument();
    expect(screen.getByText("Row 1")).toBeInTheDocument();
    expect(screen.queryByText("Row 26")).toBeNull();
  });

  it("should keep one-page table controls visible and disabled", () => {
    render(<DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />);
    expect(screen.getByRole("navigation", { name: "Pagination" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Rows per page" })).toHaveTextContent("25");
    expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
  });

  it("should keep table controls visible at the exact default page-size boundary", () => {
    const rows = Array.from({ length: 25 }, (_, index) => ({
      id: String(index),
      name: `Row ${index}`,
      spend: index,
    }));
    render(<DataTable columns={COLUMNS} rows={rows} rowKey={(row) => row.id} />);

    expect(screen.getByText("Page 1 of 1 · 25 items")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
  });

  it("honours a custom client page size", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(row) => row.id}
        pagination={{ kind: "client", pageSize: 1 }}
      />,
    );

    expect(screen.getByText("Page 1 of 2 · 2 items")).toBeInTheDocument();
    expect(screen.queryByText("Bravo")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("Bravo")).toBeInTheDocument();
  });

  it("should apply a changed client page size prop and reset to page one", () => {
    const props = {
      columns: COLUMNS,
      rows: ROWS,
      rowKey: (row: Row) => row.id,
    };
    const { rerender } = render(
      <DataTable {...props} pagination={{ kind: "client", pageSize: 1 }} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("Bravo")).toBeInTheDocument();

    rerender(
      <DataTable {...props} pagination={{ kind: "client", pageSize: 2 }} />,
    );

    expect(screen.getByText("Page 1 of 1 · 2 items")).toBeInTheDocument();
    expect(screen.getByText("Alpha")).toBeInTheDocument();
    expect(screen.getByText("Bravo")).toBeInTheDocument();
  });

  it("should reset to page one when client page size changes", () => {
    const rows = Array.from({ length: 26 }, (_, index) => ({
      id: String(index + 1),
      name: `Row ${index + 1}`,
      spend: index,
    }));
    render(<DataTable columns={COLUMNS} rows={rows} rowKey={(row) => row.id} />);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("Row 26")).toBeInTheDocument();

    const trigger = screen.getByRole("combobox", { name: "Rows per page" });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    fireEvent.click(screen.getByRole("option", { name: "50" }));

    expect(screen.getByText("Page 1 of 1 · 26 items")).toBeInTheDocument();
    expect(screen.getByText("Row 1")).toBeInTheDocument();
    expect(screen.getByText("Row 26")).toBeInTheDocument();
  });

  it("renders every row when pagination is explicitly disabled", () => {
    const rows = Array.from({ length: 26 }, (_, index) => ({ id: String(index), name: `Row ${index}`, spend: index }));
    render(<DataTable columns={COLUMNS} rows={rows} rowKey={(row) => row.id} pagination={false} />);
    expect(screen.getByText("Row 25")).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
  });
});
