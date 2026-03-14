export const AUTH_APPEARANCE = {
  variables: {
    colorBackground: "var(--z-surface-0)",
    colorInputBackground: "var(--z-surface-1)",
    colorInputText: "var(--z-text-primary)",
    colorText: "var(--z-text-primary)",
    colorTextSecondary: "var(--z-text-muted)",
    colorPrimary: "var(--z-orange)",
    colorDanger: "var(--z-red)",
    borderRadius: "var(--z-radius-sm)",
    fontFamily: "var(--z-font-sans)",
  },
  elements: {
    cardBox: {
      backgroundColor: "var(--z-surface-0)",
      border: "1px solid var(--z-border)",
      boxShadow: "0 18px 48px rgba(0, 0, 0, 0.32)",
    },
    headerTitle: {
      color: "var(--z-text-primary)",
    },
    headerSubtitle: {
      color: "var(--z-text-muted)",
    },
    socialButtonsBlockButton: {
      backgroundColor: "var(--z-surface-1)",
      border: "1px solid var(--z-border)",
      color: "var(--z-text-primary)",
    },
    socialButtonsBlockButtonText: {
      color: "var(--z-text-primary)",
    },
    dividerLine: {
      backgroundColor: "var(--z-border)",
    },
    dividerText: {
      color: "var(--z-text-dim)",
    },
    formFieldLabel: {
      color: "var(--z-text-primary)",
    },
    formFieldInput: {
      backgroundColor: "var(--z-surface-1)",
      borderColor: "var(--z-border)",
      color: "var(--z-text-primary)",
    },
    formButtonPrimary: {
      backgroundColor: "var(--z-orange)",
      color: "var(--z-text-inverse)",
      boxShadow: "0 0 24px var(--z-glow-strong)",
    },
    footerActionText: {
      color: "var(--z-text-muted)",
    },
    footerActionLink: {
      color: "var(--z-orange-bright)",
    },
    identityPreviewText: {
      color: "var(--z-text-primary)",
    },
    identityPreviewEditButton: {
      color: "var(--z-orange-bright)",
    },
    formResendCodeLink: {
      color: "var(--z-orange-bright)",
    },
    formFieldSuccessText: {
      color: "var(--z-green)",
    },
    formFieldErrorText: {
      color: "var(--z-red)",
    },
    alertText: {
      color: "var(--z-red)",
    },
    footer: {
      background:
        "linear-gradient(180deg, rgba(15, 21, 32, 0.94) 0%, rgba(11, 16, 24, 0.98) 100%)",
      borderTop: "1px solid rgba(26, 37, 51, 0.5)",
    },
  },
} as const;
