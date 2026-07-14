"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { CronExpressionParser } from "cron-parser";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CopyButton,
  Time,
} from "@agentsfleet/design-system";
import type { FleetTrigger } from "@/lib/types";
import { workspacePath } from "@/lib/workspace-routes";

type Props = {
  trigger: Extract<FleetTrigger, { type: "cron" }>;
  workspaceId: string;
  fleetId: string;
};

type NextFire =
  | { ok: true; at: Date; tz: string }
  | { ok: false; reason: string };

function computeNextFire(schedule: string, now: Date): NextFire {
  // Resolves the IANA tz once per render. Falling back to "UTC" matters
  // only inside happy-dom test runs — real browsers always populate it.
  const tz = Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  try {
    const expr = CronExpressionParser.parse(schedule, { currentDate: now, tz });
    return { ok: true, at: expr.next().toDate(), tz };
  } catch (err) {
    return {
      ok: false,
      reason: err instanceof Error ? err.message : "unparseable",
    };
  }
}

export default function CronCard({ trigger, workspaceId, fleetId }: Props) {
  // `now` snapshots at mount so SSR + first client paint render the same
  // string; a `useEffect` ticker would invite hydration mismatches without
  // adding meaningful value on a cron whose cadence is minutes-scale.
  const [now] = useState<Date>(() => new Date(0));
  const [hydrated, setHydrated] = useState(false);
  useEffect(() => {
    setHydrated(true);
  }, []);

  const liveNow = useMemo(() => (hydrated ? new Date() : now), [hydrated, now]);
  const fire = useMemo(
    () => computeNextFire(trigger.schedule, liveNow),
    [trigger.schedule, liveNow],
  );

  return (
    <Card data-testid="cron-card" className="bg-card">
      <CardHeader className="gap-1">
        {/* Every other trigger type in this accordion offers a copy; cron was the
            one that did not, and its schedule is the field most likely to be
            transcribed into another tool. */}
        <CardTitle className="flex items-center gap-1 font-mono text-base">
          <span>Cron — {trigger.schedule}</span>
          <CopyButton value={trigger.schedule} label={`Copy schedule: ${trigger.schedule}`} />
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {fire.ok ? (
          <p
            className="font-sans text-sm text-muted-foreground"
            data-testid="cron-next-fire"
            suppressHydrationWarning
          >
            Next fire{" "}
            <strong className="font-mono text-foreground">
              <Time value={fire.at} format="relative" tooltip={false} />
            </strong>{" "}
            ({fire.tz}).
          </p>
        ) : (
          <p
            className="font-sans text-sm text-destructive"
            data-testid="cron-next-fire-error"
          >
            Schedule unparseable — check{" "}
            <code className="font-mono text-xs">TRIGGER.md</code>.
          </p>
        )}

        <p className="font-sans text-sm text-muted-foreground">
          Cron triggers are read-only in the Dashboard. Edit{" "}
          <code className="font-mono text-xs">TRIGGER.md</code> and reinstall to change
          the schedule.
        </p>

        <Link
          href={`${workspacePath(workspaceId, `fleets/${fleetId}`)}?actor=cron:*`}
          className="font-mono text-xs text-pulse hover:underline"
          data-testid="cron-deliveries-link"
        >
          View cron deliveries →
        </Link>
      </CardContent>
    </Card>
  );
}
