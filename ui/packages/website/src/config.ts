import { DEFAULT_WAITLIST_URL } from "./lib/site-links";
export * from "./lib/site-links";

export const WAITLIST_URL = import.meta.env.VITE_WAITLIST_URL?.trim() || DEFAULT_WAITLIST_URL;
// Every public hosted-access surface switches destination here when access opens.
export const PUBLIC_ACCESS_URL = WAITLIST_URL;
// NOTE: the canonical contact address is SUPPORT_EMAIL ("agentsfleet@agentmail.to")
// in src/lib/contact.ts — used by Pricing, Terms, and Privacy. A second
// "team@agentsfleet.net" constant used to live here, unused by any component
// and contradicting the canonical address, so it was removed.
export const MARKETING_LEAD_CAPTURE_URL = import.meta.env.VITE_MARKETING_LEAD_CAPTURE_URL?.trim() || "";
