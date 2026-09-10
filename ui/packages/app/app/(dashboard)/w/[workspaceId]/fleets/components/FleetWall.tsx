"use client";

import { useCallback, useMemo, useState, useTransition } from "react";
import Link from "next/link";
import { PlusIcon } from "lucide-react";
import {
  Alert,
  Button,
  SectionHeader,
  TooltipButton,
} from "@agentsfleet/design-system";
import { type Fleet } from "@/lib/api/fleets";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { WorkspaceStreamProvider } from "@/components/domain/useWorkspaceStream";
import { listFleetsAction } from "../actions";
import { workspacePath } from "@/lib/workspace-routes";
import { presentErrorString } from "@/lib/errors";
import { INSTALL_FLEET_TOOLTIP } from "../new/library-docs";
import FleetTile from "./FleetTile";
import WallLiveBadge from "./WallLiveBadge";
import { tileShouldStream } from "@/lib/wall/tile-liveness";

// One page is enough: the read exists to correct the rows already on screen,
// and a wall showing more than this has paginated, whose later pages keep the
// counters they were fetched with until they are re-read in turn.
const SUMMARY_RELOAD_LIMIT = 100;

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

  const liveTotal = useMemo(
    () => fleets.filter((z) => z.status === AGENTSFLEET_STATUS.ACTIVE).length,
    [fleets],
  );
  const streamFleetIds = useMemo(
    () => fleets.filter((z) => tileShouldStream(z.status)).map((z) => z.id),
    [fleets],
  );

  /**
   * Re-read the fleet summaries because a frame could not price a settled row.
   *
   * One small read of the rows already on screen — not `router.refresh()`,
   * which would re-run every server component on the route and reconcile the
   * whole tree to move two numbers. Existing rows are replaced in place so a
   * page loaded through "Load more" is not thrown away, and the ids that came
   * back are returned so the stream can drop the rows this base now accounts
   * for.
   */
  const reloadSummaries = useCallback(async (): Promise<readonly string[]> => {
    const result = await listFleetsAction(workspaceId, { limit: SUMMARY_RELOAD_LIMIT });
    if (!result.ok) return [];
    const fresh = new Map(result.data.items.map((z) => [z.id, z]));
    setFleets((prev) => prev.map((z) => fresh.get(z.id) ?? z));
    return result.data.items.map((z) => z.id);
  }, [workspaceId]);

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

  return (
    <WorkspaceStreamProvider workspaceId={workspaceId} fleetIds={streamFleetIds}>
      <div className="grid gap-xl">
      <SectionHeader
        actions={
          <div className="flex items-center gap-3">
            <WallLiveBadge liveTotal={liveTotal} onStaleCounters={reloadSummaries} />
            <TooltipButton asChild size="sm" tooltip={INSTALL_FLEET_TOOLTIP}>
              <Link href={workspacePath(workspaceId, "fleets/new")}>
                <PlusIcon size={14} /> Install fleet
              </Link>
            </TooltipButton>
          </div>
        }
      >
        Manage fleets
      </SectionHeader>

      <div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {fleets.map((z) => (
            <FleetTile key={z.id} fleet={z} workspaceId={workspaceId} />
          ))}
        </div>

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
      </div>
      </div>
    </WorkspaceStreamProvider>
  );
}
