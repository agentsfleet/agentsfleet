// Canonical user-facing definition of an agent — the product's primary noun.
// Faithful restatement of docs/architecture/direction.md ("a durable runtime,
// not a one-shot prompt"). Single source for the marketing site so every
// first-touch surface renders identical wording (UFS). The app package mirrors
// these exact identifiers + strings in ui/packages/app/lib/copy.ts.

export const AGENT_SHORT_GLOSS =
  "An agent wakes on an event, runs your skill, and reports back.";

export const AGENT_DEFINITION =
  "An agent is a long-lived runtime you install once. It sleeps until an " +
  "event wakes it, runs your skill against that event, and reports back with " +
  "evidence — durable and autonomous, not a one-shot prompt.";
