import { ClerkProvider, Waitlist } from "@clerk/clerk-react";
import { CLERK_PUBLISHABLE_KEY } from "../config";

/*
 * Self-hosted Clerk <Waitlist>, embedded on the public marketing site so the
 * product app (app.agentsfleet.net) stays closed: there is no dashboard on
 * this domain to gain access to, so surfacing the waitlist here cannot leak
 * product access (see config.ts WAITLIST_URL for the full rationale).
 *
 * Two deliberate departures from Clerk's defaults:
 *   - the form is themed with the design-system tokens to match the dark/mint
 *     brand instead of Clerk's stock palette;
 *   - footerAction (Clerk's "Already have access? Sign in" row) is hidden —
 *     nobody has access pre-launch, so the link would dead-end.
 *
 * Lazy-loaded by App.tsx: Clerk's client runtime only loads when a visitor
 * actually opens /waitlist, keeping the landing page's first load lean.
 */
const WAITLIST_APPEARANCE = {
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
    // Lift the card off the dotted page background (mirrors the app's auth
    // appearance: card at --surface-2, inputs inset to --surface-1).
    cardBox: {
      backgroundColor: "var(--surface-2)",
      border: "1px solid var(--border-strong)",
    },
    headerTitle: { color: "var(--text)" },
    headerSubtitle: { color: "var(--text-muted)" },
    formFieldLabel: { color: "var(--text)" },
    formFieldInput: {
      backgroundColor: "var(--surface-1)",
      borderColor: "var(--border-strong)",
      color: "var(--text)",
    },
    // --pulse is the currency colour: reserved for the single live CTA fill.
    formButtonPrimary: {
      backgroundColor: "var(--pulse)",
      color: "var(--bg)",
    },
    formFieldSuccessText: { color: "var(--success)" },
    formFieldErrorText: { color: "var(--error)" },
    footer: {
      backgroundColor: "var(--surface-1)",
      borderTop: "1px solid var(--border)",
    },
    // "Already have access? Sign in" row — hidden. Nobody has access yet.
    footerAction: { display: "none" },
  },
} as const;

export default function WaitlistPage() {
  return (
    <section className="wrap flex justify-center py-16">
      <ClerkProvider publishableKey={CLERK_PUBLISHABLE_KEY}>
        <Waitlist appearance={WAITLIST_APPEARANCE} afterJoinWaitlistUrl="/" />
      </ClerkProvider>
    </section>
  );
}
