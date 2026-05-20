import { describe, expect, it } from "vitest";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

describe("AUTH_APPEARANCE", () => {
  const { elements } = AUTH_APPEARANCE;

  it("inputs are visually distinct from the card they sit on", () => {
    // Regression: both were var(--surface-2), so the input fields were
    // invisible on the card until focused. They must differ.
    expect(elements.formFieldInput.backgroundColor).not.toBe(
      elements.cardBox.backgroundColor,
    );
  });

  it("inputs carry a visible border so the click target reads without focus", () => {
    expect(elements.formFieldInput.borderColor).toBeTruthy();
  });

  it("pins the surface tokens (card lifts off page; input insets into card)", () => {
    expect(elements.cardBox.backgroundColor).toBe("var(--surface-2)");
    expect(elements.formFieldInput.backgroundColor).toBe("var(--surface-1)");
    expect(elements.formFieldInput.borderColor).toBe("var(--border-strong)");
  });
});
