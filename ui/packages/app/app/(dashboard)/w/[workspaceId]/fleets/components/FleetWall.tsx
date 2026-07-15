"use client";

import { useDeferredValue, useMemo, useState, useTransition } from "react";
import Link from "next/link";
import { PlusIcon } from "lucide-react";
import {
  Alert,
  Button,
  cn,
  EYEBROW_CLASS,
  Input,
  TooltipButton,
  WakePulse,
} from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS, type Fleet } from "@/lib/api/fleets";
import { listFleetsAction } from "../actions";
import { workspacePath } from "@/lib/workspace-routes";
import { presentErrorString } from "@/lib/errors";
import { INSTALL_FLEET_TOOLTIP } from "../new/library-docs";
import FleetTile from "./FleetTile";

type Props = {
  workspaceId: string;
  initialFleets: Fleet[];
  initialCursor: string | null;
};

// The wall over the fleet list. Every rendered fleet gets a tile; a tile beyond
// the loaded page has no tile yet and opens no stream (a live tile streams only
// once rendered), so the load-more affordance below is the stream-count bound —
// the wall never opens more streams than it has rendered tiles.
export default function FleetWall({ workspaceId, initialFleets, initialCursor }: Props) {
  const [fleets, setFleets] = useState<Fleet[]>(initialFleets);
  const [cursor, setCursor] = useState<string | null>(initialCursor);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const deferredQuery = useDeferredValue(query);

  const filtered = useMemo(() => {
    const q = deferredQuery.trim().toLowerCase();
    if (!q) return fleets;
    return fleets.filter(
      (z) =>
        z.name.toLowerCase().includes(q) ||
        z.id.toLowerCase().includes(q) ||
        z.status.toLowerCase().includes(q),
    );
  }, [fleets, deferredQuery]);

  const liveTotal = useMemo(
    () => filtered.filter((z) => z.status === AGENTSFLEET_STATUS.ACTIVE).length,
    [filtered],
  );

  function loadMore(next: string) {
    setError(null);
    startTransition(async () => {
      const result = await listFleetsAction(workspaceId, { cursor: next });
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "load more fleets",
          }),
        );
        return;
      }
      setFleets((prev) => [...prev, ...result.data.items]);
      setCursor(result.data.cursor);
    });
  }

  return (
    <>
      <div className="mb-4 flex items-center gap-3">
        <Input
          type="search"
          placeholder="Search loaded fleets by name, status, or id…"
          value={query}
          onChange={(e) => setQuery(e.currentTarget.value)}
          aria-label="Search fleets"
          className="flex-1"
        />
        {liveTotal > 0 ? (
          <span
            className={cn(EYEBROW_CLASS, "text-muted-foreground inline-flex items-center gap-2")}
            aria-label={`${liveTotal} live`}
          >
            <WakePulse
              live
              className="inline-block w-2 h-2 rounded-full bg-pulse"
              aria-hidden="true"
            />
            {liveTotal} live
          </span>
        ) : null}
        <TooltipButton asChild size="sm" tooltip={INSTALL_FLEET_TOOLTIP}>
          <Link href={workspacePath(workspaceId, "fleets/new")}>
            <PlusIcon size={14} /> Install fleet
          </Link>
        </TooltipButton>
      </div>

      {filtered.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No fleets match &ldquo;{deferredQuery}&rdquo; in the loaded set.
        </p>
      ) : (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {filtered.map((z) => (
            <FleetTile key={z.id} fleet={z} workspaceId={workspaceId} />
          ))}
        </div>
      )}

      {error ? (
        <Alert variant="destructive" className="mt-3">{error}</Alert>
      ) : null}

      {cursor ? (
        <div className="mt-4 flex justify-center">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => loadMore(cursor)}
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
