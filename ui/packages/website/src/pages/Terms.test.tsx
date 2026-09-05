import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import Terms from "./Terms";
import { SUPPORT_EMAIL } from "../lib/contact";

describe("Terms", () => {
  it("renders the heading", () => {
    render(<Terms />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/terms of service/i);
  });

  it("renders last updated date", () => {
    render(<Terms />);
    expect(screen.getByText(/last updated/i)).toBeInTheDocument();
  });

  it("renders all major sections", () => {
    render(<Terms />);
    expect(screen.getByText(/acceptance/i)).toBeInTheDocument();
    expect(screen.getByText(/service description/i)).toBeInTheDocument();
    expect(screen.getByText(/your responsibilities/i)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /billing/i })).toBeInTheDocument();
    expect(screen.getByText(/intellectual property/i)).toBeInTheDocument();
    expect(screen.getByText(/limitation of liability/i)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /termination/i })).toBeInTheDocument();
  });

  it("mentions early access in billing terms, without committing to a date", () => {
    render(<Terms />);
    expect(screen.getByText(/pricing will be confirmed before paid usage/i)).toBeInTheDocument();
    expect(screen.getByTestId("terms-page")).not.toHaveTextContent(/starter credit|free to try|zero markup|\$\d/i);
    expect(screen.getByTestId("terms-page")).not.toHaveTextContent(/enterprise tier/i);
  });

  it("renders contact email link sourced from SUPPORT_EMAIL", () => {
    render(<Terms />);
    expect(screen.getByRole("link", { name: SUPPORT_EMAIL })).toHaveAttribute(
      "href",
      `mailto:${SUPPORT_EMAIL}`,
    );
  });
});
