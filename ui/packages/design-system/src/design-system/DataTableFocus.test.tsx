import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DataTable } from "./DataTable";
import { Dialog, DialogContent, DialogTitle } from "./Dialog";

const ROWS = [{ id: "one", name: "First item" }];

describe("DataTable focus and cell identity", () => {
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
