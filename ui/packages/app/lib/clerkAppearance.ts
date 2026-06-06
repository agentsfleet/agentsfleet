/*
 * Clerk widget theming for sign-in / sign-up + the dashboard UserButton
 * avatar. Tokens map to the
 * Operational Restraint design system (docs/DESIGN_SYSTEM.md):
 *   --surface-1   cards
 *   --surface-2   inputs / elevated chrome
 *   --text*       text primary / muted / subtle
 *   --pulse       primary CTA fill ONLY (currency rule — the primary
 *                 action button is the system's "wake" affordance).
 *                 Footer links, resend-code links, edit buttons use
 *                 muted text — they are navigation, not live signals.
 *   --bg          contrast text on the pulse fill
 *   --border*     dividers + outlines
 *   --error       error states. Failed != live; never --pulse.
 * No box-shadow on chrome (spec: borders preferred over shadows).
 * No gradient on the footer (spec: no decorative gradients on chrome).
 */
export const AUTH_APPEARANCE = {
  variables: {
    colorBackground: "var(--surface-1)",
    colorInputBackground: "var(--surface-2)",
    colorInputText: "var(--text)",
    colorText: "var(--text)",
    colorTextSecondary: "var(--text-muted)",
    colorPrimary: "var(--pulse)",
    colorDanger: "var(--error)",
    borderRadius: "var(--r-sm)",
    fontFamily: "var(--ff-sans)",
  },
  elements: {
    // Dashboard header avatar (UserButton). With no uploaded image Clerk renders
    // an initials fallback — theme its fill + initials with design tokens so it
    // matches the app instead of Clerk's stock palette.
    userButtonAvatarBox: {
      backgroundColor: "var(--surface-2)",
      color: "var(--text)",
    },
    // UserButton dropdown (account menu). Without these the popover renders in
    // Clerk's stock light palette → invisible dark text on the app's dark
    // surface. Theme the card, the action rows, and the identity preview.
    userButtonPopoverCard: {
      backgroundColor: "var(--surface-1)",
      border: "1px solid var(--border-strong)",
    },
    userButtonPopoverActionButton: {
      color: "var(--text)",
    },
    userButtonPopoverActionButtonText: {
      color: "var(--text)",
    },
    userButtonPopoverActionButtonIcon: {
      color: "var(--text-muted)",
    },
    userButtonPopoverFooter: {
      backgroundColor: "var(--surface-1)",
      borderTop: "1px solid var(--border)",
    },
    userPreviewMainIdentifier: {
      color: "var(--text)",
    },
    userPreviewSecondaryIdentifier: {
      color: "var(--text-muted)",
    },
    // Mask self-service account deletion (UserProfile → Security). The robust
    // control is the instance-level toggle in the Clerk Dashboard; this hides
    // the UI section as a stopgap.
    profileSection__danger: {
      display: "none",
    },
    cardBox: {
      // --surface-2 over the page's --bg gives the card real visual lift on
      // the auth route — at --surface-1 (luminance delta = 3 units) the card
      // disappears into the background. --border-strong sharpens the edge.
      backgroundColor: "var(--surface-2)",
      border: "1px solid var(--border-strong)",
    },
    headerTitle: {
      color: "var(--text)",
    },
    headerSubtitle: {
      color: "var(--text-muted)",
    },
    socialButtonsBlockButton: {
      backgroundColor: "var(--surface-2)",
      border: "1px solid var(--border)",
      color: "var(--text)",
    },
    socialButtonsBlockButtonText: {
      color: "var(--text)",
    },
    dividerLine: {
      backgroundColor: "var(--border)",
    },
    dividerText: {
      color: "var(--text-subtle)",
    },
    formFieldLabel: {
      color: "var(--text)",
    },
    formFieldInput: {
      // Inputs sit ON the --surface-2 card (cardBox above). Filling them
      // with --surface-2 too left zero luminance delta — the field was
      // invisible until focused. Drop to --surface-1 (one step toward --bg)
      // for an inset well, and use --border-strong so the edge reads as a
      // click target without a focus event.
      backgroundColor: "var(--surface-1)",
      borderColor: "var(--border-strong)",
      color: "var(--text)",
    },
    formButtonPrimary: {
      backgroundColor: "var(--pulse)",
      color: "var(--bg)",
    },
    footerActionText: {
      color: "var(--text-muted)",
    },
    footerActionLink: {
      color: "var(--text)",
      textDecoration: "underline",
      textDecorationColor: "var(--border)",
    },
    identityPreviewText: {
      color: "var(--text)",
    },
    identityPreviewEditButton: {
      color: "var(--text-muted)",
    },
    formResendCodeLink: {
      color: "var(--text-muted)",
    },
    formFieldSuccessText: {
      color: "var(--success)",
    },
    formFieldErrorText: {
      color: "var(--error)",
    },
    alertText: {
      color: "var(--error)",
    },
    footer: {
      backgroundColor: "var(--surface-1)",
      borderTop: "1px solid var(--border)",
    },
  },
} as const;
