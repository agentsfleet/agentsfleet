import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { TooltipButton } from "./TooltipButton";
import { TooltipProvider } from "./Tooltip";

function renderButton(node: React.ReactElement) {
  return render(<TooltipProvider>{node}</TooltipProvider>);
}

describe("TooltipButton", () => {
  it("renders button text and opens a tooltip on focus", async () => {
    renderButton(<TooltipButton tooltip="Start from the fleet library">Install fleet</TooltipButton>);

    const button = screen.getByRole("button", { name: "Install fleet" });
    fireEvent.focus(button);

    expect(await screen.findByRole("tooltip")).toHaveTextContent("Start from the fleet library");
  });

  it("passes through variants and click handlers", () => {
    const onClick = vi.fn();
    renderButton(
      <TooltipButton tooltip="Create an API key" variant="destructive" onClick={onClick}>
        Create key
      </TooltipButton>,
    );

    const button = screen.getByRole("button", { name: "Create key" });
    expect(button.className).toContain("bg-destructive");
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("supports link composition without losing tooltip behavior", async () => {
    renderButton(
      <TooltipButton asChild tooltip="Start from the fleet library">
        <a href="/fleets/new">Install fleet</a>
      </TooltipButton>,
    );

    const link = screen.getByRole("link", { name: "Install fleet" });
    expect(link.getAttribute("href")).toBe("/fleets/new");
    fireEvent.focus(link);

    expect(await screen.findByRole("tooltip")).toHaveTextContent("Start from the fleet library");
  });

  it("keeps disabled buttons hoverable through a wrapper", () => {
    const onClick = vi.fn();
    renderButton(
      <TooltipButton tooltip="Create a runner" disabled onClick={onClick}>
        Create runner
      </TooltipButton>,
    );

    const button = screen.getByRole("button", { name: "Create runner" });
    expect(button).toBeDisabled();
    expect(button.className).toContain("pointer-events-none");
    expect(button.parentElement?.tagName).toBe("SPAN");
    fireEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });
});
