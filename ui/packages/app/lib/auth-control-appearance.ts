const FOCUS = { outline: "2px solid var(--pulse)", outlineOffset: "4px", boxShadow: "none" } as const;

export const AUTH_INPUT_APPEARANCE = {
  // Clerk's state selectors outrank a single class selector.
  "&&": {
    backgroundColor: "var(--bg)", border: "1px solid var(--border)",
    borderWidth: "1px", borderColor: "var(--border)", borderRadius: "var(--r-control)",
    minHeight: "44px", padding: "var(--sp-sm) var(--sp-lg)",
    fontSize: "var(--fs-body-sm)", lineHeight: "var(--lh-body-sm)",
    boxShadow: "none", color: "var(--text)",
  },
  "&&:hover:not(:disabled):not(:focus)": { borderColor: "var(--pulse)", boxShadow: "none" },
  "&&:focus": { borderColor: "var(--border)", boxShadow: "none" },
  "&&:focus-visible": FOCUS,
} as const;

export const AUTH_BUTTON_APPEARANCE = {
  "&&": {
    minHeight: "var(--control-height)", borderRadius: "var(--r-control)",
    padding: "var(--sp-sm) var(--sp-lg)", fontSize: "var(--fs-body-sm)",
    lineHeight: "var(--lh-body-sm)", fontWeight: 500, boxShadow: "none",
  },
  "&&:focus-visible": FOCUS,
} as const;
