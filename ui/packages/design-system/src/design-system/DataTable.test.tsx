import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { DataTable, type DataTableColumn } from "./DataTable";

type Row = { id: string; name: string; spend: number };

const ROWS: Row[] = [
  { id: "a", name: "Alpha", spend: 12 },
  { id: "b", name: "Bravo", spend: 34 },
];

const COLUMNS: DataTableColumn<Row>[] = [
  { key: "name", header: "Name", cell: (r) => r.name, sortValue: (r) => r.name },
  { key: "spend", header: "Spend", numeric: true, cell: (r) => `$${r.spend}`, sortValue: (r) => r.spend },
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

  it("keeps an empty externally paginated state stable while its page loads", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={[]}
        rowKey={(row) => row.id}
        pagination={{
          kind: "page",
          page: 2,
          pageSize: 25,
          total: 26,
          onPageChange: vi.fn(),
          isLoading: true,
        }}
      />,
    );

    expect(screen.getByText("Nothing to show yet")).toBeInTheDocument();
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.getByRole("navigation", { name: "Pagination" })).toHaveAttribute("aria-busy", "true");
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
      { key: "spend", header: "Spend", numeric: true, cell: (r) => r.spend },
    ];
    render(<DataTable columns={cols} rows={ROWS} rowKey={(r) => r.id} />);
    const headers = screen.getAllByRole("columnheader");
    expect(headers[0].className).toContain("hidden");
    expect(headers[1].className).toContain("text-right");
    expect(headers[1].getAttribute("aria-sort")).toBeNull();
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

  it("renders a sr-only caption when caption is supplied", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(r) => r.id}
        caption="Spend by agent"
      />,
    );
    const cap = screen.getByText("Spend by agent");
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
    expect(wrapper.className).toContain("overflow-hidden");
  });

  it("keeps dense cells while giving sortable headers the standard button target", () => {
    render(<DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />);
    const headerCell = screen.getAllByRole("columnheader")[0];
    const headerButton = screen.getByRole("button", { name: "Name" });
    const bodyCell = screen.getByText("Alpha").closest("td")!;
    expect(headerCell.className).toContain("p-0");
    expect(headerButton.className).toContain("h-8");
    expect(headerButton.className).toContain("focus-visible:ring-inset");
    expect(bodyCell.className).toContain("py-1.5");
    expect(bodyCell.className).not.toContain("py-2");
  });

  it("bounds the table and pins its header by default", () => {
    const { container } = render(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />,
    );
    const scrollDiv = container.querySelector(".max-h-96.overflow-y-auto");
    expect(scrollDiv).toBeInTheDocument();
    expect(scrollDiv?.className).toContain("motion-safe:scroll-smooth");
    expect(scrollDiv?.querySelector("table")).toBeInTheDocument();
    const thead = container.querySelector("thead") as HTMLElement;
    expect(thead.className).toContain("sticky");
    expect(thead.className).toContain("top-0");
  });

  it("uses an explicit viewport height instead of the default cap", () => {
    const { container } = render(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} viewportClassName="max-h-72" />,
    );

    expect(container.querySelector(".max-h-72.overflow-y-auto")).toBeInTheDocument();
    expect(container.querySelector(".max-h-96")).toBeNull();
  });

  it("stickyHeader=false removes the height bound and pinned header", () => {
    const { container } = render(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} stickyHeader={false} />,
    );
    expect(container.querySelector(".max-h-96")).toBeNull();
    const thead = container.querySelector("thead") as HTMLElement;
    expect(thead.className).not.toContain("sticky");
  });

  it("the scroll region is keyboard-reachable (tabIndex + region role)", () => {
    render(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} caption="Spend by agent" />,
    );
    const region = screen.getByRole("region", { name: /spend by agent, scrollable/i });
    expect(region.getAttribute("tabindex")).toBe("0");
  });

  it("a table without a caption falls back to a generic scrollable-region label", () => {
    render(<DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />);
    expect(screen.getByRole("region", { name: /scrollable table/i })).toBeInTheDocument();
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
    expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
    expect(screen.getByText("Row 1")).toBeInTheDocument();

    rerender(<DataTable columns={COLUMNS} rows={rows} rowKey={(row) => row.id} />);
    expect(screen.getByText("Page 1 of 2 · 26 items")).toBeInTheDocument();
    expect(screen.getByText("Row 1")).toBeInTheDocument();
    expect(screen.queryByText("Row 26")).toBeNull();
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

  it("omits pagination chrome for one local page", () => {
    render(<DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />);
    expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
  });

  it("omits pagination chrome at the exact default page-size boundary", () => {
    const rows = Array.from({ length: 25 }, (_, index) => ({
      id: String(index),
      name: `Row ${index}`,
      spend: index,
    }));
    render(<DataTable columns={COLUMNS} rows={rows} rowKey={(row) => row.id} />);

    expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
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

  it("renders every row when pagination is explicitly disabled", () => {
    const rows = Array.from({ length: 26 }, (_, index) => ({ id: String(index), name: `Row ${index}`, spend: index }));
    render(<DataTable columns={COLUMNS} rows={rows} rowKey={(row) => row.id} pagination={false} />);
    expect(screen.getByText("Row 25")).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
  });

  it("forwards page-backed pagination without slicing server rows", () => {
    const onPageChange = vi.fn();
    render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(row) => row.id}
        pagination={{ kind: "page", page: 2, pageSize: 2, total: 6, onPageChange }}
      />,
    );
    expect(screen.getByText("Page 2 of 3 · 6 items")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(onPageChange).toHaveBeenCalledWith(3);
  });

  it("resets the bounded viewport when a numeric server page changes", () => {
    const pagination = { kind: "page" as const, pageSize: 2, total: 6, onPageChange: vi.fn() };
    const { rerender } = render(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(row) => row.id} pagination={{ ...pagination, page: 1 }} />,
    );
    const viewport = screen.getByRole("region", { name: "Scrollable table" });
    viewport.scrollTop = 120;

    rerender(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(row) => row.id} pagination={{ ...pagination, page: 2 }} />,
    );

    expect(viewport.scrollTop).toBe(0);
  });

  it("omits server pagination when the first page contains the full result", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(row) => row.id}
        pagination={{ kind: "page", page: 1, pageSize: 25, total: 2, onPageChange: vi.fn() }}
      />,
    );

    expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
  });

  it("disables sorting while an external page is loading", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(row) => row.id}
        onSortChange={vi.fn()}
        pagination={{
          kind: "page",
          page: 1,
          pageSize: 25,
          total: 26,
          onPageChange: vi.fn(),
          isLoading: true,
        }}
      />,
    );

    expect(screen.getByRole("button", { name: "Name" })).toBeDisabled();
  });

  it("keeps page navigation reachable when a server page is empty", () => {
    const onPageChange = vi.fn();
    render(
      <DataTable
        columns={COLUMNS}
        rows={[]}
        rowKey={(row) => row.id}
        empty={<div>No rows on this page</div>}
        pagination={{ kind: "page", page: 2, pageSize: 25, total: 26, onPageChange }}
      />,
    );

    expect(screen.getByText("No rows on this page")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
    expect(onPageChange).toHaveBeenCalledWith(1);
  });

  it("pages a cursor-backed feed and stops when the caller says the feed ended", () => {
    const onPageChange = vi.fn();
    const { rerender } = render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(row) => row.id}
        pagination={{ kind: "page", page: 1, pageSize: 25, hasNext: true, onPageChange }}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(onPageChange).toHaveBeenCalledWith(2);

    rerender(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(row) => row.id}
        pagination={{ kind: "page", page: 1, pageSize: 25, hasNext: false, onPageChange }}
      />,
    );
    // Page 1 of a feed with nothing after it needs no pager at all.
    expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
  });

  it("keeps the pager reachable when an intermediate page comes back empty", () => {
    const onPageChange = vi.fn();
    render(
      <DataTable
        columns={COLUMNS}
        rows={[]}
        rowKey={(row) => row.id}
        empty={<div>No rows on this page</div>}
        pagination={{ kind: "page", page: 2, pageSize: 25, hasNext: true, onPageChange }}
      />,
    );

    expect(screen.getByText("No rows on this page")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
    expect(onPageChange).toHaveBeenCalledWith(1);
  });
});
