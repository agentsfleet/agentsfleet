import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { ConfirmDialog } from "./ConfirmDialog";

describe("ConfirmDialog", () => {
  it("renders nothing visible when open is false (Radix Dialog portal unmounted)", () => {
    render(
      <ConfirmDialog
        open={false}
        onOpenChange={() => {}}
        title="Stop agent?"
        onConfirm={() => {}}
      />,
    );
    expect(screen.queryByText("Stop agent?")).not.toBeInTheDocument();
  });

  it("renders title + description when open (alertdialog role + aria-describedby)", () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="Stop agent?"
        description="This will halt the agent."
        onConfirm={() => {}}
      />,
    );
    expect(screen.getByText("Stop agent?")).toBeInTheDocument();
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

  it("swallows a rejected onConfirm when no onError is provided", async () => {
    // The catch's `if (onError)` FALSE arm: with no onError, the rejection is
    // dropped (React drops async event-handler rejections in production), and
    // the finally still clears `pending` so the dialog returns to rest.
    const onConfirm = vi.fn().mockRejectedValue(new Error("boom"));
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="X"
        confirmLabel="Do it"
        onConfirm={onConfirm}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Do it" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Do it" })).not.toHaveAttribute("aria-busy"),
    );
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("routes the Radix close affordance through onOpenChange when idle", () => {
    // The Dialog's X close button fires Radix's onOpenChange(false), which
    // the ConfirmDialog wrapper forwards because `pending` is false (the
    // `if (!pending)` truthy arm).
    const onOpenChange = vi.fn();
    render(
      <ConfirmDialog
        open
        onOpenChange={onOpenChange}
        title="Stop"
        onConfirm={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("blocks onOpenChange while a confirm is in flight (pending guard)", async () => {
    const onOpenChange = vi.fn();
    let release: (() => void) | undefined;
    const onConfirm = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    render(
      <ConfirmDialog
        open
        onOpenChange={onOpenChange}
        title="Stop"
        confirmLabel="Stop"
        onConfirm={onConfirm}
      />,
    );
    // Kick off the in-flight action; the dialog flips to pending and the
    // confirm button shows the working label + aria-busy.
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Working…" })).toHaveAttribute(
        "aria-busy",
        "true",
      ),
    );
    // The Radix close affordance now routes through Dialog's onOpenChange;
    // while pending the ConfirmDialog wrapper must swallow it (the
    // `if (!pending)` false arm) so the dialog can't close mid-action.
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onOpenChange).not.toHaveBeenCalled();
    // Resolve so the act() teardown is clean.
    await waitFor(() => expect(release).toBeDefined());
    await act(async () => {
      await release?.();
    });
  });

  it("ignores a second confirm click while the first is still in flight", async () => {
    let release: (() => void) | undefined;
    const onConfirm = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="Stop"
        confirmLabel="Stop"
        onConfirm={onConfirm}
      />,
    );
    const confirm = screen.getByRole("button", { name: "Stop" });
    fireEvent.click(confirm);
    // Once pending, the confirm button is disabled — a second click cannot
    // re-enter handleConfirm, so onConfirm stays at a single invocation.
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Working…" })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Working…" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(release).toBeDefined());
    await act(async () => {
      await release?.();
    });
  });

  it("re-enables interaction after the in-flight action resolves", async () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
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
    // After resolution the button returns to its resting label.
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Stop" })).not.toHaveAttribute(
        "aria-busy",
      ),
    );
  });

  it("SSR renders nothing visible when open=false (portal not mounted)", () => {
    const html = renderToStaticMarkup(
      <ConfirmDialog
        open={false}
        onOpenChange={() => {}}
        title="Stop agent?"
        onConfirm={() => {}}
      />,
    );
    expect(html).not.toContain("Stop agent?");
  });
});
