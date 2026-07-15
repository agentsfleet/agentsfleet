"use client";

import Link from "next/link";
import { Card, cn, EYEBROW_CLASS, Time, WakePulse } from "@agentsfleet/design-system";
import { type Fleet } from "@/lib/api/fleets";
import { workspacePath } from "@/lib/workspace-routes";
import { useFleetEventStream } from "@/components/domain/useFleetEventStream";
import {
  deriveTileLiveness,
  formatTileEvents,
  formatTileSpend,
  tileShouldStream,
  type TileKind,
} from "@/lib/wall/tile-liveness";

type Props = { fleet: Fleet; workspaceId: string };

// One tile. The status decides the whole subtree before any hook runs: a
// drained fleet renders `DrainedTile`, which never calls the streaming hook, so
// a parked or killed fleet opens no stream at all (Dimension 1.3). A live fleet
// renders `StreamingTile`, which subscribes and then shows either `live` or
// `snapshot` — never blank. The tile is always a link to its console (all
// kinds), so no tile is ever a dead end.
export default function FleetTile({ fleet, workspaceId }: Props) {
  if (!tileShouldStream(fleet.status)) {
    return <DrainedTile fleet={fleet} workspaceId={workspaceId} />;
  }
  return <StreamingTile fleet={fleet} workspaceId={workspaceId} />;
}

function DrainedTile({ fleet, workspaceId }: Props) {
  return (
    <TileShell fleet={fleet} workspaceId={workspaceId} kind="drained">
      <span
        className="inline-block w-2 h-2 rounded-full bg-muted-foreground"
        aria-hidden="true"
      />
    </TileShell>
  );
}

function StreamingTile({ fleet, workspaceId }: Props) {
  // Reuses the console's per-fleet stream registry — the tile is a second
  // consumer of the one EventSource, not a second SSE path. No SSR events on the
  // wall, so the seed is empty; frames fill in as they arrive.
  const { events, connectionStatus } = useFleetEventStream(
    workspaceId,
    fleet.id,
    [],
  );
  const liveness = deriveTileLiveness(fleet.status, connectionStatus);
  const lastEvent = events.length > 0 ? events[events.length - 1] : null;

  return (
    <TileShell
      fleet={fleet}
      workspaceId={workspaceId}
      kind={liveness.kind}
      eyebrow={liveness.kind === "snapshot" ? "snapshot" : undefined}
      feed={lastEvent?.text}
    >
      <WakePulse
        live
        className={cn(
          "inline-block w-2 h-2 rounded-full",
          fleet.status === "installing" ? "bg-info" : "bg-pulse",
        )}
        aria-hidden="true"
      />
    </TileShell>
  );
}

type ShellProps = {
  fleet: Fleet;
  workspaceId: string;
  kind: TileKind;
  eyebrow?: string;
  feed?: string;
  children: React.ReactNode;
};

function TileShell({ fleet, workspaceId, kind, eyebrow, feed, children }: ShellProps) {
  return (
    <Card
      className={cn("p-4", kind === "drained" && "opacity-60")}
      data-kind={kind}
    >
      <Link
        href={workspacePath(workspaceId, `fleets/${fleet.id}`)}
        className="absolute inset-0 rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-label={`${fleet.name} — ${fleet.status} — ${fleet.id}`}
      />
      <div className="pointer-events-none flex flex-col gap-3">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="font-medium truncate">{fleet.name}</div>
            <div className={cn(EYEBROW_CLASS, "text-muted-foreground")}>
              {fleet.status}
            </div>
          </div>
          <div className="flex items-center gap-2">
            {eyebrow ? (
              <span className={cn(EYEBROW_CLASS, "text-text-subtle")}>{eyebrow}</span>
            ) : null}
            {children}
          </div>
        </div>

        <div className="min-h-[1.25rem] font-mono text-xs text-muted-foreground truncate">
          {feed ? feed : <>&nbsp;</>}
        </div>

        <div className="flex items-center justify-between font-mono text-xs text-muted-foreground tabular-nums">
          <span title="Lifetime spend (server truth)">
            {formatTileSpend(fleet.budget_used_nanos)}
          </span>
          <span title="Lifetime events processed">
            {formatTileEvents(fleet.events_processed)} ev
          </span>
          <Time value={new Date(fleet.updated_at)} format="relative" tooltip={false} />
        </div>
      </div>
    </Card>
  );
}
