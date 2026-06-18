import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import OperationalKnowledgeSection from "./OperationalKnowledgeSection";
import {
  KNOWLEDGE_POINTS,
  OPERATIONAL_KNOWLEDGE_HEADING,
  OPERATIONAL_KNOWLEDGE_LEDE,
} from "../lib/marketing-copy";

describe("OperationalKnowledgeSection", () => {
  it("should render the compounding knowledge heading as an h2", () => {
    render(<OperationalKnowledgeSection />);
    expect(
      screen.getByRole("heading", {
        level: 2,
        name: OPERATIONAL_KNOWLEDGE_HEADING,
      }),
    ).toBeInTheDocument();
    expect(screen.getByText(OPERATIONAL_KNOWLEDGE_LEDE)).toBeInTheDocument();
  });

  it("should render every knowledge point from shared copy constants", () => {
    render(<OperationalKnowledgeSection />);
    for (const point of KNOWLEDGE_POINTS) {
      const card = screen.getByTestId(`knowledge-point-${point.number}`);
      expect(card).toHaveTextContent(point.number);
      expect(card).toHaveTextContent(point.title);
      expect(card).toHaveTextContent(point.description);
    }
  });
});
