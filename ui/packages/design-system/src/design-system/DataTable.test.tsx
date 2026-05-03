import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DataTable, type DataTableColumn } from "./DataTable";

type Row = { id: string; name: string; spend: number };

const ROWS: Row[] = [
  { id: "a", name: "Alpha", spend: 12 },
  { id: "b", name: "Bravo", spend: 34 },
];

const COLUMNS: DataTableColumn<Row>[] = [
  { key: "name", header: "Name", cell: (r) => r.name },
  { key: "spend", header: "Spend", numeric: true, cell: (r) => `$${r.spend}` },
];

describe("DataTable", () => {
  it("renders headers, rows, and cells", () => {
    render(<DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />);
    expect(screen.getByText("Name")).toBeInTheDocument();
    expect(screen.getByText("Alpha")).toBeInTheDocument();
    expect(screen.getByText("$34")).toBeInTheDocument();
  });

  it("falls back to default EmptyState when rows are empty and not loading", () => {
    render(<DataTable columns={COLUMNS} rows={[]} rowKey={(r) => r.id} />);
    expect(screen.getByText(/nothing to show yet/i)).toBeInTheDocument();
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("renders a custom empty node when supplied", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={[]}
        rowKey={(r) => r.id}
        empty={<span>nope</span>}
      />,
    );
    expect(screen.getByText("nope")).toBeInTheDocument();
  });

  it("renders the table with aria-busy when loading even with no rows", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={[]}
        rowKey={(r) => r.id}
        isLoading
      />,
    );
    const table = screen.getByRole("table");
    expect(table.getAttribute("aria-busy")).toBe("true");
  });

  it("invokes onRowClick on click and on Enter / Space keypress", () => {
    const onRowClick = vi.fn();
    render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(r) => r.id}
        onRowClick={onRowClick}
      />,
    );
    const row = screen.getByText("Alpha").closest("tr")!;
    fireEvent.click(row);
    fireEvent.keyDown(row, { key: "Enter" });
    fireEvent.keyDown(row, { key: " " });
    fireEvent.keyDown(row, { key: "Tab" });
    expect(onRowClick).toHaveBeenCalledTimes(3);
    expect(row.getAttribute("role")).toBeNull();
    expect(row.getAttribute("tabindex")).toBe("0");
  });

  it("applies numeric and hideOnMobile cell modifiers", () => {
    const cols: DataTableColumn<Row>[] = [
      { key: "name", header: "Name", cell: (r) => r.name, hideOnMobile: true },
      { key: "spend", header: "Spend", numeric: true, cell: (r) => r.spend, ariaSort: "ascending" },
    ];
    render(<DataTable columns={cols} rows={ROWS} rowKey={(r) => r.id} />);
    const headers = screen.getAllByRole("columnheader");
    expect(headers[0].className).toContain("hidden");
    expect(headers[1].className).toContain("text-right");
    expect(headers[1].getAttribute("aria-sort")).toBe("ascending");
  });

  it("renders a sr-only caption when caption is supplied", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(r) => r.id}
        caption="Spend by zombie"
      />,
    );
    const cap = screen.getByText("Spend by zombie");
    expect(cap.tagName).toBe("CAPTION");
    expect(cap.className).toContain("sr-only");
  });

  it("merges a custom className onto the wrapper", () => {
    const { container } = render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(r) => r.id}
        className="my-table"
      />,
    );
    const wrapper = container.querySelector('[data-slot="data-table"]') as HTMLElement;
    expect(wrapper.className).toContain("my-table");
    expect(wrapper.className).toContain("overflow-x-auto");
  });
});
