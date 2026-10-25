import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DATA_TABLE_SURFACE, DataTable } from "./DataTable";

const ROWS = [{id: "one", name: "First item"}, {id: "two", name: "Second item"}];
const columns = [{key: "name", header: "Name", cell: (row: typeof ROWS[number]) => row.name}];
const DETAIL = "Unsaved detail";

function DisclosureTable({loading = false, heading = "Name"}: {loading?: boolean; heading?: string}) {
  const [expandedRow, onExpandedRowChange] = useState("");
  return <DataTable rows={ROWS} rowKey={row => row.id} surface={DATA_TABLE_SURFACE.plain} isLoading={loading} pagination={false}
    columns={[{...columns[0], header: heading}]} rowDisclosure={{expandedRow, onExpandedRowChange,
      label: row => `Details for ${row.name}`, content: row => <input aria-label={DETAIL} defaultValue={row.name} />}} />;
}

describe("plain DataTable row disclosure", () => {
  it("keeps disclosure separate from row navigation and spans both columns", () => {
    const onRowClick = vi.fn();
    const onExpandedRowChange = vi.fn();
    render(<DataTable rows={ROWS} columns={columns} rowKey={row => row.id} pagination={false}
      onRowClick={onRowClick} rowDisclosure={{expandedRow: ROWS[0].id, onExpandedRowChange,
        label: row => row.name, content: () => DETAIL}} />);
    const detail = screen.getByRole("region", {name: ROWS[0].name});
    expect(detail.closest("td")).toHaveAttribute("colspan", "2");
    expect(detail).toHaveTextContent(DETAIL);
    fireEvent.click(screen.getByRole("button", {name: ROWS[0].name}));
    expect(onExpandedRowChange).toHaveBeenCalledWith("");
    expect(onRowClick).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText(ROWS[0].name).closest("tr")!);
    expect(onRowClick).toHaveBeenCalledExactlyOnceWith(ROWS[0]);
    expect(onExpandedRowChange).toHaveBeenCalledTimes(1);
  });

  it("retains table headings for an empty plain surface", () => {
    render(<DataTable rows={[]} rowKey={row => row.id} columns={columns} surface={DATA_TABLE_SURFACE.plain} caption="Activity" empty={<p>No settled activity</p>} />);
    expect(screen.getByRole("table", {name: "Activity"})).toBeTruthy();
    expect(screen.getByRole("columnheader", {name: "Name"})).toBeTruthy();
    expect(screen.getByText("No settled activity").closest("td")).toHaveAttribute("colspan", "1");
    expect(screen.getByTestId("data-table")).toHaveAttribute("data-surface", DATA_TABLE_SURFACE.plain);
    expect(screen.getByTestId("data-table")).not.toHaveClass("border");
  });

  it("does not render details or change controlled state before the owner accepts it", () => {
    const changed = vi.fn();
    const content = vi.fn(() => <p>Private charges</p>);
    render(<DataTable rows={ROWS} rowKey={row => row.id} columns={columns} pagination={false}
      rowDisclosure={{expandedRow: "", onExpandedRowChange: changed, label: row => row.name, content}} />);
    const button = screen.getByRole("button", {name: ROWS[0].name});
    fireEvent.click(button);
    expect(changed).toHaveBeenCalledWith(ROWS[0].id);
    expect(button).toHaveAttribute("aria-expanded", "false");
    expect(content).not.toHaveBeenCalled();
    expect(screen.queryByText("Private charges")).toBeNull();
  });

  it("exposes one labelled detail region and collapses through the same control", () => {
    render(<DisclosureTable />);
    const first = screen.getByRole("button", {name: /First item/});
    first.focus();
    fireEvent.click(first);
    expect(first).toHaveAttribute("aria-expanded", "true");
    const region = screen.getByRole("region", {name: /First item/});
    expect(region.id).toBe(first.getAttribute("aria-controls"));
    expect(first).toHaveFocus();
    fireEvent.click(screen.getByRole("button", {name: /Second item/}));
    expect(screen.queryByRole("region", {name: /First item/})).toBeNull();
    expect(screen.getByRole("textbox", {name: DETAIL})).toHaveValue(ROWS[1].name);
    fireEvent.click(screen.getByRole("button", {name: /Second item/}));
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("does not change disclosure while loading and becomes usable after loading", () => {
    const view = render(<DisclosureTable loading />);
    const first = screen.getByRole("button", {name: /First item/});
    expect(first).toBeDisabled();
    fireEvent.click(first);
    fireEvent.click(screen.getAllByRole("row")[1]);
    expect(screen.queryByRole("textbox")).toBeNull();
    view.rerender(<DisclosureTable />);
    fireEvent.click(first);
    expect(screen.getByRole("textbox", {name: DETAIL})).toHaveValue(ROWS[0].name);
  });

  it("keeps detail edits and focus when the parent recreates cell callbacks", () => {
    const view = render(<DisclosureTable />);
    fireEvent.click(screen.getByRole("button", {name: /First item/}));
    const input = screen.getByRole("textbox", {name: DETAIL});
    input.focus();
    fireEvent.change(input, {target: {value: "Still editing"}});
    view.rerender(<DisclosureTable heading="Updated name" />);
    expect(screen.getByRole("textbox", {name: DETAIL})).toBe(input);
    expect(input).toHaveValue("Still editing");
    expect(input).toHaveFocus();
  });

  it("does not let a nested action or its keyboard event toggle a row", () => {
    const changed = vi.fn();
    render(<DataTable rows={ROWS} rowKey={row => row.id} pagination={false} columns={[
      ...columns, {key: "action", header: "Action", cell: () => <button>Copy</button>},
    ]} rowDisclosure={{expandedRow: "", onExpandedRowChange: changed, label: row => row.name, content: () => DETAIL}} />);
    const copy = screen.getAllByRole("button", {name: "Copy"})[0];
    fireEvent.click(copy);
    fireEvent.keyDown(copy, {key: "Enter"});
    expect(changed).not.toHaveBeenCalled();
    expect(screen.queryByRole("region", {name: ROWS[0].name})).toBeNull();
  });

  it("does not expose another page's selected detail after rows change", () => {
    const props = {columns, rowKey: (row: typeof ROWS[number]) => row.id, pagination: false as const,
      rowDisclosure: {expandedRow: ROWS[0].id, onExpandedRowChange: vi.fn(), label: (row: typeof ROWS[number]) => row.name, content: () => DETAIL}};
    const view = render(<DataTable {...props} rows={ROWS} />);
    expect(screen.getByRole("region", {name: ROWS[0].name})).toHaveTextContent(DETAIL);
    view.rerender(<DataTable {...props} rows={[ROWS[1]]} />);
    expect(screen.queryByText(DETAIL)).toBeNull();
    expect(screen.getByRole("button", {name: ROWS[1].name})).toHaveAttribute("aria-expanded", "false");
  });
});
