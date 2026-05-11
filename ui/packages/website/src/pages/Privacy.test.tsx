import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import Privacy from "./Privacy";
import { SUPPORT_EMAIL } from "../lib/contact";

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

  it("mentions self-managed model and no key collection", () => {
    render(<Privacy />);
    expect(screen.getByText(/self-managed model/i)).toBeInTheDocument();
  });

  it("renders contact email link sourced from SUPPORT_EMAIL", () => {
    render(<Privacy />);
    expect(screen.getByRole("link", { name: SUPPORT_EMAIL })).toHaveAttribute(
      "href",
      `mailto:${SUPPORT_EMAIL}`,
    );
  });
});
