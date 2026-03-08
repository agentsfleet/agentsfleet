import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import Privacy from "./Privacy";

describe("Privacy", () => {
  it("renders the heading", () => {
    render(<Privacy />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/privacy policy/i);
  });

  it("renders last updated date", () => {
    render(<Privacy />);
    expect(screen.getByText(/last updated/i)).toBeInTheDocument();
  });

  it("renders all major sections", () => {
    render(<Privacy />);
    expect(screen.getByText(/information we collect/i)).toBeInTheDocument();
    expect(screen.getByText(/information we do not collect/i)).toBeInTheDocument();
    expect(screen.getByText(/how we use your information/i)).toBeInTheDocument();
    expect(screen.getByText(/data retention/i)).toBeInTheDocument();
    expect(screen.getByText(/third-party services/i)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /contact/i })).toBeInTheDocument();
  });

  it("mentions BYOK model and no key collection", () => {
    render(<Privacy />);
    expect(screen.getByText(/BYOK model/i)).toBeInTheDocument();
  });

  it("renders contact email link", () => {
    render(<Privacy />);
    expect(screen.getByRole("link", { name: /privacy@usezombie\.com/i })).toHaveAttribute(
      "href",
      "mailto:privacy@usezombie.com"
    );
  });
});
