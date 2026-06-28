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
    expect(AUTH_APPEARANCE.variables.colorTextSecondary).toBe("var(--text-muted)");
  });
});
