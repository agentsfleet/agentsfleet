import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import Privacy from "./Privacy";
import { SUPPORT_EMAIL } from "../lib/contact";

describe("Privacy", () => {
  it("renders the heading", () => {
    render(<Privacy />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/privacy policy/i);
  });

  it("renders contact email link sourced from SUPPORT_EMAIL", () => {
    render(<Privacy />);
    expect(screen.getByRole("link", { name: SUPPORT_EMAIL })).toHaveAttribute(
      "href",
      `mailto:${SUPPORT_EMAIL}`,
    );
  });
});
