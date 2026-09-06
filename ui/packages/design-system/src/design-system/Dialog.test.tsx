import { describe, it, expect } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "./Dialog";

describe("Dialog", () => {
  it("returns focus to an external opener after a controlled dialog closes", async () => {
    function ControlledDialog() {
      const [open, setOpen] = useState(false);
      return <>
        <button onClick={() => setOpen(true)}>Edit item</button>
        <Dialog open={open} onOpenChange={setOpen}>
          <DialogContent>
            <DialogTitle>Edit item</DialogTitle>
            <DialogDescription>Update this item.</DialogDescription>
            <input aria-label="Name" />
          </DialogContent>
        </Dialog>
      </>;
    }
    render(<ControlledDialog />);
    const opener = screen.getByRole("button", { name: "Edit item" });
    opener.focus();
    fireEvent.click(opener);
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it("preserves caller-defined focus targets on open and close", async () => {
    render(<>
      <button>Return here</button>
      <Dialog>
        <DialogTrigger>Open</DialogTrigger>
        <DialogContent
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            screen.getByRole("textbox", { name: "Second field" }).focus();
          }}
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            screen.getByRole("button", { name: "Return here" }).focus();
          }}
        >
          <DialogTitle>Edit</DialogTitle>
          <input aria-label="First field" />
          <input aria-label="Second field" />
        </DialogContent>
      </Dialog>
    </>);
    fireEvent.click(screen.getByText("Open"));
    expect(screen.getByRole("textbox", { name: "Second field" })).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.getByText("Return here")).toHaveFocus());
  });

  it("falls back to the Radix trigger when focus starts on the body", async () => {
    render(<Dialog>
      <DialogTrigger>Open</DialogTrigger>
      <DialogContent><DialogTitle>Edit</DialogTitle></DialogContent>
    </Dialog>);
    expect(document.body).toHaveFocus();
    fireEvent.click(screen.getByText("Open"));
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.getByText("Open")).toHaveFocus());
  });

  it("does not focus an opener removed while the dialog is open", async () => {
    const opener = document.createElement("button");
    document.body.append(opener);
    opener.focus();
    render(<Dialog defaultOpen>
      <DialogContent><DialogTitle>Edit</DialogTitle></DialogContent>
    </Dialog>);
    opener.remove();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(document.body).toHaveFocus());
  });

  it("is closed by default — trigger present, content not rendered", () => {
    render(
      <Dialog>
        <DialogTrigger>Open</DialogTrigger>
        <DialogContent>
          <DialogTitle>Title</DialogTitle>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.getByText("Open")).toBeInTheDocument();
    expect(screen.queryByText("Title")).not.toBeInTheDocument();
  });

  it("opens the content when the trigger is clicked", () => {
    render(
      <Dialog>
        <DialogTrigger>Open</DialogTrigger>
        <DialogContent>
          <DialogTitle>Confirm</DialogTitle>
          <DialogDescription>Are you sure?</DialogDescription>
        </DialogContent>
      </Dialog>,
    );
    fireEvent.click(screen.getByText("Open"));
    expect(screen.getByText("Confirm")).toBeInTheDocument();
    expect(screen.getByText("Are you sure?")).toBeInTheDocument();
  });

  it("respects the open prop (controlled mode)", () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogTitle>Always open</DialogTitle>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.getByText("Always open")).toBeInTheDocument();
  });

  it("renders a close button with an accessible sr-only label", () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogTitle>X</DialogTitle>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.getByText("Close")).toBeInTheDocument();
  });

  it("DialogContent applies surface utilities", () => {
    render(
      <Dialog open>
        <DialogContent data-testid="content">
          <DialogTitle>T</DialogTitle>
        </DialogContent>
      </Dialog>,
    );
    const cls = screen.getByTestId("content").className;
    expect(cls).toContain("bg-card");
    expect(cls).toContain("border-border-strong");
    expect(cls).toContain("rounded-lg");
  });

  // Regression: a fixed, centre-translated panel is unreachable by page scroll,
  // so a dialog taller than the viewport used to bury its own footer — the
  // submit button rendered, was "visible and enabled", and could never be
  // clicked. The cap plus in-panel scrolling is what keeps every footer
  // reachable, so it is asserted rather than left to survive by accident.
  it("DialogContent caps itself at the viewport and scrolls its own overflow", () => {
    render(
      <Dialog open>
        <DialogContent data-testid="content">
          <DialogTitle>T</DialogTitle>
        </DialogContent>
      </Dialog>,
    );
    const cls = screen.getByTestId("content").className;
    expect(cls).toContain("max-h-[calc(100dvh-var(--spacing-3xl))]");
    expect(cls).toContain("overflow-y-auto");
  });

  it("DialogHeader/Footer apply layout utilities", () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogHeader data-testid="h">
            <DialogTitle>T</DialogTitle>
          </DialogHeader>
          <DialogFooter data-testid="f">Actions</DialogFooter>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.getByTestId("h").className).toContain("flex-col");
    expect(screen.getByTestId("f").className).toContain("justify-end");
  });

  it("DialogDescription uses muted-foreground text", () => {
    render(
      <Dialog open>
        <DialogContent>
          <DialogTitle>T</DialogTitle>
          <DialogDescription data-testid="d">Body</DialogDescription>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.getByTestId("d").className).toContain("text-muted-foreground");
  });

  it("forwards refs on the Content primitive", () => {
    const ref = { current: null as HTMLDivElement | null };
    render(
      <Dialog open>
        <DialogContent ref={ref}>
          <DialogTitle>T</DialogTitle>
        </DialogContent>
      </Dialog>,
    );
    expect(ref.current).toBeInstanceOf(HTMLElement);
  });

  it("merges custom className on content", () => {
    render(
      <Dialog open>
        <DialogContent className="max-w-md" data-testid="c">
          <DialogTitle>T</DialogTitle>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.getByTestId("c").className).toContain("max-w-md");
  });
});
