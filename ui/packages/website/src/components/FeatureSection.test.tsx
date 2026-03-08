import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import FeatureSection from "./FeatureSection";

describe("FeatureSection", () => {
  it("renders the number, title, and description", () => {
    render(
      <FeatureSection
        number="01"
        title="Test Feature"
        description="A detailed description of the test feature."
      />
    );

    expect(screen.getByText("01")).toBeInTheDocument();
    expect(screen.getByText("Test Feature")).toBeInTheDocument();
    expect(screen.getByText("A detailed description of the test feature.")).toBeInTheDocument();
  });

  it("renders the number with feature-number class", () => {
    render(
      <FeatureSection number="03" title="Title" description="Desc" />
    );

    const number = screen.getByText("03");
    expect(number).toHaveClass("feature-number");
  });

  it("renders the title as an h3", () => {
    render(
      <FeatureSection number="01" title="My Title" description="Desc" />
    );

    expect(screen.getByRole("heading", { level: 3, name: "My Title" })).toBeInTheDocument();
  });
});
