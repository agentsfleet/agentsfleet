"use client";

import { useMemo, useState, useTransition } from "react";
import {
  Alert,
  Button,
  SectionHeader,
} from "@agentsfleet/design-system";
import { type Fleet } from "@/lib/api/fleets";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { WorkspaceStreamProvider } from "@/components/domain/useWorkspaceStream";
import { listFleetsAction } from "../actions";
import { presentErrorString } from "@/lib/errors";
import { useRememberFleets } from "@/components/layout/RecentFleets";
import FleetTile from "./FleetTile";
import WallLiveBadge from "./WallLiveBadge";
import { tileShouldStream } from "@/lib/wall/tile-liveness";

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
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  useRememberFleets(workspaceId, fleets);
  const liveTotal = useMemo(
    () => fleets.filter((z) => z.status === AGENTSFLEET_STATUS.ACTIVE).length,
    [fleets],
  );
  const streamFleetIds = useMemo(
    () => fleets.filter((z) => tileShouldStream(z.status)).map((z) => z.id),
    [fleets],
  );

  function loadMore(next: string) {
    setError(null);
    startTransition(async () => {
      const result = await listFleetsAction(workspaceId, { starting_after: next });
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
      setCursor(result.data.next_cursor);
    });
  }

  if (fleets.length === 0 && !cursor && !error) return null;

  return (
    <WorkspaceStreamProvider workspaceId={workspaceId} fleetIds={streamFleetIds}>
      <FleetWallContent workspaceId={workspaceId} fleets={fleets} cursor={cursor}
        error={error} pending={pending} liveTotal={liveTotal} loadMore={loadMore} />
    </WorkspaceStreamProvider>
  );
}

type ContentProps = {
  workspaceId: string;
  fleets: Fleet[];
  cursor: string | null;
  error: string | null;
  pending: boolean;
  liveTotal: number;
  loadMore: (cursor: string) => void;
};

function FleetWallContent({ workspaceId, fleets, cursor, error, pending, liveTotal, loadMore }: ContentProps) {
  return (
    <div className="grid gap-xl">
      {fleets.length > 0 ? (
        <SectionHeader as="p" actions={<WallLiveBadge liveTotal={liveTotal} />}>
          Your fleets
        </SectionHeader>
      ) : null}
      <div>
        <div className="grid grid-cols-1 gap-lg sm:grid-cols-2 xl:grid-cols-3">
          {fleets.map((fleet) => <FleetTile key={fleet.id} fleet={fleet} workspaceId={workspaceId} />)}
        </div>
        {error ? <Alert variant="destructive" className="mt-3">{error}</Alert> : null}
        {cursor ? (
          <div className="mt-4 flex justify-center">
            <Button variant="ghost" size="sm" onClick={() => loadMore(cursor)}
              disabled={pending} aria-busy={pending}>
              {pending ? "Loading…" : "Load more"}
            </Button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
