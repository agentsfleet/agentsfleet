import type { Appearance } from "@clerk/ui";
import { AUTH_APPEARANCE } from "./clerkAppearance";

export const PROFILE_APPEARANCE = {
  ...AUTH_APPEARANCE,
  elements: {
    ...AUTH_APPEARANCE.elements,
    rootBox: { width: "100%", maxWidth: "none" },
    cardBox: {
      ...AUTH_APPEARANCE.elements.cardBox,
      width: "100%", maxWidth: "none", boxShadow: "none", backgroundColor: "var(--surface-1)",
      height: "auto", minHeight: 0, border: "1px solid var(--border)",
    },
    scrollBox: { borderRadius: "var(--r-lg)", boxShadow: "none" },
    pageScrollBox: {
      ...AUTH_APPEARANCE.elements.pageScrollBox,
      padding: "var(--sp-lg)", height: "auto",
    },
    // Native section headers prefer 264px even below the widget's mobile breakpoint.
    profileSectionHeader: { minWidth: 0, maxWidth: "100%" },
    profilePageContent: {
      minWidth: 0, maxWidth: "100%",
      // The native mobile media rule otherwise restores a second padding layer.
      "&&": { padding: 0 },
    },
    actionCard: { padding: "var(--sp-lg)" },
    navbarButton: {
      ...AUTH_APPEARANCE.elements.navbarButton,
      fontSize: "var(--fs-body-sm)", fontWeight: 500, borderRadius: "var(--r-md)",
    },
    headerTitle: { ...AUTH_APPEARANCE.elements.headerTitle, fontSize: "var(--fs-heading)", fontWeight: 650 },
    profileSectionTitleText: {
      ...AUTH_APPEARANCE.elements.profileSectionTitleText,
      fontSize: "var(--fs-body-sm)", fontWeight: 600,
    },
    formFieldInput: {
      ...AUTH_APPEARANCE.elements.formFieldInput,
      "&&": {
        ...AUTH_APPEARANCE.elements.formFieldInput["&&"],
        minHeight: "44px", padding: "8px 12px", fontSize: "var(--fs-body-sm)",
        backgroundColor: "var(--bg)", border: "1px solid var(--border)", borderColor: "var(--border)",
        borderRadius: "var(--r-control)",
      },
      "&&:focus": {borderColor: "var(--border)", boxShadow: "none"},
      "&&:focus-visible": {outline: "2px solid var(--pulse)", outlineOffset: "4px"},
      "@media (max-width: 42.5rem), (pointer: coarse)": {"&&": {fontSize: "16px", lineHeight: "24px"}},
    },
    formButtonPrimary: {
      ...AUTH_APPEARANCE.elements.formButtonPrimary,
      minHeight: "40px", padding: "8px 16px", fontSize: "var(--fs-body-sm)",
      fontWeight: 500, borderRadius: "var(--r-control)",
      "&&:hover:not(:disabled)": {filter: "brightness(.94)", backgroundColor: "var(--cta)"},
      "&&:focus-visible": {boxShadow: "none", outline: "2px solid var(--pulse)", outlineOffset: "4px"},
    },
    formButtonReset: {
      minHeight: "40px", padding: "8px 16px", backgroundColor: "var(--surface-1)",
      border: "1px solid var(--border)", borderRadius: "var(--r-control)",
      color: "var(--text)", fontSize: "var(--fs-body-sm)",
      "&&:hover": {backgroundColor: "var(--surface-2)"},
      "&&:focus-visible": {outline: "2px solid var(--pulse)", outlineOffset: "4px"},
    },
    profileSectionPrimaryButton: {
      ...AUTH_APPEARANCE.elements.profileSectionPrimaryButton,
      minHeight: "40px", fontSize: "var(--fs-body-sm)", fontWeight: 500,
    },
  },
} as const satisfies Appearance;
