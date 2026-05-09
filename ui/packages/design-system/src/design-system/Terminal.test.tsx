import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import Terminal from "./Terminal";

describe("Terminal", () => {
  it("renders the command inside a <pre><code>", () => {
    const { container } = render(<Terminal>echo hi</Terminal>);
    const pre = container.querySelector("pre");
    expect(pre).toBeInTheDocument();
    expect(pre?.textContent).toContain("echo hi");
  });

  it("uses operational mono on the deepest surface by default", () => {
    const { container } = render(<Terminal>x</Terminal>);
    const pre = container.querySelector("pre");
    // Body retains the mono + foreground colour contract.
    expect(pre?.className).toContain("text-foreground");
    expect(pre?.className).toContain("font-mono");
    // The terminal-window chrome lives on the outer wrapper now —
    // border + background are owned by the wrapper, not the <pre>.
    const wrapper = container.firstElementChild as HTMLElement | null;
    expect(wrapper?.className).toContain("border-border");
    // The terminal body sits one shade below --bg per the canonical
    // preview (`.cli { background: #06090A; }` line 195) — the Layer 0
    // token is --surface-deep, exposed as `bg-surface-deep`.
    expect(wrapper?.className).toContain("bg-surface-deep");
  });

  it("switches to success colors with green=true", () => {
    const { container } = render(<Terminal green>x</Terminal>);
    const pre = container.querySelector("pre");
    expect(pre?.className).toContain("text-success");
    const wrapper = container.firstElementChild as HTMLElement | null;
    expect(wrapper?.className).toContain("border-success");
  });

  it("renders the terminal-window chrome with three muted dot affordances", () => {
    const { container } = render(<Terminal>x</Terminal>);
    // The chrome is the first child of the wrapper; it carries three
    // <span> dot elements for the window-control affordance.
    const chrome = container.firstElementChild?.firstElementChild as HTMLElement | null;
    expect(chrome).toBeTruthy();
    const dotContainer = chrome?.querySelector("span[aria-hidden='true']");
    expect(dotContainer?.children.length).toBe(3);
  });

  it("renders the label inside the chrome when provided", () => {
    const { container } = render(<Terminal label="Install">x</Terminal>);
    const chrome = container.firstElementChild?.firstElementChild as HTMLElement | null;
    expect(chrome?.textContent).toContain("Install");
  });

  it("does not render a copy button by default", () => {
    render(<Terminal>x</Terminal>);
    expect(screen.queryByTestId("copy-btn")).not.toBeInTheDocument();
  });

  it("renders copy button when copyable=true", () => {
    render(<Terminal copyable>x</Terminal>);
    expect(screen.getByTestId("copy-btn")).toBeInTheDocument();
  });

  it("exposes the command on data-command attribute for string children", () => {
    const { container } = render(<Terminal>my cmd</Terminal>);
    expect(container.querySelector("pre")).toHaveAttribute("data-command", "my cmd");
  });

  it("renders aria-label when label is provided", () => {
    render(<Terminal label="Install">curl ...</Terminal>);
    expect(screen.getByLabelText("Install")).toBeInTheDocument();
  });

  describe("copy interaction", () => {
    beforeEach(() => {
      Object.assign(navigator, {
        clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
      });
    });

    it("writes command to clipboard and flips to Copied state on click", async () => {
      render(<Terminal copyable>install command</Terminal>);
      const btn = screen.getByTestId("copy-btn");
      fireEvent.click(btn);
      await waitFor(() =>
        expect(navigator.clipboard.writeText).toHaveBeenCalledWith("install command"),
      );
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Copied!" })).toBeInTheDocument(),
      );
    });
  });
});
