import { describe, expect, it } from "vitest";
import { dark } from "@clerk/ui/themes";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

const PULSE = "var(--pulse)";

describe("AUTH_APPEARANCE", () => {
  const { elements } = AUTH_APPEARANCE;

  it("applies the current Clerk dark theme so unmapped account controls remain readable", () => {
    // The app is dark-only (lib/theme.ts), so the dark base theme is
    // unconditional: it covers the elements the map below does not name, which
    // otherwise paint in Clerk's stock light palette (invisible on dark).
    expect(AUTH_APPEARANCE.theme).toBe(dark);
  });

  it("inputs are visually distinct from the card they sit on", () => {
    // Regression: both were var(--surface-2), so the input fields were
    // invisible on the card until focused. They must differ.
    expect(elements.formFieldInput["&&"].backgroundColor).not.toBe(
      elements.cardBox.backgroundColor,
    );
  });

  it("inputs carry a visible border so the click target reads without focus", () => {
    expect(elements.formFieldInput["&&"].borderColor).toBeTruthy();
    expect(elements.formFieldInput["&&"].borderWidth).toBe("1px");
  });

  it("keeps segmented email verification inputs visible before focus", () => {
    expect(elements.otpCodeFieldInput["&&"].backgroundColor).toBe(
      elements.formFieldInput["&&"].backgroundColor,
    );
    expect(elements.otpCodeFieldInput["&&"].borderColor).toBe(
      elements.formFieldInput["&&"].borderColor,
    );
    expect(elements.otpCodeFieldInput["&&:focus"]).toEqual(
      elements.formFieldInput["&&:focus"],
    );
  });

  it("test_clerk_v7_variable_keys: primary text + input use the v7 foreground keys, not the ignored pre-v7 names", () => {
    // Root cause of invisible account-modal text: the pre-v7 keys (colorText,
    // colorTextSecondary, colorInputBackground, colorInputText) are silently
    // dropped by Clerk v7, so text fell back to library defaults (dark-on-dark).
    const { variables } = AUTH_APPEARANCE;
    expect(variables.colorForeground).toBe("var(--text)");
    expect(variables.colorInput).toBe("var(--surface-2)");
    expect(variables.colorInputForeground).toBe("var(--text)");
    expect(variables).not.toHaveProperty("colorText");
    expect(variables).not.toHaveProperty("colorTextSecondary");
    expect(variables).not.toHaveProperty("colorInputBackground");
  });

  it("keeps the footer on a background colour and its links out of the pulse accent", () => {
    expect(AUTH_APPEARANCE.elements.footer).not.toHaveProperty("background");
    // Footer / link affordances stay muted (currency-rule guard).
    expect(AUTH_APPEARANCE.elements.footerActionLink.color).not.toBe(PULSE);
    expect(AUTH_APPEARANCE.elements.identityPreviewEditButton.color).not.toBe(PULSE);
    expect(AUTH_APPEARANCE.elements.formResendCodeLink.color).not.toBe(PULSE);
  });
});
