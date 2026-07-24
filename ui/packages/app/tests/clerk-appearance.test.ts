import { describe, expect, it } from "vitest";
import { dark } from "@clerk/themes";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

describe("AUTH_APPEARANCE", () => {
  const { elements } = AUTH_APPEARANCE;

  it("applies the dark baseTheme so unmapped Clerk internals (Security device icons, Profile inputs) render on the dark surface", () => {
    // The app is dark-only (lib/theme.ts), so the dark base theme is
    // unconditional: it covers the elements the map below does not name, which
    // otherwise paint in Clerk's stock light palette (invisible on dark).
    expect(AUTH_APPEARANCE.baseTheme).toBe(dark);
  });

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

  it("keeps segmented email verification inputs visible before focus", () => {
    expect(elements.otpCodeFieldInput.backgroundColor).toBe(
      elements.formFieldInput.backgroundColor,
    );
    expect(elements.otpCodeFieldInput.borderColor).toBe(
      elements.formFieldInput.borderColor,
    );
    expect(elements.otpCodeFieldInput["&:focus"]).toEqual(
      elements.formFieldInput["&:focus"],
    );
  });

  it("themes the UserButton avatar fallback with design tokens, not Clerk's palette", () => {
    // With no uploaded image Clerk renders an initials fallback; pin its fill +
    // text to design tokens so "what you see with no avatar" matches the app.
    expect(elements.userButtonAvatarBox.backgroundColor).toBe("var(--surface-2)");
    expect(elements.userButtonAvatarBox.color).toBe("var(--text)");
  });

  it("test_clerk_secondary_identifier_contrast: the account-modal email maps to the readable token, not the subtle one", () => {
    // Regression: the secondary identifier (email) read too dim on the dark
    // modal surface. It must be the readable text token — never the subtle one.
    expect(elements.userPreviewSecondaryIdentifier.color).toBe("var(--text)");
    expect(elements.userPreviewSecondaryIdentifier.color).not.toBe("var(--text-subtle)");
    // The global fallback for any unmapped secondary text stays readable too.
    // v7 key: colorMutedForeground (pre-v7 colorTextSecondary is ignored).
    expect(AUTH_APPEARANCE.variables.colorMutedForeground).toBe("var(--text-muted)");
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

  it("the modal close (X) button is pinned readable on the dark surface", () => {
    // Reported: the X in the top corner rendered near-black — invisible on dark.
    expect(elements.modalCloseButton.color).toBe("var(--text)");
  });
});
