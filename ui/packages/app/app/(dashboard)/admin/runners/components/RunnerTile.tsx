"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { ServerIcon } from "lucide-react";
import { Badge, Card, cn, Time } from "@agentsfleet/design-system";
import { type RunnerListItem } from "@/lib/api/runners";
import { LEASE_OUTCOME } from "@/lib/api/runners-types";
import { runnerPath } from "@/lib/runner-routes";
import {
  IDLE_SENTENCE,
  OPEN_RUNNER_LABEL,
  NEVER_CONNECTED_SENTENCE,
} from "../[runnerId]/components/runner-copy";
import { listRunnerLeasesAction } from "../actions";
import { DEGRADED_BADGE_LABEL, RunnerStatus, runnerIsAwake } from "./RunnerStatus";

// The whole card is a link; status and current work remain readable at rest.

// Only the newest page is consulted for the work line; a host running more
// live leases than one page holds reads as "many".
const WORK_LINE_LEASE_SCAN_LIMIT = 25;
const WORK_LINE_LOADING = "Loading activity…";
const WORK_LINE_UNAVAILABLE = "Activity unavailable. Open runner to retry.";
const LEASES_NOUN_SINGULAR = "lease";
const LEASES_NOUN_PLURAL = "leases";

function ServerGlyph({ awake }: { awake: boolean }) {
  return (
    <div
      className={cn(
        "flex size-11 shrink-0 items-center justify-center rounded-md border bg-surface-2",
        awake ? "border-pulse/50 text-pulse" : "border-border text-muted-foreground",
      )}
      aria-hidden="true"
    >
      <ServerIcon className="size-6" strokeWidth={1.75} />
    </div>
  );
}

// The one line saying what the host is doing right now. A busy host names its
// live work from the newest lease page; everything else derives honestly from
// the row itself — no fetch, no fabricated figures.
function useWorkLine(runner: RunnerListItem): string {
  const busy = runner.liveness === "busy";
  const [line, setLine] = useState<string>(busy ? WORK_LINE_LOADING : idleLineFor(runner));

  useEffect(() => {
    if (!busy) return;
    setLine(WORK_LINE_LOADING);
    let cancelled = false;
    void listRunnerLeasesAction(runner.id, { limit: WORK_LINE_LEASE_SCAN_LIMIT }).then((result) => {
      if (cancelled) return;
      if (!result.ok) {
        setLine(WORK_LINE_UNAVAILABLE);
        return;
      }
      const running = result.data.items.filter((lease) => lease.outcome === LEASE_OUTCOME.running);
      if (running.length === 0) {
        setLine(IDLE_SENTENCE);
        return;
      }
      const names = [...new Set(running.map((lease) => lease.fleet_name ?? lease.fleet_id))];
      const noun = running.length === 1 ? LEASES_NOUN_SINGULAR : LEASES_NOUN_PLURAL;
      setLine(`${running.length} ${noun} · ${names.join(", ")}`);
    }).catch(() => { if (!cancelled) setLine(WORK_LINE_UNAVAILABLE); });
    return () => { cancelled = true; };
  }, [busy, runner]);

  return busy ? line : idleLineFor(runner);
}

function idleLineFor(runner: RunnerListItem): string {
  return runner.liveness === "registered" ? NEVER_CONNECTED_SENTENCE : IDLE_SENTENCE;
}

export default function RunnerTile({ runner }: { runner: RunnerListItem }) {
  "use memo";
  const awake = runnerIsAwake(runner.admin_state, runner.liveness);
  const workLine = useWorkLine(runner);
  return (
    <Card asChild interactive className="group min-h-52 p-xl">
      <Link href={runnerPath(runner.id)}
        className="flex flex-col gap-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-label={`${OPEN_RUNNER_LABEL}: ${runner.host_id} — ${runner.admin_state} ${runner.liveness}`}>
        <div className="flex flex-wrap items-center justify-between gap-md">
          <ServerGlyph awake={awake} />
          <RunnerStatus adminState={runner.admin_state} liveness={runner.liveness} />
        </div>
        <div className="min-w-0">
          <h3 className="truncate font-sans text-body-lg font-semibold">{runner.host_id}</h3>
          {runner.degraded ? <Badge variant="error">{DEGRADED_BADGE_LABEL}</Badge> : null}
          <p className={cn("mt-sm truncate text-body-sm", runner.degraded && runner.degraded_reason ? "text-destructive" : "text-muted-foreground")}>
            {runner.degraded && runner.degraded_reason ? runner.degraded_reason : workLine}
          </p>
        </div>
        <div className="mt-auto flex flex-wrap items-center justify-between gap-md border-t border-border pt-lg text-label text-muted-foreground tabular-nums">
          <span>
            {runner.last_seen_at > 0 ? (
              <>Last seen <Time value={new Date(runner.last_seen_at)} format="relative" tooltip={false} /></>
            ) : NEVER_CONNECTED_SENTENCE}
          </span>
          <span className="shrink-0 font-sans text-label font-medium text-pulse" data-card-open>
            {OPEN_RUNNER_LABEL} →
          </span>
        </div>
      </Link>
    </Card>
  );
}
