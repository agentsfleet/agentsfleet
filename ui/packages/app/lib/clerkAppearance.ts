/*
 * Clerk widget theming for sign-in / sign-up + the dashboard UserButton
 * avatar. Tokens map to the
 * Operational Restraint design system (docs/DESIGN_SYSTEM.md):
 *   --surface-1   cards
 *   --surface-2   inputs / elevated chrome
 *   --surface-3   hover / active chrome
 *   --text*       text primary / muted / subtle
 *   --pulse       primary action fill ONLY (currency rule — the primary
 *                 action button is the system's "wake" affordance).
 *                 Footer links, resend-code links, edit buttons use
 *                 muted text — they are navigation, not live signals.
 *   --bg          contrast text on the pulse fill
 *   --border*     dividers + outlines
 *   --error       error states. Failed != live; never --pulse.
 * No box-shadow on chrome (spec: borders preferred over shadows).
 * No gradient on the footer (spec: no decorative gradients on chrome).
 */
import { dark } from "@clerk/themes";

const SURFACE_1 = "var(--surface-1)";
const SURFACE_2 = "var(--surface-2)";
const SURFACE_3 = "var(--surface-3)";
const TEXT = "var(--text)";
const TEXT_MUTED = "var(--text-muted)";
const TEXT_SUBTLE = "var(--text-subtle)";
const PULSE = "var(--pulse)";
const BACKGROUND = "var(--bg)";
const BORDER = "var(--border)";
const BORDER_STRONG = "var(--border-strong)";
const ERROR = "var(--error)";
const SUCCESS = "var(--success)";
const BORDER_STYLE = `1px solid ${BORDER}`;
const BORDER_STRONG_STYLE = `1px solid ${BORDER_STRONG}`;
const FOCUS_RING = `0 0 0 1px ${PULSE}`;

const MENU_ACTION_INTERACTION = {
  backgroundColor: SURFACE_3,
  color: TEXT,
} as const;

