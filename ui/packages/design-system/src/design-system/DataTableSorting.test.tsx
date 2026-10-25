import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { DataTable, type DataTableColumn } from "./DataTable";
import { ROWS, COLUMNS, type Row } from "./DataTable.test-fixture";

describe("DataTable", () => {
  it("sorts numeric amounts in both directions", () => {
    const rows = [ROWS[0], {...ROWS[1], spend: 3}];
    render(<DataTable columns={COLUMNS} rows={rows} rowKey={(r) => r.id} />);
    const sort = screen.getByRole("button", { name: "Spend" });
    fireEvent.click(sort);
    expect(within(screen.getAllByRole("row")[1]).getByText("$3")).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Spend" })).toHaveAttribute("aria-sort", "ascending");
    fireEvent.click(sort);
    expect(within(screen.getAllByRole("row")[1]).getByText("$12")).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Spend" })).toHaveAttribute("aria-sort", "descending");
  });

  it("puts a right-aligned header's sort icon after its label", () => {
    const cols: DataTableColumn<Row>[] = [
      { key: "spend", header: "Spend", numeric: true, sortValue: (r) => r.spend, cell: (r) => r.spend },
    ];
    render(<DataTable columns={cols} rows={ROWS} rowKey={(r) => r.id} />);
    const sort = screen.getByRole("button", { name: /Spend/ });
    expect(sort.className).toContain("justify-end");
    expect(sort.className).not.toContain("flex-row-reverse");
    // The label is the first child and the icon the last, and nothing reverses them.
    expect(sort.firstChild?.textContent).toBe("Spend");
    expect(sort.lastChild?.nodeName.toLowerCase()).toBe("svg");
  });

  it("renders a sortable header's aria-sort from sortKey/sortDirection, not per-column state", () => {
    const cols: DataTableColumn<Row>[] = [
      { key: "name", header: "Name", cell: (r) => r.name, sortable: true },
      { key: "spend", header: "Spend", numeric: true, cell: (r) => r.spend, sortable: true },
    ];
    render(
      <DataTable
        columns={cols}
        rows={ROWS}
        rowKey={(r) => r.id}
        sortKey="spend"
        sortDirection="ascending"
        onSortChange={vi.fn()}
      />,
    );
    const headers = screen.getAllByRole("columnheader");
    expect(headers[0].getAttribute("aria-sort")).toBe("none");
    expect(headers[1].getAttribute("aria-sort")).toBe("ascending");
  });

  it("clicking a sortable header reports its column key via onSortChange", () => {
    const onSortChange = vi.fn();
    const cols: DataTableColumn<Row>[] = [
      { key: "name", header: "Name", cell: (r) => r.name, sortable: true },
      { key: "spend", header: "Spend", numeric: true, cell: (r) => r.spend },
    ];
    render(
      <DataTable
        columns={cols}
        rows={ROWS}
        rowKey={(r) => r.id}
        sortKey="name"
        sortDirection="ascending"
        onSortChange={onSortChange}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /name/i }));
    expect(onSortChange).toHaveBeenCalledWith("name");
    expect(screen.queryByRole("button", { name: /spend/i })).toBeNull();
  });

  it("keeps externally sorted columns usable without exposing a TanStack accessor", () => {
    const onSortChange = vi.fn();
    const cols: DataTableColumn<Row>[] = [
      { key: "remote_name", header: "Remote name", cell: (row) => row.name, sortable: true },
    ];
    render(
      <DataTable
        columns={cols}
        rows={ROWS}
        rowKey={(row) => row.id}
        onSortChange={onSortChange}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Remote name" }));
    expect(onSortChange).toHaveBeenCalledWith("remote_name");
  });

  it("does not advertise local sorting without a sort value", () => {
    const cols: DataTableColumn<Row>[] = [
      { key: "remote_name", header: "Remote name", cell: (row) => row.name, sortable: true },
    ];
    render(<DataTable columns={cols} rows={ROWS} rowKey={(row) => row.id} />);

    expect(screen.queryByRole("button", { name: "Remote name" })).toBeNull();
    expect(screen.getByRole("columnheader", { name: "Remote name" })).not.toHaveAttribute("aria-sort");
  });

  it("lets sortable false override a supplied sort value", () => {
    const cols: DataTableColumn<Row>[] = [
      {
        key: "name",
        header: "Name",
        cell: (row) => row.name,
        sortValue: (row) => row.name,
        sortable: false,
      },
    ];
    render(<DataTable columns={cols} rows={ROWS} rowKey={(row) => row.id} />);

    expect(screen.queryByRole("button", { name: "Name" })).toBeNull();
  });

  it("cycles local rows through ascending, descending, and original order", () => {
    render(<DataTable columns={COLUMNS} rows={[ROWS[1], ROWS[0]]} rowKey={(r) => r.id} />);
    const button = screen.getByRole("button", { name: /name/i });

    fireEvent.click(button);
    expect(screen.getByRole("columnheader", { name: /name/i })).toHaveAttribute("aria-sort", "ascending");
    expect(within(screen.getAllByRole("row")[1]).getByText("Alpha")).toBeInTheDocument();

    fireEvent.click(button);
    expect(screen.getByRole("columnheader", { name: /name/i })).toHaveAttribute("aria-sort", "descending");
    expect(within(screen.getAllByRole("row")[1]).getByText("Bravo")).toBeInTheDocument();

    fireEvent.click(button);
    expect(screen.getByRole("columnheader", { name: /name/i })).toHaveAttribute("aria-sort", "none");
    expect(within(screen.getAllByRole("row")[1]).getByText("Bravo")).toBeInTheDocument();
  });

  it("keeps externally sorted rows in caller order and reports only the column key", () => {
    const onSortChange = vi.fn();
    render(
      <DataTable
        columns={COLUMNS}
        rows={[ROWS[1], ROWS[0]]}
        rowKey={(r) => r.id}
        sortKey="name"
        sortDirection="ascending"
        onSortChange={onSortChange}
      />,
    );
    expect(within(screen.getAllByRole("row")[1]).getByText("Bravo")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /name/i }));
    expect(onSortChange).toHaveBeenCalledWith("name");
  });

  it("resets client pagination when controlled sorting changes", () => {
    const onSortChange = vi.fn();
    const rows = Array.from({ length: 26 }, (_, index) => ({
      id: String(index + 1),
      name: `Row ${index + 1}`,
      spend: index,
    }));
    render(
      <DataTable
        columns={COLUMNS}
        rows={rows}
        rowKey={(row) => row.id}
        onSortChange={onSortChange}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByText("Page 2 of 2 · 26 items")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Name" }));

    expect(onSortChange).toHaveBeenCalledWith("name");
    expect(screen.getByText("Page 1 of 2 · 26 items")).toBeInTheDocument();
  });

  it("should delegate controlled sorting without resetting server pagination", () => {
    const onSortChange = vi.fn();
    const onPageChange = vi.fn();
    render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(row) => row.id}
        sortKey="name"
        sortDirection="ascending"
        onSortChange={onSortChange}
        pagination={{ kind: "page", page: 2, pageSize: 2, total: 6, onPageChange }}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Name" }));

    expect(onSortChange).toHaveBeenCalledWith("name");
    expect(onPageChange).not.toHaveBeenCalled();
    expect(screen.getByText("Page 2 of 3 · 6 items")).toBeInTheDocument();
  });

  it("resets the bounded viewport when local sorting changes", () => {
    render(<DataTable columns={COLUMNS} rows={ROWS} rowKey={(row) => row.id} />);
    const viewport = screen.getByRole("region", { name: "Scrollable table" });
    viewport.scrollTop = 120;

    fireEvent.click(screen.getByRole("button", { name: "Name" }));

    expect(viewport.scrollTop).toBe(0);
  });

  it("should sort with pagination disabled and reset through the browser scroll API", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={[ROWS[1], ROWS[0]]}
        rowKey={(row) => row.id}
        pagination={false}
      />,
    );
    const viewport = screen.getByRole("region", { name: "Scrollable table" });
    const scrollTo = vi.fn();
    Object.defineProperty(viewport, "scrollTo", { configurable: true, value: scrollTo });

    fireEvent.click(screen.getByRole("button", { name: "Name" }));

    expect(within(screen.getAllByRole("row")[1]).getByText("Alpha")).toBeInTheDocument();
    expect(scrollTo).toHaveBeenCalledWith({ top: 0 });
    expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
  });
});
