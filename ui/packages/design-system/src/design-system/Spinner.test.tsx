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
    const mark = getByRole("status").querySelector("svg");
    expect(mark?.getAttribute("aria-hidden")).toBe("true");
    expect(mark?.querySelectorAll(".page-loading-stroke")).toHaveLength(8);
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
    const mark = getByRole("status").querySelector("svg");
    expect(mark?.classList.contains("size-6")).toBe(true);
  });

  it("pairs the full-screen mark with a decorative wordmark and one loading status", () => {
    const { getAllByRole, getByRole, getByText } = render(<Spinner variant="full-screen" label="Loading…" aria-label="Loading page" />);
    const status = getByRole("status", { name: "Loading page" });
    expect(getAllByRole("status")).toHaveLength(1);
    const wordmark = getByText("agentsfleet");
    expect(status.contains(wordmark)).toBe(true);
    expect(wordmark.closest('[aria-hidden="true"]')?.querySelectorAll(".page-loading-stroke")).toHaveLength(8);
    expect(getByText("Loading…").closest('[aria-hidden="true"]')).toBeNull();
  });

  it("assembles the eight strokes above the page loading label", () => {
    const { getByRole, queryAllByRole } = render(<Spinner variant="page" label="Loading…" aria-label="Loading page" />);
    const status = getByRole("status", { name: "Loading page" });
    const mark = status.querySelector("svg");
    expect(status.textContent).toBe("Loading…");
    expect(status.classList.contains("flex-col")).toBe(true);
    expect(mark?.getAttribute("width")).toBe("60");
    expect(mark?.getAttribute("aria-hidden")).toBe("true");
    expect(mark?.querySelectorAll(".page-loading-stroke")).toHaveLength(8);
    expect(queryAllByRole("status")).toHaveLength(1);
  });
});
