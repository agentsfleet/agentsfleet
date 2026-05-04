import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import ProviderStrip from "./ProviderStrip";

describe("ProviderStrip", () => {
  it("renders the BYOK provider label", () => {
    render(<ProviderStrip />);
    expect(screen.getByText("Bring your own model")).toBeInTheDocument();
  });

  it("renders all supported BYOK providers", () => {
    render(<ProviderStrip />);
    expect(screen.getByText("Anthropic")).toBeInTheDocument();
    expect(screen.getByText("OpenAI")).toBeInTheDocument();
    expect(screen.getByText("Fireworks · Kimi K2")).toBeInTheDocument();
    expect(screen.getByText("Together")).toBeInTheDocument();
    expect(screen.getByText("Groq")).toBeInTheDocument();
    expect(screen.getByText("Moonshot")).toBeInTheDocument();
  });
});
