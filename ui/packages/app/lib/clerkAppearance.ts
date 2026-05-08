/*
 * Clerk widget theming for sign-in / sign-up. Tokens map to the
 * Operational Restraint design system (docs/DESIGN_SYSTEM.md):
 *   --surface-1   cards
 *   --surface-2   inputs / elevated chrome
 *   --text*       text primary / muted / subtle
 *   --pulse       primary CTA fill (currency — Clerk's primary action
 *                 IS the live signal: "you're about to wake the
 *                 platform's session")
 *   --bg          contrast text on the pulse fill
 *   --border*     dividers + outlines
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
    cardBox: {
      backgroundColor: "var(--surface-1)",
      border: "1px solid var(--border)",
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
      backgroundColor: "var(--surface-2)",
      borderColor: "var(--border)",
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
      color: "var(--pulse)",
    },
    identityPreviewText: {
      color: "var(--text)",
    },
    identityPreviewEditButton: {
      color: "var(--pulse)",
    },
    formResendCodeLink: {
      color: "var(--pulse)",
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
