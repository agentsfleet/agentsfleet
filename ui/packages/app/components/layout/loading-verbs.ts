// The waiting vocabulary. One verb is picked per loader mount (see
// LoadingVerbLabel) instead of a single static "Loading", so a wait reads as the
// product doing something rather than the page being stuck.
//
// Two flavours on purpose: fleet-metaphor verbs that describe what the platform
// actually does to fleets and runners, and neutral whimsical ones that carry the
// wait when no fleet noun follows. Every entry must read correctly in BOTH
// slots — "<verb> Fleets…" and a bare "<verb>…" — which is why they are all
// present-participle and none take an object ("Fetching" would strand a reader
// on the workspace home, "Provisioning" would over-claim).
export const LOADING_VERBS = [
  "Wrangling",
  "Percolating",
  "Herding",
  "Jittering",
  "Marshalling",
  "Noodling",
  "Spooling",
  "Simmering",
  "Mustering",
  "Corralling",
  "Puttering",
  "Rallying",
  "Churning",
  "Brewing",
  "Convening",
] as const;

export type LoadingVerb = (typeof LOADING_VERBS)[number];

/** Uniformly picks one waiting verb. Impure by design — callers freeze it in mount state. */
export function pickLoadingVerb(): LoadingVerb {
  const index = Math.floor(Math.random() * LOADING_VERBS.length);
  // The `as const` tuple makes index 0 statically known, so this coalesce is a
  // real total-function fallback rather than a non-null assertion in disguise:
  // a future out-of-range index degrades to a valid verb instead of `undefined`
  // leaking into the rendered phrase.
  return LOADING_VERBS[index] ?? LOADING_VERBS[0];
}

/**
 * The visible loader copy. `title` present → "Wrangling Fleets…"; absent → the
 * bare "Wrangling…" the multi-route dashboard fallback uses, since it stands in
 * for many routes and must not claim a specific one.
 */
export function loadingPhrase(verb: string, title?: string): string {
  return title ? `${verb} ${title}…` : `${verb}…`;
}

/**
 * The stable accessible name, deliberately NOT the random phrase. Assistive tech
 * announces this instead of the visible whimsy, so a screen-reader user hears
 * "Loading Fleets" every time rather than decoding "Percolating".
 */
export function loadingAccessibleName(title?: string): string {
  return title ? `Loading ${title}` : "Loading";
}
