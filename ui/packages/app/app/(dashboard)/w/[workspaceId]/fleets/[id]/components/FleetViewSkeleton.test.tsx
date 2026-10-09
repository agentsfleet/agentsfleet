import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { FLEET_VIEW } from "./FleetSubnavigation";
import { FleetViewSkeleton } from "./FleetViewSkeleton";

afterEach(() => cleanup());

describe("FleetViewSkeleton", () => {
  it("every fleet view has a skeleton", () => {
    for (const view of Object.values(FLEET_VIEW)) {
      render(<FleetViewSkeleton view={view} />);
      const status = screen.getByRole("status");
      expect(status.textContent).toBe(`Loading ${view}`);
      const panel = status.parentElement;
      expect(panel?.getAttribute("aria-busy")).toBe("true");
      // A shape stands in for the view's frame, so the page does not jump.
      expect(panel?.children.length).toBeGreaterThan(1);
      cleanup();
    }
  });
});
