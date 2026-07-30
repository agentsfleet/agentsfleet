"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { Badge, Card, cn, Time } from "@agentsfleet/design-system";
import { LEASE_OUTCOME, type RunnerListItem } from "@/lib/api/runners";
import { runnerPath } from "@/lib/runner-routes";
import {
  IDLE_SENTENCE,
  INSPECT_RUNNER_LABEL,
  NEVER_CONNECTED_SENTENCE,
} from "../[runnerId]/components/runner-copy";
import { listRunnerLeasesAction } from "../actions";
import { DEGRADED_BADGE_LABEL, RunnerStatus, runnerIsAwake } from "./RunnerStatus";

// One card per runner, mirroring FleetTile's grammar: an absolutely-positioned
// whole-card link over pointer-events-none content, a bottom-bordered action
// affordance, and a parked look for hosts that are administratively out or
// unreachable. A runner is a host, not an agent, so it carries the shared
// server glyph in the same 56px bordered square — the design system scopes
// deterministic sigils to Fleet tiles.

// Only the newest page is consulted for the work line; a host running more
// live leases than one page holds reads as "many".
const WORK_LINE_LEASE_SCAN_LIMIT = 25;
const WORK_LINE_LOADING = "…";
const LEASES_NOUN_SINGULAR = "lease";
const LEASES_NOUN_PLURAL = "leases";

function ServerGlyph({ awake }: { awake: boolean }) {
  return (
    <div
      className={cn(
        "flex size-14 shrink-0 items-center justify-center rounded-md border bg-surface-2",
        awake ? "border-pulse/50 text-pulse" : "border-border text-muted-foreground",
      )}
      aria-hidden="true"
    >
      <svg viewBox="0 0 24 24" className="size-7" fill="none" stroke="currentColor" strokeWidth="1.4">
        <rect x="2" y="3" width="20" height="7" rx="2" />
        <rect x="2" y="14" width="20" height="7" rx="2" />
        <path d="M6 6.5h.01M6 17.5h.01" />
      </svg>
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
    if (!busy) {
      setLine(idleLineFor(runner));
      return;
    }
    let cancelled = false;
    void listRunnerLeasesAction(runner.id, { limit: WORK_LINE_LEASE_SCAN_LIMIT }).then((result) => {
      if (cancelled) return;
      if (!result.ok) {
        setLine(IDLE_SENTENCE);
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
    });
    return () => {
      cancelled = true;
    };
  }, [busy, runner]);

  return line;
}

function idleLineFor(runner: RunnerListItem): string {
  return runner.liveness === "registered" ? NEVER_CONNECTED_SENTENCE : IDLE_SENTENCE;
}

export default function RunnerTile({ runner }: { runner: RunnerListItem }) {
  const awake = runnerIsAwake(runner.admin_state, runner.liveness);
  const parked = runner.liveness === "offline" || runner.admin_state === "revoked";
  const workLine = useWorkLine(runner);
  return (
    <Card className={cn("min-h-44 p-xl", parked && "opacity-60")}>
      <Link
        href={runnerPath(runner.id)}
        className="absolute inset-0 rounded-lg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-label={`${INSPECT_RUNNER_LABEL}: ${runner.host_id} — ${runner.admin_state} ${runner.liveness}`}
      />
      <div className="pointer-events-none flex h-full flex-col gap-lg">
        <div className="flex items-start gap-xl">
          <ServerGlyph awake={awake} />
          <div className="min-w-0 flex-1">
            <div className="truncate font-mono text-body-sm font-medium">{runner.host_id}</div>
            <div className="mt-sm flex flex-wrap items-center gap-md">
              <RunnerStatus adminState={runner.admin_state} liveness={runner.liveness} />
              {/* A host that cannot deliver its assignment is visually distinct
                  and receives no work — the badge is the tile-level face of the
                  verdict; the reason line below names the missing mechanism. */}
              {runner.degraded ? <Badge variant="error">{DEGRADED_BADGE_LABEL}</Badge> : null}
            </div>
            {runner.degraded && runner.degraded_reason ? (
              <div className="mt-md min-h-5 truncate font-mono text-body-sm text-destructive">
                {runner.degraded_reason}
              </div>
            ) : (
              <div className="mt-md min-h-5 truncate font-mono text-body-sm text-muted-foreground">
                {workLine}
              </div>
            )}
          </div>
        </div>
        <div className="mt-auto flex items-center justify-between border-t border-border pt-lg font-mono text-label text-muted-foreground tabular-nums">
          <span>
            {runner.last_seen_at > 0 ? (
              <>
                heartbeat <Time value={new Date(runner.last_seen_at)} format="relative" tooltip={false} />
              </>
            ) : (
              NEVER_CONNECTED_SENTENCE
            )}
          </span>
          <span className="font-medium text-pulse">{INSPECT_RUNNER_LABEL} →</span>
        </div>
      </div>
    </Card>
  );
}
