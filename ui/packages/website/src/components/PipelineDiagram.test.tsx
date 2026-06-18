import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import PipelineDiagram from "./PipelineDiagram";
import { LEDGER_LINES, SOURCE_CATEGORIES } from "../lib/marketing-copy";

describe("PipelineDiagram", () => {
  it("should render the Terminal Ledger with every ledger line in order", () => {
    render(<PipelineDiagram />);
    const ledger = screen.getByTestId("pipeline-ledger");
    let previous: HTMLElement | null = null;

    for (const line of LEDGER_LINES) {
      const row = screen.getByTestId(`pipeline-ledger-line-${line.id}`);
      expect(row).toHaveTextContent(line.timestamp);
      expect(row).toHaveTextContent(line.message);
      if (line.tag) {
        expect(row).toHaveTextContent(line.tag);
      }
      if (previous) {
        expect(
          previous.compareDocumentPosition(row) & Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
      }
      previous = row;
    }

    expect(ledger).toHaveTextContent("core.agent_events");
  });

  it("should render the human approval gate as a static ledger hold", () => {
    render(<PipelineDiagram />);
    const gate = screen.getByTestId("pipeline-human-gate");
    expect(gate).toHaveTextContent(/awaiting human approval/i);
    expect(gate).toHaveTextContent("hold");
    expect(gate).not.toHaveAttribute("data-live");
  });

  it("should render all source categories with local logo assets only", () => {
    render(<PipelineDiagram />);
    const strip = screen.getByTestId("pipeline-source-strip");

    for (const category of SOURCE_CATEGORIES) {
      const card = screen.getByTestId(`pipeline-source-${category.id}`);
      expect(within(card).getByText(category.label)).toBeInTheDocument();
      for (const example of category.examples) {
        expect(card).toHaveTextContent(example);
      }
    }

    const imageSources = Array.from(strip.querySelectorAll("img")).map((img) =>
      img.getAttribute("src"),
    );
    expect(imageSources).toEqual(SOURCE_CATEGORIES.map((category) => category.icon));
    expect(imageSources.every((src) => src?.startsWith("/logos/"))).toBe(true);
    expect(imageSources.some((src) => /^https?:\/\//.test(src ?? ""))).toBe(false);
    for (const img of strip.querySelectorAll("img")) {
      expect(img).toHaveAttribute("loading", "lazy");
      expect(img).toHaveAttribute("decoding", "async");
    }
  });
});
