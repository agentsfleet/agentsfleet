import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DataTable } from "./DataTable";
import { Dialog, DialogContent, DialogTitle } from "./Dialog";

const ROWS = [{ id: "one", name: "First item" }];

describe("DataTable focus and cell identity", () => {
  it("opens a row from its own keyboard target without intercepting nested controls", () => {
    const openRow = vi.fn();
    const edit = vi.fn();
    render(<DataTable rows={ROWS} rowKey={(row) => row.id} onRowClick={openRow} columns={[
      {key: "name", header: "Name", cell: (row) => <a href="#workspace">{row.name}</a>},
      {key: "edit", header: "Actions", cell: () => <button onClick={edit}>Edit</button>},
    ]} />);
    const link = screen.getByRole("link", {name: "First item"});
    expect(fireEvent.keyDown(link, {key: "Enter"})).toBe(true);
    fireEvent.click(link);
    fireEvent.keyDown(screen.getByRole("button", {name: "Edit"}), {key: " "});
    fireEvent.click(screen.getByRole("button", {name: "Edit"}));
    expect(edit).toHaveBeenCalledOnce();
    expect(openRow).not.toHaveBeenCalled();
    const row = screen.getAllByRole("row")[1];
    fireEvent.keyDown(row, {key: "Enter"});
    fireEvent.keyDown(row, {key: " "});
    fireEvent.click(row);
    expect(openRow).toHaveBeenCalledTimes(3);
    expect(openRow).toHaveBeenLastCalledWith(ROWS[0]);
  });

  it("keeps an unsaved edit and focus when sorting moves its row", () => {
    const rows = [{id: "beta", name: "Beta"}, {id: "alpha", name: "Alpha"}];
    render(<DataTable rows={rows} rowKey={(row) => row.id} columns={[
      {key: "name", header: "Name", sortValue: (row) => row.name,
        cell: (row) => <input aria-label={row.name} defaultValue={row.name} />},
    ]} />);
    const input = screen.getByRole("textbox", {name: "Beta"});
    input.focus();
    fireEvent.change(input, {target: {value: "Unsubmitted edit"}});
    fireEvent.click(screen.getByRole("button", {name: "Name"}));
    expect(screen.getAllByRole("textbox")[0]).toHaveAccessibleName("Alpha");
    expect(screen.getByRole("textbox", {name: "Beta"})).toBe(input);
    expect(input).toHaveValue("Unsubmitted edit");
    expect(input).toHaveFocus();
  });

  it("refuses page and sort changes while a mutation is pending", () => {
    const rows = [{id: "one", name: "First"}, {id: "two", name: "Second"}];
    const view = render(<DataTable rows={rows} rowKey={(row) => row.id} isLoading
      pagination={{pageSize: 1}} columns={[{key: "name", header: "Name", sortValue: (row) => row.name, cell: (row) => row.name}]} />);
    expect(screen.getByRole("button", {name: "Next page"})).toBeDisabled();
    expect(screen.getByRole("button", {name: "Name"})).toBeDisabled();
    expect(screen.getByRole("combobox", {name: "Rows per page"})).toBeDisabled();
    view.rerender(<DataTable rows={rows} rowKey={(row) => row.id} pagination={{pageSize: 1}}
      columns={[{key: "name", header: "Name", sortValue: (row) => row.name, cell: (row) => row.name}]} />);
    fireEvent.click(screen.getByRole("button", {name: "Next page"}));
    expect(screen.getByText("Second")).toBeTruthy();
    expect(screen.queryByText("First")).toBeNull();
  });

  it("keeps an edited field mounted when column render callbacks change", () => {
    function EditableTable({ label }: { label: string }) {
      return <DataTable rows={ROWS} rowKey={(row) => row.id} columns={[
        { key: "name", header: label, cell: (row) => <input aria-label="Name" defaultValue={row.name} /> },
      ]} />;
    }
    const view = render(<EditableTable label="Name" />);
    const input = screen.getByRole("textbox", { name: "Name" });
    input.focus();
    fireEvent.change(input, { target: { value: "Unsubmitted edit" } });
    view.rerender(<EditableTable label="Updated label" />);
    expect(screen.getByRole("textbox", { name: "Name" })).toBe(input);
    expect(input).toHaveValue("Unsubmitted edit");
    expect(input).toHaveFocus();
  });

  it("returns focus to a table action after its controlled dialog closes", async () => {
    function ActionTable() {
      const [open, setOpen] = useState(false);
      return <>
        <DataTable rows={ROWS} rowKey={(row) => row.id} columns={[
          { key: "actions", header: "Actions", cell: () => <button onClick={() => setOpen(true)}>Edit item</button> },
        ]} />
        <Dialog open={open} onOpenChange={setOpen}>
          <DialogContent><DialogTitle>Edit item</DialogTitle></DialogContent>
        </Dialog>
      </>;
    }
    render(<ActionTable />);
    const opener = screen.getByRole("button", { name: "Edit item" });
    opener.focus();
    fireEvent.click(opener);
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(opener).toHaveFocus());
    expect(screen.getByRole("button", { name: "Edit item" })).toBe(opener);
  });
});
