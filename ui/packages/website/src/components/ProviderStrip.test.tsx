import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import ProviderStrip from "./ProviderStrip";

describe("ProviderStrip", () => {
  it("renders the workflow surfaces label", () => {
    render(<ProviderStrip />);
    expect(screen.getByText("Where UseZombie works")).toBeInTheDocument();
  });

  it("renders all supported workflow surfaces", () => {
    render(<ProviderStrip />);
    expect(screen.getByText("GitHub")).toBeInTheDocument();
    expect(screen.getByText("CLI")).toBeInTheDocument();
    expect(screen.getByText("API")).toBeInTheDocument();
  });
});
