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

  it("uses info-colored border/text by default", () => {
    const { container } = render(<Terminal>x</Terminal>);
    const pre = container.querySelector("pre");
    expect(pre?.className).toContain("text-info");
    expect(pre?.className).toContain("border-border");
  });

  it("switches to success colors with green=true", () => {
    const { container } = render(<Terminal green>x</Terminal>);
    const pre = container.querySelector("pre");
    expect(pre?.className).toContain("text-success");
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
