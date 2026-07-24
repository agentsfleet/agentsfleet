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
const WARN = "var(--warn)";
const RADIUS_SM = "var(--r-sm)";
const FONT_SANS = "var(--ff-sans)";
const BORDER_STYLE = `1px solid ${BORDER}`;
const BORDER_STRONG_STYLE = `1px solid ${BORDER_STRONG}`;
const FOCUS_RING = `0 0 0 1px ${PULSE}`;

const AUTH_INPUT_APPEARANCE = {
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
} as const;

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
  // Colors are `var()` refs to the design-system tokens (tokens.css) — Clerk's
  // own themes declare `variables` the same way (e.g. `colorForeground:
  // "var(--card-foreground)"`), so custom properties resolve fine here; there
  // is no JS color parsing that would need hex literals.
  //
  // The key names are the Clerk v7 set (colorForeground / colorMutedForeground /
  // colorInput / colorInputForeground / colorPrimaryForeground / colorBorder).
  // The pre-v7 keys (colorText, colorTextSecondary, colorInputBackground,
  // colorInputText) are silently ignored by v7 — that was the "invisible
  // account-modal text": those shades fell back to Clerk defaults, dark-on-dark.
  variables: {
    colorBackground: SURFACE_1,
    colorInput: SURFACE_2,
    colorInputForeground: TEXT,
    colorForeground: TEXT,
    colorMutedForeground: TEXT_MUTED,
    colorPrimary: PULSE,
    colorPrimaryForeground: BACKGROUND,
    colorBorder: BORDER_STRONG,
    colorDanger: ERROR,
    colorSuccess: SUCCESS,
    colorWarning: WARN,
    borderRadius: RADIUS_SM,
    fontFamily: FONT_SANS,
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
    // The modal's close (X) button rendered in a near-black default on the
    // dark surface — invisible. Pin it readable, with a hover fill matching
    // the other chrome actions.
    modalCloseButton: {
      color: TEXT,
      "&:hover": MENU_ACTION_INTERACTION,
    },
    pageScrollBox: {
      backgroundColor: SURFACE_1,
    },
    navbar: {
      backgroundColor: SURFACE_1,
      borderRight: BORDER_STYLE,
    },
    // The account modal's left nav ("Account" / "Security"). At TEXT_MUTED the
    // inactive tab label sat too dim to read on the dark surface; pin it to the
    // primary token — the active tab still stands out via its SURFACE_3 fill.
    navbarButton: {
      backgroundColor: "transparent",
      color: TEXT,
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
    // Profile row values — the email address under "Email addresses", the
    // "Chrome on macOS" line under "Active devices" — render as Clerk's
    // secondary text, which is near-invisible on the dark modal surface. Pin the
    // section content + item rows to the primary readable token.
    profileSectionContent: {
      color: TEXT,
    },
    profileSectionItem: {
      borderColor: BORDER,
      color: TEXT,
    },
    profileSectionItemList: {
      borderColor: BORDER,
    },
    // Inline chips ("Primary" on an email, "This device" on a session) default
    // to Clerk's light-palette badge → invisible on dark; give them real tokens.
    badge: {
      backgroundColor: SURFACE_2,
      color: TEXT,
      border: BORDER_STYLE,
    },
    // Active-device rows can sit inside an accordion; keep the trigger legible.
    accordionTriggerButton: {
      color: TEXT,
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
      ...AUTH_INPUT_APPEARANCE,
      "&::placeholder": {
        color: TEXT_SUBTLE,
      },
    },
    // Clerk renders email verification as six segmented inputs under a
    // separate element key. Reuse the ordinary input treatment so every code
    // slot remains visible before focus on the dark card.
    otpCodeFieldInput: AUTH_INPUT_APPEARANCE,
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
