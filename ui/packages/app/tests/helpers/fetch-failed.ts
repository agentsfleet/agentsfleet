/** What Node's `fetch` throws for a socket failure: a `TypeError` whose `cause` carries the code. */
export function fetchFailed(code: string): TypeError {
  return new TypeError("fetch failed", { cause: Object.assign(new Error(code), { code }) });
}
