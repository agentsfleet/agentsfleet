import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DataTable, type DataTableColumn } from "./DataTable";
import { ROWS, COLUMNS, type Row } from "./DataTable.test-fixture";

describe("DataTable", () => {
  it("does not activate an empty row and spans its navigation column", () => {
    const onRowClick = vi.fn();
    render(<DataTable columns={COLUMNS} rows={[]} rowKey={(r) => r.id}
      onRowClick={onRowClick} isLoading empty={<span>No activity</span>} />);
    const empty = screen.getByRole("cell", { name: "No activity" });
    expect(empty).toHaveAttribute("colspan", "3");
    expect(screen.getAllByRole("columnheader")).toHaveLength(3);
    fireEvent.click(empty);
    expect(onRowClick).not.toHaveBeenCalled();
  });

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
    expect(row).toHaveRole("row");
    expect(screen.getByRole("table")).toContainElement(row);
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

  it("is unbounded by default, so a paginated table lets the page scroll", () => {
    const { container } = render(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />,
    );
    const viewport = container.querySelector('[role="region"]') as HTMLElement;
    expect(viewport.className).toContain("overflow-y-auto");
    expect(viewport.className).toContain("motion-safe:scroll-smooth");
    // No height bound of any size. The retired max-h-96 default opened a
    // second scroll region whose scrollbar sat inside the cell padding, so on
    // exactly the tables with more rows than the box the header text and row
    // actions moved 6px in from the right while the footer stayed put.
    expect(viewport.className).not.toMatch(/\bmax-h-/);
    expect(viewport.querySelector("table")).toBeInTheDocument();
    const thead = container.querySelector("thead") as HTMLElement;
    expect(thead.className).toContain("sticky");
    expect(thead.className).toContain("top-0");
  });

  it("bounds the viewport only when the consumer passes a height", () => {
    const { container } = render(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} viewportClassName="max-h-72" />,
    );

    expect(container.querySelector(".max-h-72.overflow-y-auto")).toBeInTheDocument();
    expect(container.querySelector(".max-h-96")).toBeNull();
  });

  // `stickyHeader` governs whether the header PINS, not whether rows scroll.
  // The rows always scroll: a reader who has to move the whole page to reach
  // row 40 loses the column headings that make row 40 mean anything, and the
  // page-scrolls-instead default left six of ten tables doing exactly that.
  it("stickyHeader=false removes the pinned header but keeps the scroll region", () => {
    const { container } = render(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} stickyHeader={false} />,
    );
    const viewport = container.querySelector('[role="region"]') as HTMLElement;
    expect(viewport.className).toContain("overflow-y-auto");
    const thead = container.querySelector("thead") as HTMLElement;
    expect(thead.className).not.toContain("sticky");
  });

  it("contains overscroll on the horizontal axis only", () => {
    render(<DataTable columns={COLUMNS} rows={ROWS} rowKey={(r) => r.id} />);
    const viewport = screen.getByRole("region", { name: "Scrollable table" });
    const classes = viewport.className.split(/\s+/);

    // `overflow-x-auto` makes this a scroll container on BOTH axes, so a
    // two-axis containment swallows the wheel even with nothing here to
    // scroll vertically. On a surface that grows with the page rather than
    // bounding the table, that leaves the page unscrollable with the pointer
    // anywhere over the rows.
    expect(classes).toContain("overscroll-x-contain");
    expect(classes).not.toContain("overscroll-contain");
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
});
