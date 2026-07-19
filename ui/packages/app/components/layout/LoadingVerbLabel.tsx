"use client";

import { useState } from "react";

import { loadingPhrase, pickLoadingVerb } from "./loading-verbs";

/*
 * The randomly-verbed loader text. Client-only because the pick is impure: this
 * is the one node in the loading chrome whose server and client renders may
 * legitimately disagree.
 *
 * `useState(pickLoadingVerb)` freezes the verb at mount, so the word never
 * changes underneath a reader mid-wait — a rotating word would also retext the
 * role=status live region on a timer and make assistive tech re-announce.
 *
 * `suppressHydrationWarning` is load-bearing, not a papered-over bug: on a hard
 * page load the server streams this fallback with one verb and the client picks
 * another, which is exactly the intended behaviour and the case the escape hatch
 * exists for. React keeps the server's word on that path; on client-side
 * navigation the loader renders fresh and picks its own. Both paths land on a
 * random verb, and neither logs a mismatch.
 */
export function LoadingVerbLabel({ title }: { title?: string }) {
  const [verb] = useState(pickLoadingVerb);

  return <span suppressHydrationWarning>{loadingPhrase(verb, title)}</span>;
}
