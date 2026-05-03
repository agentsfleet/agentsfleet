"use client";

import { useDeferredValue, useMemo, useState, useTransition } from "react";
import Link from "next/link";
import { Alert, Button, Input, List, ListItem } from "@usezombie/design-system";
import { useClientToken } from "@/lib/auth/client";
import { listZombies, type Zombie } from "@/lib/api/zombies";

type Props = {
  workspaceId: string;
  initialZombies: Zombie[];
  initialCursor: string | null;
};

export default function ZombiesList({
  workspaceId,
  initialZombies,
  initialCursor,
}: Props) {
  const { getToken } = useClientToken();
  const [zombies, setZombies] = useState<Zombie[]>(initialZombies);
  const [cursor, setCursor] = useState<string | null>(initialCursor);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const deferredQuery = useDeferredValue(query);

  const filtered = useMemo(() => {
    const q = deferredQuery.trim().toLowerCase();
    if (!q) return zombies;
    return zombies.filter(
      (z) =>
        z.name.toLowerCase().includes(q) ||
        z.id.toLowerCase().includes(q) ||
        z.status.toLowerCase().includes(q),
    );
  }, [zombies, deferredQuery]);

  function loadMore() {
    if (!cursor) return;
    setError(null);
    startTransition(async () => {
      const token = await getToken();
      if (!token) {
        setError("Not authenticated");
        return;
      }
      try {
        const next = await listZombies(workspaceId, token, { cursor });
        setZombies((prev) => [...prev, ...next.items]);
        setCursor(next.cursor);
      } catch (e) {
        const err = e as Error;
        setError(err.message || "Failed to load more");
      }
    });
  }

  return (
    <>
      <div className="mb-4">
        <Input
          type="search"
          placeholder="Search loaded zombies by name, status, or id…"
          value={query}
          onChange={(e) => setQuery(e.currentTarget.value)}
          aria-label="Search zombies"
        />
      </div>

      {filtered.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No zombies match &ldquo;{deferredQuery}&rdquo; in the loaded set.
        </p>
      ) : (
        <List
          variant="plain"
          className="divide-y divide-border rounded-lg border border-border space-y-0"
        >
          {filtered.map((z) => (
            <ListItem key={z.id}>
              <Link
                href={`/zombies/${z.id}`}
                className="flex items-center justify-between px-4 py-3 hover:bg-muted/40"
              >
                <div>
                  <div className="font-medium">{z.name}</div>
                  <div className="text-xs uppercase tracking-wide text-muted-foreground">
                    {z.status}
                  </div>
                </div>
                <div className="font-mono text-xs text-muted-foreground">
                  {z.id}
                </div>
              </Link>
            </ListItem>
          ))}
        </List>
      )}

      {error ? (
        <Alert variant="destructive" className="mt-3">{error}</Alert>
      ) : null}

      {cursor ? (
        <div className="mt-4 flex justify-center">
          <Button
            variant="ghost"
            size="sm"
            onClick={loadMore}
            disabled={pending}
            aria-busy={pending}
          >
            {pending ? "Loading…" : "Load more"}
          </Button>
        </div>
      ) : null}
    </>
  );
}
