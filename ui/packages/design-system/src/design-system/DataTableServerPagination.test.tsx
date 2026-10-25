import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DataTable } from "./DataTable";
import { ROWS, COLUMNS } from "./DataTable.test-fixture";

describe("DataTable", () => {
  // Bug this catches: an empty server-paginated feed that can go nowhere would
  // otherwise render a pager with both buttons dead under a table with no
  // rows — an control that looks broken rather than absent. The footer is
  // suppressed only when there is genuinely nowhere to navigate, which is why
  // an explicit hasNext:false is what makes it disappear.
  it("hides the pager for an empty server page with no navigation available", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={[]}
        rowKey={(row) => row.id}
        isLoading
        pagination={{
          kind: "page",
          page: 1,
          pageSize: 10,
          total: 0,
          hasNext: false,
          onPageChange: vi.fn(),
        }}
      />,
    );

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

  it("resets the bounded viewport when the server page size changes", () => {
    const pagination = { kind: "page" as const, page: 1, total: 6, onPageChange: vi.fn() };
    const { rerender } = render(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(row) => row.id} pagination={{ ...pagination, pageSize: 2 }} />,
    );
    const viewport = screen.getByRole("region", { name: "Scrollable table" });
    viewport.scrollTop = 120;

    rerender(
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={(row) => row.id} pagination={{ ...pagination, pageSize: 25 }} />,
    );

    expect(viewport.scrollTop).toBe(0);
  });

  it("should keep server table controls visible when the first page contains the full result", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        rowKey={(row) => row.id}
        pagination={{ kind: "page", page: 1, pageSize: 25, total: 2, onPageChange: vi.fn() }}
      />,
    );

    expect(screen.getByText("Page 1 of 1 · 2 items")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
  });

  it("should hide empty first-page cursor controls when the caller reports no next page", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={[]}
        rowKey={(row) => row.id}
        pagination={{
          kind: "page",
          page: 1,
          pageSize: 25,
          hasNext: false,
          onPageChange: vi.fn(),
        }}
      />,
    );

    expect(screen.getByText("Nothing to show yet")).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Pagination" })).toBeNull();
  });

  it("should keep empty first-page controls when the total proves another page exists", () => {
    render(
      <DataTable
        columns={COLUMNS}
        rows={[]}
        rowKey={(row) => row.id}
        pagination={{
          kind: "page",
          page: 1,
          pageSize: 25,
          total: 26,
          onPageChange: vi.fn(),
        }}
      />,
    );

    expect(screen.getByText("Nothing to show yet")).toBeInTheDocument();
    expect(screen.getByText("Page 1 of 2 · 26 items")).toBeInTheDocument();
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
    expect(screen.getByRole("navigation", { name: "Pagination" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
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
