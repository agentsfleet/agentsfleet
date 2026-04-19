import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { ConfirmDialog } from "./ConfirmDialog";

describe("ConfirmDialog", () => {
  it("renders nothing visible when open is false (Radix Dialog portal unmounted)", () => {
    render(
      <ConfirmDialog
        open={false}
        onOpenChange={() => {}}
        title="Stop zombie?"
        onConfirm={() => {}}
      />,
    );
    expect(screen.queryByText("Stop zombie?")).not.toBeInTheDocument();
  });

  it("renders title + description when open (alertdialog role + aria-describedby)", () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="Stop zombie?"
        description="This will halt the agent."
        onConfirm={() => {}}
      />,
    );
    expect(screen.getByText("Stop zombie?")).toBeInTheDocument();
    expect(screen.getByText("This will halt the agent.")).toBeInTheDocument();
    const dlg = screen.getByTestId("confirm-dialog");
    expect(dlg).toHaveAttribute("role", "alertdialog");
    expect(dlg.getAttribute("aria-describedby")).not.toBeNull();
  });

  it("uses the destructive Button variant when intent=destructive", () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="Stop"
        intent="destructive"
        confirmLabel="Stop"
        onConfirm={() => {}}
      />,
    );
    const btn = screen.getByRole("button", { name: "Stop" });
    expect(btn.className).toContain("bg-destructive");
  });

  it("calls onConfirm when the confirm button is clicked", async () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="Stop"
        confirmLabel="Stop"
        onConfirm={onConfirm}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(onConfirm).toHaveBeenCalledTimes(1));
  });

  it("calls onOpenChange(false) when cancel is clicked", () => {
    const onOpenChange = vi.fn();
    render(
      <ConfirmDialog
        open
        onOpenChange={onOpenChange}
        title="X"
        onConfirm={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("surfaces the errorMessage as an alert banner", () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="X"
        onConfirm={() => {}}
        errorMessage="Something failed"
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Something failed");
    expect(alert.className).toContain("text-destructive");
  });

  it("routes rejected onConfirm to onError when provided", async () => {
    const onError = vi.fn();
    const boom = new Error("boom");
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="X"
        confirmLabel="Do it"
        onConfirm={() => {
          throw boom;
        }}
        onError={onError}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Do it" }));
    await waitFor(() => expect(onError).toHaveBeenCalledWith(boom));
  });

  it("SSR renders nothing visible when open=false (portal not mounted)", () => {
    const html = renderToStaticMarkup(
      <ConfirmDialog
        open={false}
        onOpenChange={() => {}}
        title="Stop zombie?"
        onConfirm={() => {}}
      />,
    );
    expect(html).not.toContain("Stop zombie?");
  });
});
