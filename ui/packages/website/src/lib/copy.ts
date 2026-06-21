// Canonical user-facing definition of a Fleet — the product's primary runtime noun.
// Faithful restatement of docs/architecture/direction.md ("a durable runtime,
// not a one-shot prompt"). Single source for the marketing site so every
// first-touch surface renders identical wording (Unified Field Semantics). The app package mirrors
// these exact identifiers + strings in ui/packages/app/lib/copy.ts.

export const FLEET_SHORT_GLOSS =
  "A Fleet wakes on an event, runs your skill, and reports back.";

export const FLEET_DEFINITION =
  "A Fleet is a long-lived runtime you install once. It sleeps until an " +
  "event wakes it, runs your skill against that event, and reports back with " +
  "evidence — durable and autonomous, not a one-shot prompt.";
