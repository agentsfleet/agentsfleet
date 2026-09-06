import { loadingPhrase, pickLoadingVerb } from "./loading-verbs";

// This must stay a Server Component. A route loading boundary is the first
// thing the router can paint; making its label a client reference means the
// fallback itself can wait for a route-specific JavaScript chunk. The RSC
// response carries this one picked phrase as plain text, so it is stable for
// the lifetime of that fallback and needs neither hydration nor a client chunk.
export function LoadingVerbLabel({ title }: { title?: string }) {
  return <span>{loadingPhrase(pickLoadingVerb(), title)}</span>;
}
