import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";

import { Spinner } from "./Spinner";

describe("Spinner", () => {
  it("is a polite status region marked busy", () => {
    const { getByRole } = render(<Spinner />);
    const el = getByRole("status");
    expect(el.getAttribute("aria-busy")).toBe("true");
  });

  it("renders the brand wake-pulse dot, not a spinning icon", () => {
    const { getByRole } = render(<Spinner />);
    const dot = getByRole("status").firstElementChild as HTMLElement;
    expect(dot.hasAttribute("data-live")).toBe(true);
    expect(dot.className).toContain("bg-pulse");
  });

  it("shows a visible label for standalone loaders", () => {
    const { getByText } = render(<Spinner label="Loading zombies…" />);
    expect(getByText("Loading zombies…")).toBeTruthy();
  });

  it("falls back to a screen-reader-only label when no visible label", () => {
    const { getByText } = render(<Spinner srLabel="Installing" />);
    expect(getByText("Installing").className).toContain("sr-only");
  });

  it("scales the dot by size", () => {
    const { getByRole } = render(<Spinner size="lg" />);
    const dot = getByRole("status").firstElementChild as HTMLElement;
    expect(dot.className).toContain("h-5");
  });
});
