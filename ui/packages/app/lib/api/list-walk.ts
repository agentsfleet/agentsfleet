// Walk-to-exhaustion for client list reads that expose no paging controls:
// follow next_cursor until the server reports the end, so the caller renders
// the complete collection. The walk counts no pages; it fails only when the
// server repeats a cursor it already followed, which would replay forever.

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
  const followed = new Set<string>();
  for (;;) {
    const page = await fetchPage(cursor);
    items.push(...page.items);
    total = page.total ?? total;
    if (page.next_cursor === null) return { items, total };
    if (followed.has(page.next_cursor)) throw new Error(`the ${what} repeated page cursor ${page.next_cursor}`);
    followed.add(page.next_cursor);
    cursor = page.next_cursor;
  }
}