export const AUTH_APPEARANCE = {
  // Dark is the only product surface (lib/theme.ts forces it), so the dark
  // baseTheme applies unconditionally. It styles the Clerk internals the element
  // map below does not name — the Security tab's device-type icons and the
  // account modal's inputs — which otherwise render in Clerk's stock light
  // palette (dark text/icons on the dark surface, effectively invisible). The
  // variables + elements below then map the design tokens on top, so the
  // baseline is dark and the accents stay on-brand.
  baseTheme: dark,
  variables: {
    colorBackground: SURFACE_1,
    colorInputBackground: SURFACE_2,
    colorInputText: TEXT,
    colorText: TEXT,
    colorTextSecondary: TEXT_MUTED,
    colorPrimary: PULSE,
    colorDanger: ERROR,
    borderRadius: "var(--r-sm)",
    fontFamily: "var(--ff-sans)",
  },
  elements: {
    // Dashboard header avatar (UserButton). With no uploaded image Clerk renders
    // an initials fallback — theme its fill + initials with design tokens so it
    // matches the app instead of Clerk's stock palette.
    userButtonAvatarBox: {
      backgroundColor: SURFACE_2,
      color: TEXT,
    },
    // UserButton dropdown (account menu). Without these the popover renders in
    // Clerk's stock light palette → invisible dark text on the app's dark
    // surface. Theme the card, the action rows, and the identity preview.
    userButtonPopoverCard: {
      backgroundColor: SURFACE_1,
      border: BORDER_STRONG_STYLE,
      boxShadow: "none",
    },
    userButtonPopoverActionButton: {
      backgroundColor: "transparent",
      color: TEXT,
      "&:hover": MENU_ACTION_INTERACTION,
      "&:focus": MENU_ACTION_INTERACTION,
    },
    userButtonPopoverActionButtonText: {
      color: TEXT,
    },
    userButtonPopoverActionButtonIcon: {
      color: TEXT_MUTED,
    },
    userButtonPopoverFooter: {
      backgroundColor: SURFACE_1,
      borderTop: BORDER_STYLE,
    },
    userPreviewMainIdentifier: {
      color: TEXT,
    },
    // The account modal's email (secondary identifier) read as a too-dim token
    // on the dark modal surface — effectively invisible. Pin it to the primary
    // readable text token; hierarchy below the name comes from Clerk's smaller
    // secondary-identifier type, not colour.
    userPreviewSecondaryIdentifier: {
      color: TEXT,
    },
    userProfileRoot: {
      backgroundColor: SURFACE_1,
      color: TEXT,
    },
    userProfileCard: {
      backgroundColor: SURFACE_1,
      border: BORDER_STRONG_STYLE,
      boxShadow: "none",
      color: TEXT,
    },
    userProfilePage: {
      backgroundColor: SURFACE_1,
      color: TEXT,
    },
    userProfileModalContent: {
      backgroundColor: SURFACE_1,
      border: BORDER_STRONG_STYLE,
      boxShadow: "none",
    },
    modalContent: {
      backgroundColor: SURFACE_1,
      border: BORDER_STRONG_STYLE,
      boxShadow: "none",
    },
    modalBackdrop: {
      backgroundColor: "rgba(10, 13, 14, 0.72)",
    },
    pageScrollBox: {
      backgroundColor: SURFACE_1,
    },
    navbar: {
      backgroundColor: SURFACE_1,
      borderRight: BORDER_STYLE,
    },
    navbarButton: {
      backgroundColor: "transparent",
      color: TEXT_MUTED,
      "&:hover": MENU_ACTION_INTERACTION,
      "&:focus": MENU_ACTION_INTERACTION,
    },
    navbarButton__active: {
      backgroundColor: SURFACE_3,
      color: TEXT,
    },
    navbarButtonIcon: {
      color: TEXT_MUTED,
    },
    navbarButtonText: {
      color: "inherit",
    },
    profileSectionTitleText: {
      color: TEXT,
    },
    profileSectionContent: {
      color: TEXT_MUTED,
    },
    profileSectionItem: {
      borderColor: BORDER,
    },
    profileSectionItemList: {
      borderColor: BORDER,
    },
    profileSectionPrimaryButton: {
      color: PULSE,
    },
    profileSectionSecondaryButton: {
      color: TEXT_MUTED,
    },
    // Mask self-service account deletion (UserProfile → Security). The robust
    // control is the instance-level toggle in the Clerk Dashboard; this hides
    // the user-interface section as a stopgap.
    profileSection__danger: {
      display: "none",
    },
    cardBox: {
      // --surface-2 over the page's --bg gives the card real visual lift on
      // the auth route — at --surface-1 (luminance delta = 3 units) the card
      // disappears into the background. --border-strong sharpens the edge.
      backgroundColor: SURFACE_2,
      border: BORDER_STRONG_STYLE,
    },
    headerTitle: {
      color: TEXT,
    },
    headerSubtitle: {
      color: TEXT_MUTED,
    },
    socialButtonsBlockButton: {
      backgroundColor: SURFACE_2,
      border: BORDER_STYLE,
      color: TEXT,
    },
    socialButtonsBlockButtonText: {
      color: TEXT,
    },
    dividerLine: {
      backgroundColor: BORDER,
    },
    dividerText: {
      color: TEXT_SUBTLE,
    },
    formFieldLabel: {
      color: TEXT,
    },
    formFieldInput: {
      // Inputs sit ON the --surface-2 card (cardBox above). Filling them
      // with --surface-2 too left zero luminance delta — the field was
      // invisible until focused. Drop to --surface-1 (one step toward --bg)
      // for an inset well, and use --border-strong so the edge reads as a
      // click target without a focus event.
      backgroundColor: SURFACE_1,
      border: BORDER_STRONG_STYLE,
      borderColor: BORDER_STRONG,
      boxShadow: "none",
      color: TEXT,
      "&:hover": {
        borderColor: PULSE,
      },
      "&:focus": {
        borderColor: PULSE,
        boxShadow: FOCUS_RING,
      },
      "&::placeholder": {
        color: TEXT_SUBTLE,
      },
    },
    formButtonPrimary: {
      backgroundColor: PULSE,
      color: BACKGROUND,
    },
    footerActionText: {
      color: TEXT_MUTED,
    },
    footerActionLink: {
      color: TEXT,
      textDecoration: "underline",
      textDecorationColor: BORDER,
    },
    identityPreviewText: {
      color: TEXT,
    },
    identityPreviewEditButton: {
      color: TEXT_MUTED,
    },
    formResendCodeLink: {
      color: TEXT_MUTED,
    },
    formFieldSuccessText: {
      color: SUCCESS,
    },
    formFieldErrorText: {
      color: ERROR,
    },
    alertText: {
      color: ERROR,
    },
    footer: {
      backgroundColor: SURFACE_1,
      borderTop: BORDER_STYLE,
    },
  },
} as const;
