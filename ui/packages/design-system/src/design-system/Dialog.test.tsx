import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
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
    expect(cls).toContain("border-border");
    expect(cls).toContain("rounded-xl");
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
