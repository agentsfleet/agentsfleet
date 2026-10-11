import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "./Tooltip";

describe("Tooltip", () => {
  it("renders only the trigger by default", () => {
    render(
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger>Hover</TooltipTrigger>
          <TooltipContent>Info</TooltipContent>
        </Tooltip>
      </TooltipProvider>,
    );
    expect(screen.getByText("Hover")).toBeInTheDocument();
    expect(screen.queryByText("Info")).not.toBeInTheDocument();
  });

  it("TooltipTrigger forwards ref and applies no extra classes by default", () => {
    const ref = { current: null as HTMLButtonElement | null };
    render(
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger ref={ref}>Hover</TooltipTrigger>
          <TooltipContent>Info</TooltipContent>
        </Tooltip>
      </TooltipProvider>,
    );
    expect(ref.current).toBeInstanceOf(HTMLButtonElement);
  });
});
