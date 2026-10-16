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
    },
    scrollBox: { borderRadius: "var(--r-lg)", boxShadow: "none" },
    pageScrollBox: {
      ...AUTH_APPEARANCE.elements.pageScrollBox,
      padding: "var(--sp-2xl)",
    },
    navbarButton: {
      ...AUTH_APPEARANCE.elements.navbarButton,
      fontSize: "var(--fs-body-sm)", fontWeight: 500, borderRadius: "var(--r-md)",
    },
    headerTitle: { ...AUTH_APPEARANCE.elements.headerTitle, fontSize: "var(--fs-heading)" },
    profileSectionTitleText: {
      ...AUTH_APPEARANCE.elements.profileSectionTitleText,
      fontSize: "var(--fs-body-sm)", fontWeight: 500,
    },
    profileSectionPrimaryButton: {
      ...AUTH_APPEARANCE.elements.profileSectionPrimaryButton,
      fontSize: "var(--fs-body-sm)",
    },
  },
} as const satisfies Appearance;
