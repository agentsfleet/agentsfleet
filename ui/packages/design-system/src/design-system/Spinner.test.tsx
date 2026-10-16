import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";

import { Spinner } from "./Spinner";

describe("Spinner", () => {
  it("is a polite status region marked busy", () => {
    const { getByRole } = render(<Spinner />);
    const el = getByRole("status");
    expect(el.getAttribute("aria-busy")).toBe("true");
  });

  it("uses a decorative brand signal without nesting another live status", () => {
    const { getByRole, queryAllByRole } = render(<Spinner />);
    const mark = getByRole("status").querySelector("[data-signal-spinner]");
    expect(mark?.getAttribute("aria-hidden")).toBe("true");
    expect(mark?.querySelectorAll("path")).toHaveLength(8);
    expect(queryAllByRole("status")).toHaveLength(1);
  });

  it("shows a visible label for standalone loaders", () => {
    const { getByRole, getByText } = render(<Spinner label="Loading agents…" />);
    expect(getByText("Loading agents…")).toBeTruthy();
    expect(getByRole("status").className).toContain("font-sans");
  });

  it("falls back to a screen-reader-only label when no visible label", () => {
    const { getByText } = render(<Spinner srLabel="Installing" />);
    expect(getByText("Installing").className).toContain("sr-only");
  });

  it("scales the mark by size", () => {
    const { getByRole } = render(<Spinner size="lg" />);
    const orbit = getByRole("status").querySelector("[data-signal-spinner]") as SVGElement;
    expect(orbit.getAttribute("class")).toContain("size-6");
  });
});
