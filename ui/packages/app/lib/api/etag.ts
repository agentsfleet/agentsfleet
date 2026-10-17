const WEAK_ETAG_PREFIX = "W/";

/** Compression may weaken the HTTP validator; conditional writes retain the body's resource tag. */
export function matchesResourceEtag(resourceTag: string, responseTag: string | null): boolean {
  return !resourceTag.startsWith(WEAK_ETAG_PREFIX)
    && (responseTag === resourceTag || responseTag === `${WEAK_ETAG_PREFIX}${resourceTag}`);
}
