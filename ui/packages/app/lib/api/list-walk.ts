// Walk-to-exhaustion for client list reads that expose no paging controls:
// follow next_cursor until the server reports the end, so the caller renders
// the complete collection. The request bound stops a runaway cursor (a server
// bug echoing the same continuation forever) from spinning the walk — at the
// servers' default pages it covers thousands of rows, far past any real
// collection these callers read.
export const MAX_LIST_WALK_REQUESTS = 40;

export type ListPage<T> = {
  items: T[];
  total: number | null;
  next_cursor: string | null;
};

// `what` names the collection in the runaway error ("API key list") so the
// thrown message points at the endpoint that failed to terminate.
export async function walkList<T>(
  what: string,
  fetchPage: (cursor: string | null) => Promise<ListPage<T>>,
): Promise<{ items: T[]; total: number | null }> {
  const items: T[] = [];
  let total: number | null = null;
  let cursor: string | null = null;
  for (let requests = 0; requests < MAX_LIST_WALK_REQUESTS; requests += 1) {
    const page = await fetchPage(cursor);
    items.push(...page.items);
    total = page.total ?? total;
    if (page.next_cursor === null) return { items, total };
    cursor = page.next_cursor;
  }
  throw new Error(`the ${what} did not end after ${MAX_LIST_WALK_REQUESTS} pages`);
}
