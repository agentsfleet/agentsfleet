"use client";

import { useMemo, useState, useTransition } from "react";
import Link from "next/link";
import { PlusIcon } from "lucide-react";
import {
  Alert,
  Button,
  cn,
  EYEBROW_CLASS,
  SectionHeader,
  TooltipButton,
  WakePulse,
} from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS, type Fleet } from "@/lib/api/fleets";
import { WorkspaceStreamProvider } from "@/components/domain/useWorkspaceStream";
import { listFleetsAction } from "../actions";
import { workspacePath } from "@/lib/workspace-routes";
import { presentErrorString } from "@/lib/errors";
import { INSTALL_FLEET_TOOLTIP } from "../new/library-docs";
import FleetTile from "./FleetTile";
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
    <div className="grid gap-xl">
      <SectionHeader
        actions={
          <div className="flex items-center gap-3">
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
        }
      >
        Manage fleets
      </SectionHeader>

      <div>
        <WorkspaceStreamProvider workspaceId={workspaceId} fleetIds={streamFleetIds}>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {fleets.map((z) => (
              <FleetTile key={z.id} fleet={z} workspaceId={workspaceId} />
            ))}
          </div>
        </WorkspaceStreamProvider>

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
  );
}
