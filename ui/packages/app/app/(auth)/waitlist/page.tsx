import { AuthWaitlist } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

// Self-hosted Clerk <Waitlist>. The marketing site's "Get early access" CTAs
// point here (WAITLIST_URL → app /waitlist), and the app's own sign-in page
// routes its "Join the waitlist" link here too (ClerkProvider waitlistUrl in
// app/layout.tsx). Self-hosting gives deterministic control the hosted Account
// Portal can't: hide the footerAction row (Clerk's "Already have access? Sign
// in" link — nobody has access pre-launch) and apply our mint/dark
// AUTH_APPEARANCE. The provider wrapping is inherited from the root layout, so
// the page only renders the widget.
//
// No catch-all segment (unlike sign-in/sign-up): <Waitlist> is a single form
// with no Clerk-routed sub-paths.
export default function WaitlistPage() {
  return (
    <AuthWaitlist
      appearance={{
        ...AUTH_APPEARANCE,
        elements: {
          ...AUTH_APPEARANCE.elements,
          // "Already have access? Sign in" row — hidden.
          footerAction: { display: "none" },
        },
      }}
    />
  );
}
