"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { CronExpressionParser } from "cron-parser";
import { Card, CardContent, CardHeader, CardTitle, Time } from "@agentsfleet/design-system";
import type { ZombieTrigger } from "@/lib/types";

type Props = {
  trigger: Extract<ZombieTrigger, { type: "cron" }>;
  zombieId: string;
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

export default function CronCard({ trigger, zombieId }: Props) {
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
        <CardTitle className="font-mono text-base">
          Cron — {trigger.schedule}
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
          href={`/zombies/${zombieId}?actor=cron:*`}
          className="font-mono text-xs text-pulse hover:underline"
          data-testid="cron-deliveries-link"
        >
          View cron deliveries →
        </Link>
      </CardContent>
    </Card>
  );
}
