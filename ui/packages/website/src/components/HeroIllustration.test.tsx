import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import HeroIllustration from "./HeroIllustration";

describe("HeroIllustration", () => {
  it("renders the illustration shell and caption", () => {
    const { container } = render(<HeroIllustration />);
    const aside = container.querySelector(".hero-illustration");
    const svg = container.querySelector(".hero-zombie-mark");
    const gradients = container.querySelectorAll("linearGradient");

    expect(aside).toHaveAttribute("aria-hidden", "true");
    expect(svg).toHaveAttribute("role", "presentation");
    expect(gradients).toHaveLength(2);
    expect(screen.getByText(/undead operator mark/i)).toBeInTheDocument();
  });
});
