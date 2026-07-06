"use client";

import { useDeferredValue, useMemo, useState, useTransition } from "react";
import Link from "next/link";
import { PlusIcon } from "lucide-react";
import {
  Alert,
  Button,
  buttonClassName,
  EYEBROW_CLASS,
  Input,
  List,
  ListItem,
  Time,
  WakePulse,
} from "@agentsfleet/design-system";
import { cn } from "@/lib/utils";
import { AGENTSFLEET_STATUS, type Fleet } from "@/lib/api/fleets";
import { listFleetsAction } from "../actions";
import { workspacePath } from "@/lib/workspace-routes";
import { presentErrorString } from "@/lib/errors";

type Props = {
  workspaceId: string;
  initialFleets: Fleet[];
  initialCursor: string | null;
};

const PULSE_CAP = 5;

type LiveState = "live" | "installing" | "parked" | "failed";

function liveStateOf(status: string): LiveState {
  if (status === AGENTSFLEET_STATUS.ACTIVE) return "live";
  if (status === AGENTSFLEET_STATUS.INSTALLING) return "installing";
  if (status === AGENTSFLEET_STATUS.KILLED) return "failed";
  return "parked";
}

export default function FleetsList({
  workspaceId,
  initialFleets,
  initialCursor,
}: Props) {
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

  // Pulse currency cap: only the first N live rows in render order pulse.
  // Beyond the cap we render a static glow + the header consolidation count.
  const liveTotal = useMemo(
    () => filtered.filter((z) => liveStateOf(z.status) === "live").length,
    [filtered],
  );
  const cappedLiveIds = useMemo(() => {
    const ids = new Set<string>();
    let n = 0;
    for (const z of filtered) {
      if (n >= PULSE_CAP) break;
      if (liveStateOf(z.status) === "live") {
        ids.add(z.id);
        n++;
      }
    }
    return ids;
  }, [filtered]);
  const overCap = liveTotal > PULSE_CAP;

  // `cursor` is passed in (narrowed to a non-null string by the `{cursor ? …}`
  // render guard on the trigger), so no in-function null check is needed.
  function loadMore(cursor: string) {
    setError(null);
    startTransition(async () => {
      const result = await listFleetsAction(workspaceId, { cursor });
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
            {liveTotal} live{overCap ? ` · capped at ${PULSE_CAP}` : ""}
          </span>
        ) : null}
        <Link href={workspacePath(workspaceId, "fleets/new")} className={buttonClassName("default", "sm")}>
          <PlusIcon size={14} /> Install fleet
        </Link>
      </div>

      {filtered.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No fleets match &ldquo;{deferredQuery}&rdquo; in the loaded set.
        </p>
      ) : (
        <List variant="plain" className="divide-y divide-border rounded-md border border-border space-y-0">
          {filtered.map((z) => (
            <ListItem key={z.id} className="p-0">
              <FleetRow fleet={z} workspaceId={workspaceId} pulses={cappedLiveIds.has(z.id)} />
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

type FleetRowProps = { fleet: Fleet; workspaceId: string; pulses: boolean };

function FleetRow({ fleet: z, workspaceId, pulses }: FleetRowProps) {
  const state = liveStateOf(z.status);
  return (
    <Link
      href={workspacePath(workspaceId, `fleets/${z.id}`)}
      className="grid grid-cols-12 gap-3 items-center px-4 py-3 transition-colors duration-snap ease-snap hover:bg-muted"
      data-state={state}
    >
      <div className="col-span-1 flex justify-start" aria-hidden="true">
        <StateDot state={state} pulses={pulses} />
      </div>
      <div className="col-span-7 sm:col-span-5 min-w-0">
        <div className="font-medium truncate">{z.name}</div>
        <div className={cn(EYEBROW_CLASS, "text-muted-foreground")}>
          {z.status}
        </div>
      </div>
      <div className="hidden sm:block sm:col-span-3 font-mono text-xs text-muted-foreground tabular-nums truncate">
        {z.id}
      </div>
      <div className="col-span-4 sm:col-span-3 font-mono text-xs text-muted-foreground tabular-nums text-right">
        <Time value={new Date(z.updated_at)} format="relative" tooltip={false} />
      </div>
    </Link>
  );
}

type StateDotProps = { state: LiveState; pulses: boolean };

function StateDot({ state, pulses }: StateDotProps) {
  if (state === "live") {
    return (
      <WakePulse
        live={pulses}
        className={
          pulses
            ? "inline-block w-2 h-2 rounded-full bg-pulse"
            : "inline-block w-2 h-2 rounded-full bg-pulse shadow-[0_0_0_3px_var(--pulse-glow)] opacity-70"
        }
      />
    );
  }
  // Installing: a live info-toned pulse so an in-flight install reads as active
  // progress (never hidden) without competing with the mint live signal.
  if (state === "installing") {
    return <WakePulse live className="inline-block w-2 h-2 rounded-full bg-info" />;
  }
  if (state === "failed") {
    return <span className="inline-block w-2 h-2 rounded-full bg-destructive" />;
  }
  return <span className="inline-block w-2 h-2 rounded-full bg-muted-foreground" />;
}

