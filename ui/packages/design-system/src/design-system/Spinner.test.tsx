import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";

import { Spinner } from "./Spinner";

describe("Spinner", () => {
  it("is a polite status region marked busy", () => {
    const { getByRole } = render(<Spinner />);
    const el = getByRole("status");
    expect(el.getAttribute("aria-busy")).toBe("true");
  });

  it("renders the preview-style arc with the brand wake-pulse dot", () => {
    const { getByRole } = render(<Spinner />);
    const orbit = getByRole("status").querySelector("[data-spinner-orbit]") as HTMLElement;
    const dot = orbit.querySelector("[data-live]") as HTMLElement;
    expect(orbit.className).toContain("place-items-center");
    expect(dot.hasAttribute("data-live")).toBe(true);
    expect(dot.className).toContain("bg-pulse");
  });

  it("shows a visible label for standalone loaders", () => {
    const { getByRole, getByText } = render(<Spinner label="Loading agents…" />);
    expect(getByText("Loading agents…")).toBeTruthy();
    expect(getByRole("status").className).toContain("font-mono");
  });

  it("falls back to a screen-reader-only label when no visible label", () => {
    const { getByText } = render(<Spinner srLabel="Installing" />);
    expect(getByText("Installing").className).toContain("sr-only");
  });

  it("scales the dot by size", () => {
    const { getByRole } = render(<Spinner size="lg" />);
    const orbit = getByRole("status").querySelector("[data-spinner-orbit]") as HTMLElement;
    expect(orbit.className).toContain("h-5");
  });
});
